use std::sync::{Arc, mpsc};

use secs_local_api::{EventHub, LocalApiError, Runtime};
use secs_service::{LogLevel, Service};
use secs_types::ipc::{ErrorCode, IpcError, LocalApiSettings, LocalApiStatus};
use tauri::{AppHandle, Emitter};

#[derive(Debug)]
struct SettingsApplier {
    sender: mpsc::Sender<(LocalApiSettings, LocalApiSettings)>,
}

impl SettingsApplier {
    fn new(runtime: Arc<Runtime>) -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("secs-local-api-settings".into())
            .spawn(move || {
                while let Ok((old, new)) = receiver.recv() {
                    runtime.apply_settings(old, new);
                }
            })
            .expect("local API settings worker must start");
        Self { sender }
    }

    fn apply_settings(&self, old: LocalApiSettings, new: LocalApiSettings) {
        let _ = self.sender.send((old, new));
    }
}

pub(crate) struct Manager {
    runtime: Arc<Runtime>,
    settings: SettingsApplier,
}

impl std::fmt::Debug for Manager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalApiManager")
            .field("runtime", &self.runtime)
            .field("settings", &self.settings)
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
        let runtime = Arc::new(Runtime::new(
            config_dir,
            service,
            hub,
            on_change,
            runtime_log,
        ));
        let settings = SettingsApplier::new(runtime.clone());
        Self { runtime, settings }
    }

    pub(crate) fn startup(&self, settings: LocalApiSettings) {
        self.runtime.startup(settings);
    }

    /// The service's local-API settings effect (ADR 0051) lands here, once per
    /// saved revision and outside the settings lock.
    pub(crate) fn apply_settings(&self, old: LocalApiSettings, new: LocalApiSettings) {
        self.settings.apply_settings(old, new);
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
    fn settings_are_applied_on_a_worker_without_blocking_the_caller() {
        let root = std::env::temp_dir().join(format!(
            "secs2r-local-api-settings-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let runtime = Arc::new(Runtime::new(
            root.clone(),
            Arc::new(Service::new(secs_service::Deps::default())),
            EventHub::default(),
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        ));
        let disabled = LocalApiSettings::default();
        runtime.startup(disabled);
        let applier = SettingsApplier::new(runtime.clone());
        let enabled = LocalApiSettings {
            enabled: true,
            port: 0,
        };

        let started = std::time::Instant::now();
        applier.apply_settings(disabled, enabled);
        assert!(started.elapsed() < std::time::Duration::from_millis(100));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if runtime.status().unwrap().running {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "local API did not start"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        runtime.stop();
        drop(applier);
        std::fs::remove_dir_all(root).unwrap();
    }

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
