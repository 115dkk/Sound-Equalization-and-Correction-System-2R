use std::path::Path;

use secs_types::ipc::{ErrorCode, IpcError};

#[derive(Clone, Copy)]
pub(crate) enum LinkKind {
    Symlink,
    Any,
}

pub(crate) fn is_link_or_junction(metadata: &std::fs::Metadata, kind: LinkKind) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    if matches!(kind, LinkKind::Symlink) {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        // Win32 FILE_ATTRIBUTE_REPARSE_POINT (audit #155 TD-110).
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub(crate) fn io_failure(path: &Path, error: &std::io::Error, code: ErrorCode) -> IpcError {
    let detail = error.to_string();
    let lowercase = detail.to_ascii_lowercase();
    let denied = error.kind() == std::io::ErrorKind::PermissionDenied
        || lowercase.contains("permission denied")
        || lowercase.contains("access is denied");
    IpcError::new(
        if denied {
            ErrorCode::PermissionDenied
        } else {
            code
        },
        detail,
    )
    .with_param("path", path.to_string_lossy())
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8], code: ErrorCode) -> Result<(), IpcError> {
    secs_types::fs::write_atomic_bytes(path, bytes).map_err(|error| io_failure(path, &error, code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_denied_write_is_permission_denied_for_every_caller_td208() {
        let path = Path::new("settings.json");
        for code in [ErrorCode::ProjectIo, ErrorCode::ApplyFailed] {
            let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
            let error = io_failure(path, &denied, code);
            assert_eq!(error.code, ErrorCode::PermissionDenied);
            assert_eq!(error.params["path"], path.to_string_lossy());
        }

        let denied = std::io::Error::other("Access is denied. (os error 5)");
        let error = io_failure(path, &denied, ErrorCode::ProjectIo);
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert_eq!(error.params["path"], path.to_string_lossy());

        let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
        for code in [ErrorCode::ProjectIo, ErrorCode::ApplyFailed] {
            let error = io_failure(path, &missing, code);
            assert_eq!(error.code, code);
            assert_eq!(error.params["path"], path.to_string_lossy());
        }
    }
}
