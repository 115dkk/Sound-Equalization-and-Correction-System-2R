use std::sync::{Arc, Mutex, MutexGuard};

use secs_local_api::{EventHub, LocalApiError, Runtime};
use secs_service::{AppEffects, LogLevel, RevisedAppSettings, Service};
use secs_types::ipc::{ErrorCode, IpcError, LocalApiSettings, LocalApiStatus};
use tauri::{AppHandle, Emitter, Manager as TauriManager};

pub(crate) struct Manager {
    runtime: Runtime,
    app: AppHandle,
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
        let change_app = app.clone();
        let on_change = Arc::new(move |method: &str| {
            let _ = change_app.emit("secs://external-change", method.to_owned());
            if method == "app_settings_set" {
                let app = change_app.clone();
                let _ = std::thread::Builder::new()
                    .name("secs-local-api-settings".into())
                    .spawn(move || {
                        let service = app.state::<Arc<Service>>();
                        let revised = service.revised_app_settings();
                        app.state::<Manager>()
                            .apply_revision(app.state::<Mutex<AppEffects>>().inner(), revised);
                    });
            }
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
            app,
        }
    }

    pub(crate) fn startup(&self, settings: LocalApiSettings) {
        self.runtime.startup(settings);
    }

    pub(crate) fn apply_revision(&self, effects: &Mutex<AppEffects>, revised: RevisedAppSettings) {
        let mut effects = lock(effects);
        let Some(previous) = effects.accept(revised.revision, &revised.settings) else {
            return;
        };
        if previous.icon != revised.settings.icon {
            crate::icon::show(&self.app, revised.settings.icon);
        }
        if previous.local_api != revised.settings.local_api {
            self.runtime
                .apply_settings(previous.local_api, revised.settings.local_api);
        }
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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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
