use std::path::Path;
use std::time::Duration;

use secs_service::{ApplyAdapter, ManagedHostAdapter};
use secs_types::ipc::{
    ApplyOutcome, CamillaStatus, ErrorCode, HostRequest, HostResponse, IncludeState, IpcError,
    ServiceState,
};

#[derive(Debug)]
pub(crate) struct NativeApply;

impl ApplyAdapter for NativeApply {
    fn camilla_status(&self, address: &str, timeout: Duration) -> Result<CamillaStatus, IpcError> {
        secs_sys::camilla_status(address, timeout)
    }

    fn camilla_apply(
        &self,
        address: &str,
        config_path: &Path,
        config_text: &str,
        timeout: Duration,
    ) -> Result<ApplyOutcome, IpcError> {
        secs_sys::camilla_apply(address, config_path, config_text, timeout)
    }

    fn eqapo_include_state(&self, config_text: &str) -> IncludeState {
        secs_sys::eqapo_include_state(config_text)
    }

    fn eqapo_with_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        Ok(secs_sys::eqapo_with_include(config_text))
    }

    fn eqapo_without_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        Ok(secs_sys::eqapo_without_include(config_text))
    }
}

#[derive(Debug)]
pub(crate) struct NativeManagedHost;

impl ManagedHostAdapter for NativeManagedHost {
    fn service_state(&self) -> ServiceState {
        secs_sys::host::service_state()
    }

    fn call(&self, request: &HostRequest) -> Result<HostResponse, IpcError> {
        secs_sys::host::pipe_call(request)
    }

    fn install(&self) -> Result<(), IpcError> {
        secs_sys::host::install_service_elevated(&host_executable()?)
    }

    fn uninstall(&self) -> Result<(), IpcError> {
        secs_sys::host::uninstall_service_elevated(&host_executable()?)
    }
}

fn host_executable() -> Result<std::path::PathBuf, IpcError> {
    let current = std::env::current_exe().map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("find CLI executable: {error}"),
        )
    })?;
    Ok(current.with_file_name(if cfg!(target_os = "windows") {
        "secs2r-host.exe"
    } else {
        "secs2r-host"
    }))
}
