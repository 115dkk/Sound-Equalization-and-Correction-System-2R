//! REW executable discovery and launch at the operating-system boundary.

use std::path::Path;
use std::process::{Command, Stdio};

use secs_rew::RewError;
use secs_rew::launch::{Platform, find_rew_exe, launch_command};

/// Starts the installed REW executable in API mode without invoking a shell.
pub fn launch() -> Result<(), RewError> {
    let exe = find_rew_exe().ok_or_else(|| RewError::Connection {
        path: "launch".into(),
        message: "REW installation not found".into(),
    })?;
    launch_path(&exe, Platform::current())
}

fn launch_path(exe: &Path, platform: Platform) -> Result<(), RewError> {
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
    let mut child = command.spawn().map_err(|error| RewError::Connection {
        path: "launch".into(),
        message: error.to_string(),
    })?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
