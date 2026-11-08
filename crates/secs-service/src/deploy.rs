//! Three replacement conventions are deliberate: [`install`] installs a whole
//! generation folder without a backup, `design_export::replace_folder` replaces a
//! bundle with a backup, and `secs_graph::files::install` rolls back per file.

use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use secs_types::ipc::{ErrorCode, IpcError};

use crate::fs::is_link_or_junction;

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
    fs::create_dir_all(root)
        .map_err(|error| crate::fs::io_failure(root, &error, ErrorCode::ApplyFailed))?;
    refuse_link(root)?;
    for number in 1_u64.. {
        let name = format!("{prefix}-{number}");
        let dir = root.join(&name);
        match fs::symlink_metadata(&dir) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(crate::fs::io_failure(&dir, &error, ErrorCode::ApplyFailed)),
        }
        fs::rename(stage, &dir)
            .map_err(|error| crate::fs::io_failure(&dir, &error, ErrorCode::ApplyFailed))?;
        return Ok(Generation { name, dir });
    }
    unreachable!("u64 generation names are sufficient")
}

pub(crate) fn prune(root: &Path, keep: &[&str]) -> Result<(), IpcError> {
    refuse_link(root)?;
    for entry in fs::read_dir(root)
        .map_err(|error| crate::fs::io_failure(root, &error, ErrorCode::ApplyFailed))?
    {
        let entry =
            entry.map_err(|error| crate::fs::io_failure(root, &error, ErrorCode::ApplyFailed))?;
        if keep
            .iter()
            .any(|name| entry.file_name() == OsStr::new(name))
        {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| crate::fs::io_failure(&path, &error, ErrorCode::ApplyFailed))?;
        if is_link_or_junction(&metadata) {
            remove_link(&path)?;
        } else if metadata.is_dir() {
            fs::remove_dir_all(&path)
                .map_err(|error| crate::fs::io_failure(&path, &error, ErrorCode::ApplyFailed))?;
        } else {
            fs::remove_file(&path)
                .map_err(|error| crate::fs::io_failure(&path, &error, ErrorCode::ApplyFailed))?;
        }
    }
    Ok(())
}

pub(crate) fn write_entry(path: &Path, bytes: &[u8]) -> Result<(), IpcError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| crate::fs::io_failure(parent, &error, ErrorCode::ApplyFailed))?;
    }
    crate::fs::write_atomic(path, bytes, ErrorCode::ApplyFailed)
}

fn refuse_link(path: &Path) -> Result<(), IpcError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_link_or_junction(&metadata) => Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "deployment root must not be a link or junction",
        )
        .with_param("path", path.to_string_lossy())),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(crate::fs::io_failure(path, &error, ErrorCode::ApplyFailed)),
    }
}

fn remove_link(path: &Path) -> Result<(), IpcError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(file_error) => match fs::remove_dir(path) {
            Ok(()) => Ok(()),
            Err(_) => Err(crate::fs::io_failure(
                path,
                &file_error,
                ErrorCode::ApplyFailed,
            )),
        },
    }
}
