use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use secs_service::{LogLevel, Service};
use secs_types::ipc::{LocalApiSettings, LocalApiStatus};

use crate::{EventHub, LocalApiError, ServerConfig, ServerHandle, token};

/// Callback invoked after a successful mutating local API request.
pub type ChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;
/// Callback used for runtime lifecycle failures.
pub type RuntimeLog = Arc<dyn Fn(LogLevel, &str) + Send + Sync>;

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

/// Owns the local API token, settings, and server lifetime.
pub struct Runtime {
    config_dir: PathBuf,
    service: Arc<Service>,
    hub: EventHub,
    on_change: ChangeCallback,
    log: RuntimeLog,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Runtime")
            .field("config_dir", &self.config_dir)
            .field("service", &self.service)
            .field("hub", &self.hub)
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl Runtime {
    /// Creates a stopped runtime.
    pub fn new(
        config_dir: PathBuf,
        service: Arc<Service>,
        hub: EventHub,
        on_change: ChangeCallback,
        log: RuntimeLog,
    ) -> Self {
        Self {
            config_dir,
            service,
            hub,
            on_change,
            log,
            inner: Mutex::new(Inner {
                settings: LocalApiSettings::default(),
                server: None,
                token: None,
                error: None,
            }),
        }
    }

    /// Loads the token and starts the server when `settings.enabled` is true.
    pub fn startup(&self, settings: LocalApiSettings) {
        let mut inner = lock(&self.inner);
        inner.settings = settings;
        if let Err(error) = self.ensure_token(&mut inner) {
            inner.error = Some(error.to_string());
            (self.log)(LogLevel::Error, &error.to_string());
            return;
        }
        if settings.enabled {
            self.start_locked(&mut inner);
        }
    }

    /// Applies one enabled/port transition.
    pub fn apply_settings(&self, old: LocalApiSettings, new: LocalApiSettings) {
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

    /// Returns the configured and running status.
    pub fn status(&self) -> Result<LocalApiStatus, LocalApiError> {
        let mut inner = lock(&self.inner);
        self.ensure_token(&mut inner)?;
        Ok(self.status_locked(&inner))
    }

    /// Replaces the bearer token and restarts an enabled server.
    pub fn regenerate_token(&self) -> Result<LocalApiStatus, LocalApiError> {
        let token = token::regenerate(&self.config_dir)
            .map_err(|source| token_error(self.token_path(), source))?;
        let mut inner = lock(&self.inner);
        inner.token = Some(token);
        inner.error = None;
        Self::stop_locked(&mut inner);
        if inner.settings.enabled {
            self.start_locked(&mut inner);
        }
        Ok(self.status_locked(&inner))
    }

    /// Stops the server within its two-second shutdown bound.
    pub fn stop(&self) {
        Self::stop_locked(&mut lock(&self.inner));
    }

    fn ensure_token(&self, inner: &mut Inner) -> Result<(), LocalApiError> {
        if inner.token.is_none() {
            inner.token = Some(
                token::load_or_create(&self.config_dir)
                    .map_err(|source| token_error(self.token_path(), source))?,
            );
        }
        Ok(())
    }

    fn start_locked(&self, inner: &mut Inner) {
        if let Err(error) = self.ensure_token(inner) {
            inner.server = None;
            inner.error = Some(error.to_string());
            (self.log)(LogLevel::Error, &error.to_string());
            return;
        }
        let config = ServerConfig {
            port: inner.settings.port,
            token: inner.token.clone().expect("token was loaded"),
            service: self.service.clone(),
            hub: self.hub.clone(),
            on_change: self.on_change.clone(),
        };
        match crate::start(config) {
            Ok(server) => {
                inner.server = Some(server);
                inner.error = None;
            }
            Err(error) => {
                inner.server = None;
                inner.error = Some(error.to_string());
                (self.log)(LogLevel::Error, &error.to_string());
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
            token_path: self.token_path().to_string_lossy().into_owned(),
        }
    }

    fn token_path(&self) -> PathBuf {
        self.config_dir.join(secs_types::local_api::TOKEN_FILE)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Ok(inner) = self.inner.get_mut()
            && let Some(server) = inner.server.take()
        {
            server.stop();
        }
    }
}

fn token_error(path: impl AsRef<Path>, source: std::io::Error) -> LocalApiError {
    LocalApiError::Token {
        path: path.as_ref().to_path_buf(),
        source,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
