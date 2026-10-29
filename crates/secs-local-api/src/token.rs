//! Persistent bearer-token helpers.

use std::fs;
use std::io;
use std::path::Path;

use secs_types::local_api::TOKEN_FILE;

/// Loads a valid token or atomically replaces a missing or invalid token.
pub fn load_or_create(dir: &Path) -> io::Result<String> {
    let path = dir.join(TOKEN_FILE);
    if let Ok(bytes) = fs::read(&path)
        && valid(&bytes)
    {
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
    secs_types::fs::write_atomic_bytes(&dir.join(TOKEN_FILE), token.as_bytes())?;
    Ok(token)
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
