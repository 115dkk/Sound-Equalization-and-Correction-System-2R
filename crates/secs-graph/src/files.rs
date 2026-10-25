//! File helpers every export writer shares: the FIR WAV bytes, the upstream
//! filename sanitiser and the staged bundle install (#92 TD-22, TD-45).
//!
//! Moved from `secs-camilla` so the filter writers no longer borrow them
//! from the CamillaDSP writer (#92 C11).

use crate::GraphError;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Serialize an IEEE-float, mono, 32-bit WAV exactly like upstream `write_wav`.
///
/// Fails when a tap is not finite once narrowed to `f32` (`1e100` is a finite
/// `f64` but an infinite `f32`), so every writer that shares it rejects the
/// coefficients before its first file.
pub fn wav_bytes(data: &[f64], rate: u32) -> Result<Vec<u8>, GraphError> {
    if let Some(index) = data.iter().position(|value| !(*value as f32).is_finite()) {
        return Err(GraphError::NonFinite(format!("fir[{index}] as f32")));
    }
    let data_len = u32::try_from(data.len() * 4).expect("FIR WAV fits u32");
    let mut out = Vec::with_capacity(44 + data.len() * 4);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&3_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&32_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for value in data {
        out.extend_from_slice(&(*value as f32).to_le_bytes());
    }
    Ok(out)
}

/// Replace filename-unsafe or non-ASCII characters as upstream does.
pub fn safe_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii() && (ch.is_ascii_alphanumeric() || "-_.".contains(ch)) {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        "secs_calibration".into()
    } else {
        safe
    }
}

fn safe_bundle_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

/// Stage a complete bundle beside its destination, then install its entry file last.
#[doc(hidden)]
pub fn write_bundle_observed(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
    entry: &Path,
    after_stage_write: &dyn Fn(usize, &Path) -> std::io::Result<()>,
) -> Result<(), std::io::Error> {
    if !safe_bundle_path(entry)
        || !files.iter().any(|(path, _)| path == entry)
        || files.iter().any(|(path, _)| !safe_bundle_path(path))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "bundle paths must be relative file paths and include the entry",
        ));
    }
    let parent = root.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let folder = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("export");
    let mut staging = None;
    for _ in 0..100 {
        let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{folder}.stage-{}-{serial}", std::process::id()));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                staging = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let Some(staging) = staging else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create export staging directory",
        ));
    };
    let stage_result = (|| {
        use std::io::Write as _;
        for (index, (relative, bytes)) in files.iter().enumerate() {
            let path = staging.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            after_stage_write(index, &path)?;
        }
        Ok::<(), std::io::Error>(())
    })();
    if let Err(error) = stage_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    fs::create_dir_all(root)?;
    let install = |relative: &Path| -> Result<(), std::io::Error> {
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(staging.join(relative), destination)
    };
    let result = (|| {
        for (relative, _) in files.iter().filter(|(path, _)| path != entry) {
            install(relative)?;
        }
        install(entry)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}
