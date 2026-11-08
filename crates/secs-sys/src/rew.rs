//! REW executable discovery and launch at the operating-system boundary.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use secs_rew::launch::{Platform, find_rew_exe, launch_command};

/// Structured failures from executable discovery and process spawning.
#[derive(Debug)]
pub enum RewLaunchError {
    /// No installed executable was found.
    ExeNotFound,
    /// The executable the project names does not exist (audit #186 TD-221).
    ConfiguredExeMissing(PathBuf),
    /// The executable exists but the operating system refuses to start it.
    CannotLaunch(std::io::Error),
    /// Process creation failed for another reason.
    LaunchFailed(std::io::Error),
}

impl std::fmt::Display for RewLaunchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExeNotFound => formatter.write_str("REW installation not found"),
            Self::ConfiguredExeMissing(path) => write!(
                formatter,
                "configured REW executable not found: {}",
                path.display()
            ),
            Self::CannotLaunch(error) => write!(formatter, "cannot launch REW: {error}"),
            Self::LaunchFailed(error) => write!(formatter, "REW launch failed: {error}"),
        }
    }
}

impl std::error::Error for RewLaunchError {}

/// Starts REW in API mode without invoking a shell: the executable the
/// project names when there is one, otherwise the installed one discovery
/// finds. A named executable that is missing is reported with its path and
/// discovery is not tried, so the user learns the setting is wrong (TD-221).
pub fn launch_with(executable: Option<&Path>) -> Result<(), RewLaunchError> {
    let exe = match executable {
        Some(path) if path.is_file() => path.to_path_buf(),
        Some(path) => return Err(RewLaunchError::ConfiguredExeMissing(path.to_path_buf())),
        None => find_rew_exe().ok_or(RewLaunchError::ExeNotFound)?,
    };
    launch_path(&exe, Platform::current())
}

fn launch_path(exe: &Path, platform: Platform) -> Result<(), RewLaunchError> {
    let spec = launch_command(exe, platform);
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(cwd) = spec.cwd {
        command.current_dir(cwd);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported => {
            RewLaunchError::CannotLaunch(error)
        }
        _ => RewLaunchError::LaunchFailed(error),
    })?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{RewLaunchError, launch_with};

    #[test]
    fn a_configured_executable_that_is_missing_is_reported_with_its_path_td221() {
        let missing = std::env::temp_dir()
            .join(format!("secs-sys-rew-missing-{}", std::process::id()))
            .join("roomeqwizard.exe");
        match launch_with(Some(&missing)) {
            Err(RewLaunchError::ConfiguredExeMissing(path)) => assert_eq!(path, missing),
            other => panic!("expected the configured path to be reported: {other:?}"),
        }
    }
}
