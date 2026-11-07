use std::sync::Arc;

use secs_local_api::{EventHub, LocalApiError, Runtime};
use secs_service::{LogLevel, Service};
use secs_types::ipc::{ErrorCode, IpcError, LocalApiSettings, LocalApiStatus};
use tauri::{AppHandle, Emitter};

pub(crate) struct Manager {
    runtime: Runtime,
}

impl std::fmt::Debug for Manager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalApiManager")
            .field("runtime", &self.runtime)
            .finish_non_exhaustive()
    }
}

impl Manager {
    pub(crate) fn new(
        config_dir: std::path::PathBuf,
        service: Arc<Service>,
        hub: EventHub,
        app: AppHandle,
        log: crate::applog::AppLog,
    ) -> Self {
        let on_change = Arc::new(move |method: &str| {
            let _ = app.emit("secs://external-change", method.to_owned());
        });
        let runtime_log = Arc::new(move |level: LogLevel, text: &str| {
            let level = match level {
                LogLevel::Warn => crate::applog::Level::Warn,
                LogLevel::Error => crate::applog::Level::Error,
            };
            log.line(level, "local_api", text);
        });
        Self {
            runtime: Runtime::new(config_dir, service, hub, on_change, runtime_log),
        }
    }

    pub(crate) fn startup(&self, settings: LocalApiSettings) {
        self.runtime.startup(settings);
    }

    /// The service's local-API settings effect (ADR 0051) lands here, once per
    /// saved revision and outside the settings lock.
    pub(crate) fn apply_settings(&self, old: LocalApiSettings, new: LocalApiSettings) {
        self.runtime.apply_settings(old, new);
    }

    pub(crate) fn status(&self) -> Result<LocalApiStatus, IpcError> {
        self.runtime.status().map_err(local_error)
    }

    pub(crate) fn regenerate_token(&self) -> Result<LocalApiStatus, IpcError> {
        self.runtime.regenerate_token().map_err(local_error)
    }
}

fn local_error(error: LocalApiError) -> IpcError {
    match error {
        LocalApiError::Token { path, source } => IpcError::io(ErrorCode::ProjectIo, path, source),
        error => IpcError::new(ErrorCode::Internal, error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_token_folder_is_a_permission_error_td208() {
        let error = local_error(LocalApiError::Token {
            path: std::path::Path::new(r"C:\x\token").to_path_buf(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        });
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert_eq!(error.params["path"], r"C:\x\token");
        let error = local_error(LocalApiError::Token {
            path: std::path::Path::new("missing/token").to_path_buf(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        });
        assert_eq!(error.code, ErrorCode::ProjectIo);
        assert_eq!(error.params["path"], "missing/token");
    }
}
