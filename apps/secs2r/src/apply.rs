use std::path::Path;
use std::time::Duration;

use secs_service::ApplyAdapter;
use secs_types::ipc::{ApplyOutcome, CamillaStatus, IncludeState, IpcError};

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
