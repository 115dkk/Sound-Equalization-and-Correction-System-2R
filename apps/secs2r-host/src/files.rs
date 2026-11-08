use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use secs_types::ipc::{ErrorCode, HostLogLine, HostManaged, IpcError};

use crate::rules::{HostInstallPaths, HostUpdatePaths, UpdateReject};

pub const LOG_LIMIT: u64 = 256 * 1024;

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Checks the folder a caller names for `update_self`: a local folder on a
/// drive, never a UNC share, device path, link, or junction which the service
/// (SYSTEM) would otherwise open on the caller's behalf.
pub fn validate_update_source(from_dir: &Path) -> Result<(), UpdateReject> {
    let text = from_dir.to_string_lossy();
    let bytes = text.as_bytes();
    let drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    if !drive_absolute {
        return Err(UpdateReject::Io(format!(
            "the update folder must be a local absolute path: {text}"
        )));
    }

    for component in from_dir.ancestors().collect::<Vec<_>>().into_iter().rev() {
        match fs::symlink_metadata(component) {
            Ok(metadata) if metadata_is_link_or_reparse(&metadata) => {
                return Err(UpdateReject::Io(format!(
                    "the update folder must not contain a link or junction: {}",
                    component.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(UpdateReject::Io(format!(
                    "inspect update folder component {}: {error}",
                    component.display()
                )));
            }
        }
    }
    // This check and the later opens are not atomic. The active console user
    // can replace a component between them, but every installed executable is
    // still covered by the signed manifest and verified hash (audit #155 TD-135).
    Ok(())
}

/// Reads a regular file of at most `limit` bytes. The service reads these for
/// a caller, so a directory, a pipe or a device is refused before it is opened.
pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, UpdateReject> {
    let metadata = fs::metadata(path).map_err(|error| UpdateReject::Io(error.to_string()))?;
    if !metadata.is_file() {
        return Err(UpdateReject::Io(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    if metadata.len() > limit {
        return Err(UpdateReject::TooLarge);
    }
    let file = fs::File::open(path).map_err(|error| UpdateReject::Io(error.to_string()))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| UpdateReject::Io(error.to_string()))?;
    if bytes.len() as u64 > limit {
        Err(UpdateReject::TooLarge)
    } else {
        Ok(bytes)
    }
}

/// Writes the staged executable. Called only with bytes whose hash already
/// matched the signed manifest.
pub fn write_staged(destination: &Path, bytes: &[u8]) -> Result<(), UpdateReject> {
    match fs::remove_file(destination) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(UpdateReject::Io(error.to_string())),
    }
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|error| UpdateReject::Io(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| UpdateReject::Io(error.to_string()))?;
        file.sync_all()
            .map_err(|error| UpdateReject::Io(error.to_string()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

pub trait UpdateFileOps {
    fn remove_file(&mut self, path: &Path) -> std::io::Result<()>;
    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()>;
    fn wait_before_retry(&mut self, duration: Duration);
}

pub struct RealUpdateFileOps;

impl UpdateFileOps for RealUpdateFileOps {
    fn remove_file(&mut self, path: &Path) -> std::io::Result<()> {
        fs::remove_file(path)
    }

    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
        fs::rename(from, to)
    }

    fn wait_before_retry(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

fn rename_with_retry(
    files: &mut impl UpdateFileOps,
    from: &Path,
    to: &Path,
) -> std::io::Result<()> {
    const RETRIES: usize = 50;
    for attempt in 0..=RETRIES {
        match files.rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if attempt < RETRIES => {
                files.wait_before_retry(Duration::from_millis(100));
                let _ = error;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the retry loop always returns")
}

/// Replaces the stopped service executable and restores it if the second rename fails.
pub fn replace_staged_update(
    files: &mut impl UpdateFileOps,
    paths: &HostUpdatePaths,
) -> std::io::Result<()> {
    match files.remove_file(&paths.old) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    rename_with_retry(files, &paths.executable, &paths.old)?;
    if let Err(update_error) = rename_with_retry(files, &paths.staged, &paths.executable) {
        if let Err(restore_error) = rename_with_retry(files, &paths.old, &paths.executable) {
            return Err(std::io::Error::new(
                restore_error.kind(),
                format!("install new host: {update_error}; restore old host: {restore_error}"),
            ));
        }
        return Err(update_error);
    }
    Ok(())
}

pub fn replace_host_executable(source: &Path, paths: &HostInstallPaths) -> std::io::Result<()> {
    if source == paths.executable {
        return Ok(());
    }
    match fs::remove_file(&paths.staged) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.staged)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    drop(output);
    match fs::remove_file(&paths.executable) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(&paths.staged, &paths.executable)
}

pub fn remove_host_copy(paths: &HostInstallPaths) -> std::io::Result<()> {
    for path in [&paths.staged, &paths.executable] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    match fs::remove_dir(&paths.directory) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

pub fn load_state(path: &Path) -> Result<HostManaged, IpcError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("read {}: {error}", path.display()),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HostManaged::default()),
        Err(error) => Err(IpcError::new(
            ErrorCode::ApplyFailed,
            format!("read {}: {error}", path.display()),
        )),
    }
}

pub fn save_state(path: &Path, managed: &HostManaged) -> Result<(), IpcError> {
    let bytes = serde_json::to_vec_pretty(managed)
        .map_err(|error| IpcError::new(ErrorCode::Internal, error.to_string()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    secs_types::fs::write_atomic_bytes(path, &bytes).map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("write {}: {error}", path.display()),
        )
    })
}

pub fn load_logs(path: &Path, maximum: usize) -> Vec<HostLogLine> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut lines = text
        .lines()
        .filter_map(|line| serde_json::from_str::<HostLogLine>(line).ok())
        .collect::<Vec<_>>();
    if lines.len() > maximum {
        lines.drain(..lines.len() - maximum);
    }
    lines
}

pub fn append_log(path: &Path, line: &HostLogLine, limit: u64) -> Result<(), IpcError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    let encoded = serde_json::to_vec(line)
        .map_err(|error| IpcError::new(ErrorCode::Internal, error.to_string()))?;
    let incoming = u64::try_from(encoded.len().saturating_add(1)).unwrap_or(u64::MAX);
    let existing = fs::metadata(path).map_or(0, |metadata| metadata.len());
    if existing.saturating_add(incoming) > limit && existing != 0 {
        let rotated = path.with_extension("log.1");
        match fs::remove_file(&rotated) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(IpcError::new(
                    ErrorCode::ApplyFailed,
                    format!("remove {}: {error}", rotated.display()),
                ));
            }
        }
        fs::rename(path, &rotated).map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("rotate {}: {error}", path.display()),
            )
        })?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("open {}: {error}", path.display()),
            )
        })?;
    file.write_all(&encoded)
        .and_then(|()| file.write_all(b"\n"))
        .map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("append {}: {error}", path.display()),
            )
        })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use secs_types::ipc::{ManagedCamilla, ManagedEqApo};

    use super::*;
    use crate::rules::{
        InstallAction, UPDATE_STAGED_EXECUTABLE, host_install_paths, host_update_paths,
        install_action, log_line, sha256_hex,
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn root(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "secs2r-host-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn managed() -> HostManaged {
        HostManaged {
            camilla: Some(ManagedCamilla {
                exe_path: "camilladsp.exe".into(),
                config_path: "room.yml".into(),
                port: 1234,
                design_id: "camilla-design".into(),
            }),
            eqapo: Some(ManagedEqApo {
                config_path: "apo".into(),
                design_id: "apo-design".into(),
            }),
            owner_session_id: Some(7),
            owner_user_sid: Some("sid-a".into()),
        }
    }

    struct TestUpdateFiles {
        operations: Vec<String>,
        fail_staged_renames: usize,
    }

    impl TestUpdateFiles {
        fn operation(name: &str, first: &Path, second: Option<&Path>) -> String {
            match second {
                Some(second) => format!(
                    "{name}:{}:{}",
                    first.file_name().unwrap().to_string_lossy(),
                    second.file_name().unwrap().to_string_lossy()
                ),
                None => format!("{name}:{}", first.file_name().unwrap().to_string_lossy()),
            }
        }
    }

    impl UpdateFileOps for TestUpdateFiles {
        fn remove_file(&mut self, path: &Path) -> std::io::Result<()> {
            self.operations.push(Self::operation("remove", path, None));
            fs::remove_file(path)
        }

        fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
            self.operations
                .push(Self::operation("rename", from, Some(to)));
            if from.file_name() == Some(std::ffi::OsStr::new(UPDATE_STAGED_EXECUTABLE))
                && self.fail_staged_renames > 0
            {
                self.fail_staged_renames -= 1;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "injected lock",
                ));
            }
            fs::rename(from, to)
        }

        fn wait_before_retry(&mut self, _duration: Duration) {}
    }

    #[test]
    fn bounded_reads_take_regular_files_only_and_staging_writes_verified_bytes() {
        let root = root("bounded-read");
        let source = root.join("source.exe");
        let destination = root.join(UPDATE_STAGED_EXECUTABLE);
        fs::write(&source, b"host bytes").unwrap();
        let bytes = read_bounded(&source, 10).unwrap();
        assert_eq!(
            sha256_hex(&bytes),
            "43bcde663fb99877d335d8d1c06c25386bb8528a324218f1a9e8424f50d3cb03"
        );
        assert_eq!(read_bounded(&source, 9), Err(UpdateReject::TooLarge));
        assert!(matches!(
            read_bounded(&root, 1024),
            Err(UpdateReject::Io(_))
        ));
        assert!(matches!(
            read_bounded(&root.join("missing.exe"), 1024),
            Err(UpdateReject::Io(_))
        ));
        write_staged(&destination, &bytes).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"host bytes");
        write_staged(&destination, b"newer").unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"newer");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn update_source_must_be_a_local_drive_folder() {
        for accepted in [r"C:\Users\a\AppData\Local\SECS 2R", "D:/apps/SECS 2R"] {
            assert_eq!(
                validate_update_source(Path::new(accepted)),
                Ok(()),
                "{accepted}"
            );
        }
        for refused in [
            r"\\server\share\SECS 2R",
            r"\\.\pipe\secs2r",
            r"\\?\C:\SECS 2R",
            r"relative\SECS 2R",
            "C:",
            "",
        ] {
            assert!(
                matches!(
                    validate_update_source(Path::new(refused)),
                    Err(UpdateReject::Io(_))
                ),
                "{refused}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn update_source_refuses_an_existing_system_junction() {
        use std::os::windows::fs::MetadataExt;

        let junction = Path::new(r"C:\Documents and Settings");
        let Ok(metadata) = fs::symlink_metadata(junction) else {
            let message = format!(
                "skipping system junction test: {} is absent",
                junction.display()
            );
            assert!(std::env::var_os("GITHUB_ACTIONS").is_none(), "{message}");
            eprintln!("{message}");
            return;
        };
        if metadata.file_attributes() & 0x400 == 0 {
            let message = format!(
                "skipping system junction test: {} is not a reparse point",
                junction.display()
            );
            assert!(std::env::var_os("GITHUB_ACTIONS").is_none(), "{message}");
            eprintln!("{message}");
            return;
        }
        assert!(matches!(
            validate_update_source(&junction.join("bundle")),
            Err(UpdateReject::Io(_))
        ));
    }

    #[cfg(windows)]
    #[test]
    fn update_source_refuses_an_existing_directory_symlink() {
        use std::os::windows::fs::symlink_dir;

        let root = root("update-source-link");
        let target = root.join("real");
        let link = root.join("linked");
        fs::create_dir(&target).unwrap();
        if let Err(error) = symlink_dir(&target, &link) {
            let message = format!("skipping directory symlink test: {error}");
            fs::remove_dir_all(root).unwrap();
            assert!(std::env::var_os("GITHUB_ACTIONS").is_none(), "{message}");
            eprintln!("{message}");
            return;
        }
        assert!(matches!(
            validate_update_source(&link.join("bundle")),
            Err(UpdateReject::Io(_))
        ));
        fs::remove_dir(&link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn at_least_one_link_rejection_case_runs_td192() {
        use std::os::windows::fs::symlink_dir;

        let root = root("link-rejection-required");
        let target = root.join("real");
        let link = root.join("linked");
        fs::create_dir(&target).unwrap();
        if let Err(error) = symlink_dir(&target, &link) {
            fs::remove_dir_all(root).unwrap();
            panic!(
                "directory symlink creation failed: {error}; enable Windows Developer Mode so the link rejection tests can run"
            );
        }
        assert!(matches!(
            validate_update_source(&link.join("bundle")),
            Err(UpdateReject::Io(_))
        ));
        fs::remove_dir(&link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_update_replaces_in_order_and_retries_locked_rename() {
        let root = root("finish-success");
        let paths = host_update_paths(&root);
        fs::write(&paths.executable, b"old").unwrap();
        fs::write(&paths.staged, b"new").unwrap();
        fs::write(&paths.old, b"older").unwrap();
        let mut files = TestUpdateFiles {
            operations: Vec::new(),
            fail_staged_renames: 2,
        };

        replace_staged_update(&mut files, &paths).unwrap();

        assert_eq!(fs::read(&paths.executable).unwrap(), b"new");
        assert_eq!(fs::read(&paths.old).unwrap(), b"old");
        assert_eq!(
            &files.operations[..3],
            [
                "remove:secs2r-host.old.exe",
                "rename:secs2r-host.exe:secs2r-host.old.exe",
                "rename:secs2r-host.new.exe:secs2r-host.exe",
            ]
        );
        assert_eq!(
            files
                .operations
                .iter()
                .filter(|operation| {
                    operation.as_str() == "rename:secs2r-host.new.exe:secs2r-host.exe"
                })
                .count(),
            3
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_update_restores_old_executable_when_new_rename_never_succeeds() {
        let root = root("finish-restore");
        let paths = host_update_paths(&root);
        fs::write(&paths.executable, b"old").unwrap();
        fs::write(&paths.staged, b"new").unwrap();
        let mut files = TestUpdateFiles {
            operations: Vec::new(),
            fail_staged_renames: 51,
        };

        assert!(replace_staged_update(&mut files, &paths).is_err());

        assert_eq!(fs::read(&paths.executable).unwrap(), b"old");
        assert_eq!(fs::read(&paths.staged).unwrap(), b"new");
        assert!(!paths.old.exists());
        assert_eq!(
            files.operations.last().map(String::as_str),
            Some("rename:secs2r-host.old.exe:secs2r-host.exe")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn state_file_round_trip_missing_and_corrupt_handling() {
        let root = root("state");
        let path = root.join("host.json");
        assert_eq!(load_state(&path).unwrap(), HostManaged::default());
        save_state(&path, &managed()).unwrap();
        assert_eq!(load_state(&path).unwrap(), managed());
        fs::write(&path, b"{not json").unwrap();
        assert_eq!(load_state(&path).unwrap_err().code, ErrorCode::ApplyFailed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn install_paths_stage_then_replace_and_reinstall_updates() {
        let root = root("install-copy");
        let paths = host_install_paths(&root);
        assert_eq!(paths.directory, root.join("SECS 2R Host"));
        assert_eq!(paths.executable, paths.directory.join("secs2r-host.exe"));
        assert_eq!(paths.staged, paths.directory.join(".secs2r-host.exe.tmp"));
        assert_eq!(install_action(false), InstallAction::Register);
        assert_eq!(install_action(true), InstallAction::ReplaceAndUpdate);

        fs::create_dir(&paths.directory).unwrap();
        let source = root.join("source.exe");
        fs::write(&source, b"first").unwrap();
        replace_host_executable(&source, &paths).unwrap();
        assert_eq!(fs::read(&paths.executable).unwrap(), b"first");
        assert!(!paths.staged.exists());

        fs::write(&source, b"second").unwrap();
        replace_host_executable(&source, &paths).unwrap();
        assert_eq!(fs::read(&paths.executable).unwrap(), b"second");
        assert!(!paths.staged.exists());
        remove_host_copy(&paths).unwrap();
        assert!(!paths.directory.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn log_rotates_once_at_the_size_cap() {
        let root = root("log");
        let path = root.join("host.log");
        fs::write(&path, vec![b'x'; 64]).unwrap();
        append_log(&path, &log_line("launch", []), 64).unwrap();
        assert!(root.join("host.log.1").is_file());
        assert!(path.is_file());
        append_log(&path, &log_line("exit", []), 64).unwrap();
        assert!(root.join("host.log.1").is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
