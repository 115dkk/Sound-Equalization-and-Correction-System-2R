use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use secs_types::ipc::{ErrorCode, IpcError};

pub(crate) struct Generation {
    pub name: String,
    pub dir: PathBuf,
}

pub(crate) fn install(root: &Path, prefix: &str, stage: &Path) -> Result<Generation, IpcError> {
    let mut components = Path::new(prefix).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "generation prefix must be one file name",
        ));
    }
    refuse_link(root)?;
    fs::create_dir_all(root).map_err(|error| io_error(root, error))?;
    refuse_link(root)?;
    for number in 1_u64.. {
        let name = format!("{prefix}-{number}");
        let dir = root.join(&name);
        match fs::symlink_metadata(&dir) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&dir, error)),
        }
        fs::rename(stage, &dir).map_err(|error| io_error(&dir, error))?;
        return Ok(Generation { name, dir });
    }
    unreachable!("u64 generation names are sufficient")
}

pub(crate) fn prune(root: &Path, keep: &[&str]) -> Result<(), IpcError> {
    refuse_link(root)?;
    for entry in fs::read_dir(root).map_err(|error| io_error(root, error))? {
        let entry = entry.map_err(|error| io_error(root, error))?;
        if keep
            .iter()
            .any(|name| entry.file_name() == OsStr::new(name))
        {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|error| io_error(&path, error))?;
        } else {
            fs::remove_file(&path).map_err(|error| io_error(&path, error))?;
        }
    }
    Ok(())
}

pub(crate) fn write_entry(path: &Path, bytes: &[u8]) -> Result<(), IpcError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    secs_types::fs::write_atomic_bytes(path, bytes).map_err(|error| io_error(path, error))
}

fn refuse_link(path: &Path) -> Result<(), IpcError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "deployment root must not be a link or junction",
        )
        .with_param("path", path.to_string_lossy())),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(path, error)),
    }
}

fn io_error(path: &Path, error: impl std::fmt::Display) -> IpcError {
    IpcError::new(ErrorCode::ApplyFailed, error.to_string())
        .with_param("path", path.to_string_lossy())
}
