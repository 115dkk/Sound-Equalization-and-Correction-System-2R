use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use secs_local_api::{EventHub, ServerConfig, ServerHandle};
use secs_service::{NoEvents, Service};
use secs_types::ipc::{
    Envelope, ErrorCode, IpcError, LocalApiSettings, LocalApiStatus, Request, Response,
};
use tauri::{AppHandle, Emitter, Manager as TauriManager};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Start,
    Stop,
    Restart,
    Nothing,
}

fn lifecycle(old: LocalApiSettings, new: LocalApiSettings) -> Lifecycle {
    match (old.enabled, new.enabled) {
        (false, true) => Lifecycle::Start,
        (true, false) => Lifecycle::Stop,
        (true, true) if old.port != new.port => Lifecycle::Restart,
        _ => Lifecycle::Nothing,
    }
}

#[derive(Debug)]
struct Inner {
    settings: LocalApiSettings,
    server: Option<ServerHandle>,
    token: Option<String>,
    error: Option<String>,
}

pub(crate) struct Manager {
    config_dir: PathBuf,
    service: Arc<Service>,
    hub: EventHub,
    app: AppHandle,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for Manager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalApiManager")
            .field("config_dir", &self.config_dir)
            .field("service", &self.service)
            .field("hub", &self.hub)
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl Manager {
    pub(crate) fn new(
        config_dir: PathBuf,
        service: Arc<Service>,
        hub: EventHub,
        app: AppHandle,
    ) -> Self {
        Self {
            config_dir,
            service,
            hub,
            app,
            inner: Mutex::new(Inner {
                settings: LocalApiSettings::default(),
                server: None,
                token: None,
                error: None,
            }),
        }
    }

    pub(crate) fn startup(&self, settings: LocalApiSettings) {
        let mut inner = lock(&self.inner);
        inner.settings = settings;
        if settings.enabled {
            self.start_locked(&mut inner);
        }
    }

    pub(crate) fn apply_settings(&self, old: LocalApiSettings, new: LocalApiSettings) {
        let mut inner = lock(&self.inner);
        inner.settings = new;
        match lifecycle(old, new) {
            Lifecycle::Start => self.start_locked(&mut inner),
            Lifecycle::Stop => Self::stop_locked(&mut inner),
            Lifecycle::Restart => {
                Self::stop_locked(&mut inner);
                self.start_locked(&mut inner);
            }
            Lifecycle::Nothing => {}
        }
    }

    fn reconcile(&self, settings: LocalApiSettings) {
        let old = lock(&self.inner).settings;
        self.apply_settings(old, settings);
    }

    pub(crate) fn status(&self) -> Result<LocalApiStatus, IpcError> {
        let mut inner = lock(&self.inner);
        self.ensure_token(&mut inner)?;
        Ok(self.status_locked(&inner))
    }

    pub(crate) fn regenerate_token(&self) -> Result<LocalApiStatus, IpcError> {
        let token = secs_local_api::token::regenerate(&self.config_dir).map_err(local_error)?;
        let mut inner = lock(&self.inner);
        inner.token = Some(token);
        inner.error = None;
        if inner.server.is_some() {
            Self::stop_locked(&mut inner);
        }
        if inner.settings.enabled {
            self.start_locked(&mut inner);
        }
        Ok(self.status_locked(&inner))
    }

    fn ensure_token(&self, inner: &mut Inner) -> Result<(), IpcError> {
        if inner.token.is_none() {
            inner.token =
                Some(secs_local_api::token::load_or_create(&self.config_dir).map_err(local_error)?);
        }
        Ok(())
    }

    fn start_locked(&self, inner: &mut Inner) {
        if let Err(error) = self.ensure_token(inner) {
            inner.error = Some(error.detail);
            return;
        }
        let app = self.app.clone();
        let on_change = Arc::new(move |method: &str| {
            let _ = app.emit("secs://external-change", method.to_owned());
            if method == "app_settings_set" {
                let app = app.clone();
                let _ = std::thread::Builder::new()
                    .name("secs-local-api-settings".into())
                    .spawn(move || {
                        let service = app.state::<Arc<Service>>();
                        if let Envelope::Ok(response) =
                            service.call(Request::AppSettingsGet, Arc::new(NoEvents))
                            && let Response::AppSettingsGet(settings) = *response
                        {
                            app.state::<Manager>().reconcile(settings.local_api);
                        }
                    });
            }
        });
        let config = ServerConfig {
            port: inner.settings.port,
            token: inner.token.clone().expect("token was loaded"),
            service: self.service.clone(),
            hub: self.hub.clone(),
            on_change,
        };
        match secs_local_api::start(config) {
            Ok(server) => {
                inner.server = Some(server);
                inner.error = None;
            }
            Err(error) => {
                inner.server = None;
                inner.error = Some(error.to_string());
            }
        }
    }

    fn stop_locked(inner: &mut Inner) {
        if let Some(server) = inner.server.take() {
            server.stop();
        }
        inner.error = None;
    }

    fn status_locked(&self, inner: &Inner) -> LocalApiStatus {
        let port = inner
            .server
            .as_ref()
            .map_or(inner.settings.port, ServerHandle::port);
        LocalApiStatus {
            enabled: inner.settings.enabled,
            port,
            running: inner.server.is_some(),
            error: inner.error.clone(),
            url: format!("http://127.0.0.1:{port}"),
            token: inner.token.clone().unwrap_or_default(),
            token_path: self
                .config_dir
                .join(secs_types::local_api::TOKEN_FILE)
                .to_string_lossy()
                .into_owned(),
        }
    }
}

fn local_error(error: impl std::fmt::Display) -> IpcError {
    IpcError::new(ErrorCode::Internal, error.to_string())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(enabled: bool, port: u16) -> LocalApiSettings {
        LocalApiSettings { enabled, port }
    }

    #[test]
    fn local_api_lifecycle_follows_enabled_and_port_changes() {
        assert_eq!(
            lifecycle(settings(false, 1), settings(true, 1)),
            Lifecycle::Start
        );
        assert_eq!(
            lifecycle(settings(true, 1), settings(false, 1)),
            Lifecycle::Stop
        );
        assert_eq!(
            lifecycle(settings(true, 1), settings(true, 2)),
            Lifecycle::Restart
        );
        assert_eq!(
            lifecycle(settings(true, 1), settings(true, 1)),
            Lifecycle::Nothing
        );
        assert_eq!(
            lifecycle(settings(false, 1), settings(false, 2)),
            Lifecycle::Nothing
        );
        assert_eq!(
            lifecycle(settings(false, 1), settings(false, 1)),
            Lifecycle::Nothing
        );
    }
}
