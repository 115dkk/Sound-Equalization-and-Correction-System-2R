//! Native application adapters and the shared service dependency assembly.

use std::borrow::Cow;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use secs_service::{ApplyAdapter, Deps, ManagedHostAdapter};
use secs_types::ipc::{
    ApplyOutcome, CamillaStatus, ErrorCode, HostRequest, HostResponse, IncludeState, IpcError,
    ServiceState,
};

/// The managed-host shell that follows service installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellAutostart {
    pub executable: PathBuf,
}

impl ShellAutostart {
    #[must_use]
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
        }
    }
}

/// Native fields used to assemble one service dependency set.
#[derive(Debug, Clone)]
pub struct NativeEmbedding {
    pub app_version: Cow<'static, str>,
    pub app_data_path: PathBuf,
    pub shell: Option<ShellAutostart>,
}

#[derive(Debug, Default)]
pub struct OsApply;

impl ApplyAdapter for OsApply {
    fn camilla_status(&self, address: &str, timeout: Duration) -> Result<CamillaStatus, IpcError> {
        crate::camilla_status(address, timeout)
    }

    fn camilla_apply(
        &self,
        address: &str,
        config_path: &Path,
        config_text: &str,
        timeout: Duration,
    ) -> Result<ApplyOutcome, IpcError> {
        crate::camilla_apply(address, config_path, config_text, timeout)
    }

    fn camilla_apply_report(
        &self,
        address: &str,
        config_path: &Path,
        config_text: &str,
        timeout: Duration,
    ) -> Result<(ApplyOutcome, Option<String>), IpcError> {
        crate::camilla_apply_report(address, config_path, config_text, timeout)
    }

    fn eqapo_include_state(&self, config_text: &str) -> IncludeState {
        crate::eqapo_include_state(config_text)
    }

    fn eqapo_with_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        Ok(crate::eqapo_with_include(config_text))
    }

    fn eqapo_without_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        Ok(crate::eqapo_without_include(config_text))
    }
}

/// Operating-system commands behind [`OsManagedHost`].
#[doc(hidden)]
pub trait HostCommands: Send + Sync + Debug {
    fn service_state(&self) -> ServiceState;
    fn pipe_call(&self, request: &HostRequest) -> Result<HostResponse, IpcError>;
    fn install_service(&self, executable: &Path) -> Result<(), IpcError>;
    fn uninstall_service(&self, executable: &Path) -> Result<(), IpcError>;
    fn set_shell_autostart(&self, executable: Option<&Path>) -> Result<(), IpcError>;
    fn spawn_detached(&self, executable: &Path, args: &[&str]) -> Result<(), IpcError>;
}

#[derive(Debug, Default)]
struct NativeHostCommands;

impl HostCommands for NativeHostCommands {
    fn service_state(&self) -> ServiceState {
        crate::host::service_state()
    }

    fn pipe_call(&self, request: &HostRequest) -> Result<HostResponse, IpcError> {
        crate::host::pipe_call(request)
    }

    fn install_service(&self, executable: &Path) -> Result<(), IpcError> {
        crate::host::install_service_elevated(executable)
    }

    fn uninstall_service(&self, executable: &Path) -> Result<(), IpcError> {
        crate::host::uninstall_service_elevated(executable)
    }

    fn set_shell_autostart(&self, executable: Option<&Path>) -> Result<(), IpcError> {
        crate::host::set_shell_autostart(executable)
    }

    fn spawn_detached(&self, executable: &Path, args: &[&str]) -> Result<(), IpcError> {
        crate::host::spawn_detached(executable, args)
    }
}

#[derive(Debug)]
pub struct OsManagedHost {
    host_executable: Result<PathBuf, IpcError>,
    shell: Option<ShellAutostart>,
    commands: Arc<dyn HostCommands>,
}

impl OsManagedHost {
    #[must_use]
    pub fn new(host_executable: PathBuf, shell: Option<ShellAutostart>) -> Self {
        Self {
            host_executable: Ok(host_executable),
            shell,
            commands: Arc::new(NativeHostCommands),
        }
    }

    fn unavailable(error: IpcError, shell: Option<ShellAutostart>) -> Self {
        Self {
            host_executable: Err(error),
            shell,
            commands: Arc::new(NativeHostCommands),
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn with_commands(
        host_executable: PathBuf,
        shell: Option<ShellAutostart>,
        commands: Arc<dyn HostCommands>,
    ) -> Self {
        Self {
            host_executable: Ok(host_executable),
            shell,
            commands,
        }
    }

    #[must_use]
    pub fn service_state(&self) -> ServiceState {
        self.commands.service_state()
    }

    pub fn call(&self, request: &HostRequest) -> Result<HostResponse, IpcError> {
        self.commands.pipe_call(request)
    }

    pub fn install(&self) -> Result<(), IpcError> {
        let host = self
            .host_executable
            .as_ref()
            .map_err(|error| error.clone())?;
        self.commands.install_service(host)?;
        if let Some(shell) = &self.shell {
            self.commands.set_shell_autostart(Some(&shell.executable))?;
            self.commands
                .spawn_detached(&shell.executable, &["--hidden"])?;
        }
        Ok(())
    }

    pub fn uninstall(&self) -> Result<(), IpcError> {
        let host = self
            .host_executable
            .as_ref()
            .map_err(|error| error.clone())?;
        self.commands.uninstall_service(host)?;
        if self.shell.is_some() {
            self.commands.set_shell_autostart(None)?;
        }
        Ok(())
    }
}

impl ManagedHostAdapter for OsManagedHost {
    fn service_state(&self) -> ServiceState {
        self.service_state()
    }

    fn call(&self, request: &HostRequest) -> Result<HostResponse, IpcError> {
        self.call(request)
    }

    fn install(&self) -> Result<(), IpcError> {
        self.install()
    }

    fn uninstall(&self) -> Result<(), IpcError> {
        self.uninstall()
    }
}

#[must_use]
pub fn native_deps(embedding: NativeEmbedding) -> Deps {
    let managed_host = match application_executable() {
        Ok(current) => OsManagedHost::new(host_executable(&current), embedding.shell),
        Err(error) => OsManagedHost::unavailable(error, embedding.shell),
    };
    Deps {
        app_version: embedding.app_version,
        app_data_path: embedding.app_data_path,
        audio_backend: Arc::new(crate::audio::default_backend),
        rew_transport: Arc::new(|| Box::new(secs_rew::HttpTransport::default())),
        process: Arc::new(crate::process::OsProcessProbe),
        memory: Arc::new(crate::memory::OsMemoryProbe),
        eqapo_install: Arc::new(crate::eqapo_install),
        apply: Arc::new(OsApply),
        managed_host: Arc::new(managed_host),
        clock: Arc::new(crate::clock::local_clock),
        clock_offset_seconds: Arc::new(crate::clock::local_offset_seconds),
        unix_ms: Arc::new(crate::clock::unix_ms),
        ..Deps::default()
    }
}

pub fn application_executable() -> Result<PathBuf, IpcError> {
    std::env::current_exe().map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("find application executable: {error}"),
        )
    })
}

#[must_use]
pub fn host_executable(current: &Path) -> PathBuf {
    current.with_file_name(if cfg!(target_os = "windows") {
        "secs2r-host.exe"
    } else {
        "secs2r-host"
    })
}

#[must_use]
pub fn shell_executable(current: &Path) -> PathBuf {
    current.with_file_name(if cfg!(target_os = "windows") {
        "secs2r-host-shell.exe"
    } else {
        "secs2r-host-shell"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::UNIX_EPOCH;

    #[derive(Debug, Default)]
    struct RecordingHost {
        actions: Mutex<Vec<String>>,
    }

    impl RecordingHost {
        fn actions(&self) -> Vec<String> {
            self.actions.lock().unwrap().clone()
        }

        fn push(&self, action: impl Into<String>) {
            self.actions.lock().unwrap().push(action.into());
        }
    }

    impl HostCommands for RecordingHost {
        fn service_state(&self) -> ServiceState {
            ServiceState::NotInstalled
        }

        fn pipe_call(&self, _request: &HostRequest) -> Result<HostResponse, IpcError> {
            unreachable!("install test does not call the host pipe")
        }

        fn install_service(&self, executable: &Path) -> Result<(), IpcError> {
            self.push(format!("install:{}", executable.display()));
            Ok(())
        }

        fn uninstall_service(&self, executable: &Path) -> Result<(), IpcError> {
            self.push(format!("uninstall:{}", executable.display()));
            Ok(())
        }

        fn set_shell_autostart(&self, executable: Option<&Path>) -> Result<(), IpcError> {
            self.push(format!(
                "autostart:{}",
                executable.map_or_else(|| "off".into(), |path| path.display().to_string())
            ));
            Ok(())
        }

        fn spawn_detached(&self, executable: &Path, args: &[&str]) -> Result<(), IpcError> {
            self.push(format!("spawn:{}:{}", executable.display(), args.join(" ")));
            Ok(())
        }
    }

    #[test]
    fn native_deps_keeps_the_clock_and_its_offset_consistent() {
        let deps = native_deps(NativeEmbedding {
            app_version: "test".into(),
            app_data_path: PathBuf::from("data"),
            shell: None,
        });
        let local = (deps.clock)();
        let utc = local.shift_seconds(-(deps.clock_offset_seconds)());
        let from_unix = secs_service::LocalTime::from_system(
            UNIX_EPOCH + Duration::from_millis((deps.unix_ms)().unsigned_abs()),
            0,
        );
        let one_second_before = from_unix.shift_seconds(-1);
        assert!(
            utc == from_unix || utc == one_second_before,
            "{utc:?} {from_unix:?}"
        );
    }

    #[test]
    fn managed_host_install_touches_the_shell_only_when_asked() {
        let without_commands = Arc::new(RecordingHost::default());
        let without =
            OsManagedHost::with_commands(PathBuf::from("host"), None, without_commands.clone());
        without.install().unwrap();
        assert_eq!(without_commands.actions(), ["install:host"]);

        let with_commands = Arc::new(RecordingHost::default());
        let with = OsManagedHost::with_commands(
            PathBuf::from("host"),
            Some(ShellAutostart::new("shell")),
            with_commands.clone(),
        );
        with.install().unwrap();
        assert_eq!(
            with_commands.actions(),
            ["install:host", "autostart:shell", "spawn:shell:--hidden"]
        );
    }
}
