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
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| crate::fs::io_failure(path, &error, ErrorCode::ApplyFailed))?;
    let result = if metadata.is_dir() {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    };
    result.map_err(|error| crate::fs::io_failure(path, &error, ErrorCode::ApplyFailed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "secs-service-deploy-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn install_refuses_a_prefix_that_is_not_one_file_name_td202() {
        let temp = TempDir::new();
        let root = temp.0.join("root");
        for prefix in ["a/b", "..", ""] {
            let stage = temp
                .0
                .join(format!("stage-{}", prefix.replace(['/', '.'], "_")));
            fs::create_dir(&stage).unwrap();
            let error = match install(&root, prefix, &stage) {
                Err(error) => error,
                Ok(_) => panic!("accepted prefix {prefix:?}"),
            };
            assert_eq!(error.code, ErrorCode::InvalidRequest);
            assert_eq!(error.detail, "generation prefix must be one file name");
            assert!(stage.is_dir(), "stage was changed for prefix {prefix:?}");
        }
    }

    #[test]
    fn remove_link_removes_a_file_or_an_empty_directory_td202() {
        let temp = TempDir::new();

        let file = temp.0.join("file");
        fs::write(&file, b"file").unwrap();
        remove_link(&file).unwrap();
        assert!(!file.exists());

        let empty = temp.0.join("empty");
        fs::create_dir(&empty).unwrap();
        remove_link(&empty).unwrap();
        assert!(!empty.exists());

        let nonempty = temp.0.join("nonempty");
        fs::create_dir(&nonempty).unwrap();
        fs::write(nonempty.join("held"), b"held").unwrap();
        let error = remove_link(&nonempty).unwrap_err();
        assert_eq!(error.code, ErrorCode::ApplyFailed);
        assert_eq!(error.params["path"], nonempty.to_string_lossy());
        assert!(nonempty.is_dir());
    }
}
