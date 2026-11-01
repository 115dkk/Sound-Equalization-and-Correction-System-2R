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
    before_install: &dyn Fn(usize, &Path) -> std::io::Result<()>,
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
    let mut backup = None;
    for _ in 0..100 {
        let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{folder}.backup-{}-{serial}", std::process::id()));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                backup = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(error);
            }
        }
    }
    let Some(backup) = backup else {
        let _ = fs::remove_dir_all(&staging);
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create export backup directory",
        ));
    };

    let mut moved = Vec::<(PathBuf, bool, bool)>::new();
    let install_order = files
        .iter()
        .filter(|(path, _)| path != entry)
        .map(|(path, _)| path.as_path())
        .chain(std::iter::once(entry));
    let result = (|| {
        for (index, relative) in install_order.enumerate() {
            let destination = root.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            let backup_path = backup.join(relative);
            let had_old = fs::symlink_metadata(&destination).is_ok();
            if had_old {
                if let Some(parent) = backup_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&destination, &backup_path)?;
            }
            moved.push((relative.to_path_buf(), had_old, false));
            before_install(index, &destination)?;
            fs::rename(staging.join(relative), &destination)?;
            moved.last_mut().expect("just pushed").2 = true;
        }
        Ok::<(), std::io::Error>(())
    })();

    if let Err(error) = result {
        let rollback = (|| {
            for (relative, had_old, installed) in moved.iter().rev() {
                let destination = root.join(relative);
                if *installed {
                    fs::remove_file(&destination)?;
                }
                if *had_old {
                    fs::rename(backup.join(relative), &destination)?;
                }
            }
            Ok::<(), std::io::Error>(())
        })();
        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(&backup);
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback) => Err(std::io::Error::new(
                rollback.kind(),
                format!("install failed ({error}); rollback failed ({rollback})"),
            )),
        };
    }

    let _ = fs::remove_dir_all(&staging);
    fs::remove_dir_all(&backup)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("secs-graph-{name}-{}-{serial}", std::process::id()))
    }

    #[test]
    fn failed_second_install_restores_every_old_file() {
        let parent = temp_dir("install-rollback");
        let root = parent.join("bundle");
        fs::create_dir_all(root.join("coeffs")).unwrap();
        fs::write(root.join("coeffs/L.wav"), b"old coefficient").unwrap();
        fs::write(root.join("config.txt"), b"old config").unwrap();
        let files = vec![
            (PathBuf::from("coeffs/L.wav"), b"new coefficient".to_vec()),
            (PathBuf::from("config.txt"), b"new config".to_vec()),
        ];

        let error = write_bundle_observed(
            &root,
            &files,
            Path::new("config.txt"),
            &|_, _| Ok(()),
            &|index, _| {
                if index == 1 {
                    Err(std::io::Error::other("injected second install failure"))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("injected second install failure")
        );
        assert_eq!(
            fs::read(root.join("coeffs/L.wav")).unwrap(),
            b"old coefficient"
        );
        assert_eq!(fs::read(root.join("config.txt")).unwrap(), b"old config");
        fs::remove_dir_all(parent).unwrap();
    }
}
