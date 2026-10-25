//! Atomic file replacement shared by every writer (#92 TD-74).
//!
//! It uses only `std` and lives in this lowest crate so the storage crate,
//! the export writers and the service share one implementation without a
//! new dependency edge (docs/ARCHITECTURE.md section 3).

use std::borrow::Cow;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Names tried before giving up when every candidate already exists.
const ATTEMPTS: usize = 100;
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Replaces `path` with the bytes `write` puts into a sibling temporary file.
///
/// The temporary file is `.{name}.tmp-{pid}-{counter}` in the destination's
/// folder, created with `create_new` (another name is tried when one exists).
/// After `write` returns it is flushed with `sync_all`, closed and renamed over
/// `path`; std's rename replaces an existing destination on every platform.
/// On any error the temporary file is removed and `path` keeps its previous
/// content. The destination's folder must already exist.
pub fn write_atomic<F>(path: &Path, write: F) -> io::Result<()>
where
    F: FnOnce(&mut File) -> io::Result<()>,
{
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map_or(Cow::Borrowed("file"), |name| name.to_string_lossy());
    let (temporary, mut file) = create_sibling(parent, &name)?;
    let result = (|| {
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// [`write_atomic`] for bytes already in memory.
pub fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic(path, |file| file.write_all(bytes))
}

fn create_sibling(parent: &Path, name: &str) -> io::Result<(PathBuf, File)> {
    let pid = std::process::id();
    for _ in 0..ATTEMPTS {
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.tmp-{pid}-{counter}"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("no free temporary name for {name} in {}", parent.display()),
    ))
}
