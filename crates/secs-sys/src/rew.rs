//! REW executable discovery and launch at the operating-system boundary.

use std::path::Path;
use std::process::{Command, Stdio};

use secs_rew::launch::{Platform, find_rew_exe, launch_command};

/// Structured failures from executable discovery and process spawning.
#[derive(Debug)]
pub enum RewLaunchError {
    /// No installed executable was found.
    ExeNotFound,
    /// The executable exists but the operating system refuses to start it.
    CannotLaunch(std::io::Error),
    /// Process creation failed for another reason.
    LaunchFailed(std::io::Error),
}

impl std::fmt::Display for RewLaunchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExeNotFound => formatter.write_str("REW installation not found"),
            Self::CannotLaunch(error) => write!(formatter, "cannot launch REW: {error}"),
            Self::LaunchFailed(error) => write!(formatter, "REW launch failed: {error}"),
        }
    }
}

impl std::error::Error for RewLaunchError {}

/// Starts the installed REW executable in API mode without invoking a shell.
pub fn launch() -> Result<(), RewLaunchError> {
    let exe = find_rew_exe().ok_or(RewLaunchError::ExeNotFound)?;
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
    #[cfg(target_os = "windows")]
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
