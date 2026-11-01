//! Persistent bearer-token helpers.

use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::fs::OpenOptions;
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};

use secs_types::local_api::TOKEN_FILE;

/// Loads a valid token or atomically replaces a missing or invalid token.
pub fn load_or_create(dir: &Path) -> io::Result<String> {
    let path = dir.join(TOKEN_FILE);
    if let Ok(bytes) = fs::read(&path)
        && valid(&bytes)
    {
        secure_permissions(&path)?;
        return String::from_utf8(bytes).map_err(io::Error::other);
    }
    regenerate(dir)
}

/// Generates and atomically stores a fresh 256-bit token.
pub fn regenerate(dir: &Path) -> io::Result<String> {
    fs::create_dir_all(dir)?;
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let mut token = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in random {
        token.push(char::from(HEX[usize::from(byte >> 4)]));
        token.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    write_token(&dir.join(TOKEN_FILE), token.as_bytes())?;
    Ok(token)
}

#[cfg(unix)]
fn secure_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::metadata(path)?.permissions();
    if permissions.mode() & 0o777 != 0o600 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_permissions(_path: &Path) -> io::Result<()> {
    // Windows inherits the access-control list of the user's profile folder.
    Ok(())
}

#[cfg(unix)]
fn write_token(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map_or_else(|| "token".into(), |name| name.to_string_lossy());
    for _ in 0..100 {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".{name}.tmp-{}-{id}", std::process::id()));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = (|| {
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a token staging file",
    ))
}

#[cfg(not(unix))]
fn write_token(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // Windows inherits the access-control list of the user's profile folder.
    secs_types::fs::write_atomic_bytes(path, bytes)
}

fn valid(bytes: &[u8]) -> bool {
    bytes.len() == 64
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    fn root() -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "secs-local-api-token-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[cfg(unix)]
    #[test]
    fn token_files_are_created_and_tightened_to_owner_only_td109() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let root = root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join(TOKEN_FILE);
        let token = load_or_create(&root).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(load_or_create(&root).unwrap(), token);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let probe = root.join("umask-probe");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o666)
            .open(&probe)
            .unwrap();
        assert_ne!(
            fs::metadata(probe).unwrap().permissions().mode() & 0o777,
            0o600,
            "the test process must have a permissive enough umask to expose default creation"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn token_is_persistent_lowercase_hex_and_invalid_files_are_replaced() {
        let root = root();
        let first = load_or_create(&root).unwrap();
        assert_eq!(first.len(), 64);
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert_eq!(load_or_create(&root).unwrap(), first);

        fs::write(root.join(TOKEN_FILE), b"ABC").unwrap();
        let replacement = load_or_create(&root).unwrap();
        assert_ne!(replacement, first);
        assert!(valid(replacement.as_bytes()));

        let regenerated = regenerate(&root).unwrap();
        assert_ne!(regenerated, replacement);
        assert_eq!(
            fs::read_to_string(root.join(TOKEN_FILE)).unwrap(),
            regenerated
        );
        fs::remove_dir_all(root).unwrap();
    }
}
