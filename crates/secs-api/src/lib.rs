#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Supported Rust API for [SECS 2R](https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R),
//! a room-correction system designed from loudspeaker measurements.
//!
//! Add the crate under the short name used in these examples:
//!
//! ```text
//! cargo add sound-equalization-and-correction-system-rust-api --rename secs
//! ```
//!
//! # Features
//!
//! * `engine` (default) embeds the file-based correction and export service.
//! * `client` controls a running SECS 2R desktop app through its authenticated
//!   loopback HTTP and WebSocket API.
//!
//! Both implementations accept the same [`Request`] values through [`Secs`].
//!
//! | Included in this public API | Not included |
//! | --- | --- |
//! | Project files, imported measurements, calibration, MIMO, results, and exports | Native audio-device measurement |
//! | CPU calculation | GPU acceleration |
//! | An injectable [`secs_audio::AudioBackend`] with `engine` | SECS 2R's operating-system audio adapters |
//! | A client for the running app with `client` | Desktop screens and dialogs |
//!
//! # Licence
//!
//! This crate and the internal `secs-*` crates it depends on are
//! AGPL-3.0-only. A program that links this crate forms a combined work and
//! must be distributed under AGPL-3.0-only-compatible terms when it is conveyed
//! to others, and a modified version that users interact with over a network
//! must offer those users its source code (AGPL section 13). The SECS 2R
//! desktop app remains GPL-3.0-only.
//!
//! # Embedded engine example
//!
//! ```no_run
//! # use sound_equalization_and_correction_system_rust_api as secs;
//! use std::time::Duration;
//! use secs::types::ipc::{
//!     CalibrateParams, ExportKind, ExportParams, JobEvent, ProjectPathParams,
//! };
//! use secs::{Engine, Request, Response, Secs};
//!
//! # fn main() -> secs::Result<()> {
//! let engine = Engine::new();
//! engine.call(Request::ProjectOpen(ProjectPathParams {
//!     path: "room.secs".into(),
//! }))?;
//!
//! let Response::Calibrate(started) =
//!     engine.call(Request::Calibrate(CalibrateParams::default()))?
//! else {
//!     unreachable!("calibrate has one response variant")
//! };
//! let JobEvent::Finished { .. } =
//!     engine.wait_job(started.job, Duration::from_secs(120))?
//! else {
//!     unreachable!("wait_job returns a terminal event")
//! };
//!
//! let Response::Results(results) = engine.call(Request::Results)? else {
//!     unreachable!("results has one response variant")
//! };
//! assert!(results.calibration.is_some());
//!
//! let Response::Export(started) = engine.call(Request::Export(ExportParams {
//!     kind: ExportKind::CamillaDsp,
//!     folder: "camilladsp".into(),
//!     mdat_scope: None,
//!     design_name: Some("Living room".into()),
//!     camilla_devices: None,
//!     extra_rates: Vec::new(),
//! }))? else {
//!     unreachable!("export has one response variant")
//! };
//! engine.wait_job(started.job, Duration::from_secs(120))?;
//! # Ok(())
//! # }
//! ```
//!
//! # Running-app client example
//!
//! ```no_run
//! # use sound_equalization_and_correction_system_rust_api as secs;
//! # #[cfg(feature = "client")]
//! # fn main() -> secs::Result<()> {
//! use secs::types::local_api::{DEFAULT_PORT, ServerEvent};
//! use secs::Client;
//!
//! let client = Client::from_token_file(DEFAULT_PORT, Client::default_token_path())?;
//! let status = client.status()?;
//! println!("{} {}", status.app_name, status.app_version);
//!
//! for event in client.events()?.take(10) {
//!     match event? {
//!         ServerEvent::Job { event } => println!("job: {event:?}"),
//!         ServerEvent::Changed { method } => println!("changed: {method}"),
//!     }
//! }
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "client"))]
//! # fn main() {}
//! ```

use std::fmt;

pub use secs_types as types;
pub use secs_types::ipc::{ErrorCode, IpcError, JobEvent, Request, Response};

/// An error returned by the embedded engine or local-API client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A structured error returned by the SECS service.
    Api(IpcError),
    /// An HTTP, WebSocket, file, or socket operation failed.
    Transport(String),
    /// A peer returned data that does not follow the local-API protocol.
    Protocol(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(error) => write!(
                formatter,
                "SECS API error {:?}: {}",
                error.code, error.detail
            ),
            Self::Transport(message) => write!(formatter, "transport error: {message}"),
            Self::Protocol(message) => write!(formatter, "protocol error: {message}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<IpcError> for Error {
    fn from(error: IpcError) -> Self {
        Self::Api(error)
    }
}

/// Result type used by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Common request interface implemented by the embedded engine and local client.
pub trait Secs {
    /// Handles one typed request and turns an error envelope into [`Error::Api`].
    fn call(&self, request: Request) -> Result<Response>;
}

#[cfg(feature = "engine")]
fn envelope_result(envelope: secs_types::ipc::Envelope) -> Result<Response> {
    match envelope {
        secs_types::ipc::Envelope::Ok(response) => Ok(*response),
        secs_types::ipc::Envelope::Err(error) => Err(Error::Api(error)),
    }
}

#[cfg(feature = "engine")]
mod engine {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
    use std::time::{Duration, Instant};

    use secs_service::{Deps, EventSink, NoEvents, Service};
    use secs_types::compute::ComputePreference;
    use secs_types::ipc::{Envelope, JobEvent, JobId, Request, Response};

    use crate::{Error, Result, Secs, envelope_result};

    type AudioFactory = Arc<dyn Fn() -> Box<dyn secs_audio::AudioBackend> + Send + Sync>;
    type RewFactory = Arc<dyn Fn() -> Box<dyn secs_rew::Transport> + Send + Sync>;

    #[derive(Default)]
    struct EventState {
        subscribers: Vec<mpsc::Sender<JobEvent>>,
        terminal: HashMap<JobId, JobEvent>,
    }

    struct Observer {
        state: Mutex<EventState>,
        changed: Condvar,
        external: Option<Arc<dyn EventSink>>,
    }

    impl fmt::Debug for Observer {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            let state = lock(&self.state);
            formatter
                .debug_struct("Observer")
                .field("subscribers", &state.subscribers.len())
                .field("terminal_jobs", &state.terminal.len())
                .field("external", &self.external.as_ref().map(|_| "observer"))
                .finish()
        }
    }

    impl EventSink for Observer {
        fn send(&self, event: JobEvent) {
            if let Some(external) = &self.external {
                external.send(event.clone());
            }
            let mut state = lock(&self.state);
            state
                .subscribers
                .retain(|subscriber| subscriber.send(event.clone()).is_ok());
            if let JobEvent::Finished { job, .. } = &event {
                state.terminal.insert(*job, event);
                self.changed.notify_all();
            }
        }
    }

    /// Builder for an embedded [`Engine`].
    pub struct EngineBuilder {
        app_settings_path: Option<PathBuf>,
        compute_preference: Option<ComputePreference>,
        audio_backend: Option<AudioFactory>,
        rew_transport: RewFactory,
        observer: Option<Arc<dyn EventSink>>,
    }

    impl fmt::Debug for EngineBuilder {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("EngineBuilder")
                .field("app_settings_path", &self.app_settings_path)
                .field("compute_preference", &self.compute_preference)
                .field(
                    "audio_backend",
                    &self.audio_backend.as_ref().map(|_| "factory"),
                )
                .field("rew_transport", &"factory")
                .field("observer", &self.observer.as_ref().map(|_| "observer"))
                .finish()
        }
    }

    impl Default for EngineBuilder {
        fn default() -> Self {
            Self {
                app_settings_path: None,
                compute_preference: None,
                audio_backend: None,
                rew_transport: Arc::new(|| Box::new(secs_rew::HttpTransport::default())),
                observer: None,
            }
        }
    }

    impl EngineBuilder {
        /// Stores application settings at `path` instead of keeping them in memory.
        #[must_use]
        pub fn app_settings_path(mut self, path: impl Into<PathBuf>) -> Self {
            self.app_settings_path = Some(path.into());
            self
        }

        /// Overrides the persisted compute preference for this engine process.
        #[must_use]
        pub fn compute_preference(mut self, preference: ComputePreference) -> Self {
            self.compute_preference = Some(preference);
            self
        }

        /// Supplies an audio backend factory for measurement requests.
        #[must_use]
        pub fn audio_backend_factory(mut self, factory: AudioFactory) -> Self {
            self.audio_backend = Some(factory);
            self
        }

        /// Supplies the REW transport factory used by REW measurement requests.
        #[must_use]
        pub fn rew_transport_factory(mut self, factory: RewFactory) -> Self {
            self.rew_transport = factory;
            self
        }

        /// Adds an observer that receives every job event in service order.
        #[must_use]
        pub fn observer(mut self, observer: Arc<dyn EventSink>) -> Self {
            self.observer = Some(observer);
            self
        }

        /// Builds the embedded engine.
        #[must_use]
        pub fn build(self) -> Engine {
            let observer = Arc::new(Observer {
                state: Mutex::new(EventState::default()),
                changed: Condvar::new(),
                external: self.observer,
            });
            let mut deps = Deps {
                app_version: env!("CARGO_PKG_VERSION").into(),
                compute_override: self.compute_preference,
                rew_transport: self.rew_transport,
                observer: observer.clone(),
                app_settings_path: self.app_settings_path,
                ..Deps::default()
            };
            if let Some(audio_backend) = self.audio_backend {
                deps.audio_backend = audio_backend;
            }
            Engine {
                service: Service::new(deps),
                observer,
            }
        }
    }

    /// Embedded, file-based SECS 2R service.
    pub struct Engine {
        service: Service,
        observer: Arc<Observer>,
    }

    impl fmt::Debug for Engine {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("Engine")
                .field("service", &self.service)
                .field("observer", &self.observer)
                .finish()
        }
    }

    impl Default for Engine {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Engine {
        /// Builds an engine with in-memory settings, CPU computation, no native
        /// audio backend, and the default REW HTTP transport.
        #[must_use]
        pub fn new() -> Self {
            EngineBuilder::default().build()
        }

        /// Handles an untyped JSON request and preserves the service envelope.
        #[must_use]
        pub fn call_json(&self, request: serde_json::Value) -> Envelope {
            self.service.call_value(request, Arc::new(NoEvents))
        }

        /// Subscribes to every job event emitted after this call.
        #[must_use]
        pub fn subscribe(&self) -> mpsc::Receiver<JobEvent> {
            let (sender, receiver) = mpsc::channel();
            lock(&self.observer.state).subscribers.push(sender);
            receiver
        }

        /// Waits for the terminal event of `job_id` until `timeout` expires.
        pub fn wait_job(&self, job_id: JobId, timeout: Duration) -> Result<JobEvent> {
            let deadline = Instant::now() + timeout;
            let mut state = lock(&self.observer.state);
            loop {
                if let Some(event) = state.terminal.get(&job_id) {
                    return Ok(event.clone());
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(Error::Protocol(format!(
                        "timed out waiting for job {}",
                        job_id.0
                    )));
                }
                let (next, wait) = self
                    .observer
                    .changed
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state = next;
                if wait.timed_out() && !state.terminal.contains_key(&job_id) {
                    return Err(Error::Protocol(format!(
                        "timed out waiting for job {}",
                        job_id.0
                    )));
                }
            }
        }

        /// Returns the underlying service for operations not wrapped by this crate.
        #[must_use]
        pub fn service(&self) -> &Service {
            &self.service
        }
    }

    impl Secs for Engine {
        fn call(&self, request: Request) -> Result<Response> {
            envelope_result(self.service.call(request, Arc::new(NoEvents)))
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    use std::fmt;
}

#[cfg(feature = "engine")]
pub use engine::{Engine, EngineBuilder};

#[cfg(feature = "client")]
mod client {
    use std::fs;
    use std::net::TcpStream;
    use std::path::{Path, PathBuf};

    use secs_types::ipc::{IpcError, Request, Response};
    use secs_types::local_api::{self, ServerEvent, Status};
    use serde::Deserialize;
    use tungstenite::client::ClientRequestBuilder;
    use tungstenite::protocol::WebSocket;
    use tungstenite::stream::MaybeTlsStream;

    use crate::{Error, Result, Secs};

    const APP_IDENTIFIER: &str = "io.github.115dkk.secs2r";
    const BODY_LIMIT: u64 = 16 * 1024 * 1024;

    #[derive(Deserialize)]
    struct WireEnvelope {
        ok: bool,
        data: Option<Response>,
        error: Option<IpcError>,
    }

    /// Blocking client for a running SECS 2R app's authenticated loopback API.
    #[derive(Clone)]
    pub struct Client {
        port: u16,
        token: String,
        agent: ureq::Agent,
    }

    impl std::fmt::Debug for Client {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("Client")
                .field("port", &self.port)
                .field("token", &"<redacted>")
                .finish_non_exhaustive()
        }
    }

    impl Client {
        /// Creates a client for `127.0.0.1:port` with one bearer token.
        #[must_use]
        pub fn new(port: u16, token: impl Into<String>) -> Self {
            let config = ureq::Agent::config_builder()
                .http_status_as_error(false)
                .proxy(None)
                .max_redirects(0)
                .build();
            Self {
                port,
                token: token.into(),
                agent: config.into(),
            }
        }

        /// Reads a bearer token from `path` and creates a client.
        pub fn from_token_file(port: u16, path: impl AsRef<Path>) -> Result<Self> {
            let token = fs::read_to_string(path.as_ref()).map_err(|error| {
                Error::Transport(format!(
                    "could not read token file {}: {error}",
                    path.as_ref().display()
                ))
            })?;
            Ok(Self::new(port, token.trim().to_owned()))
        }

        /// Returns the desktop app's default `local-api-token` path.
        ///
        /// The base is the platform configuration directory used by Tauri and
        /// the next component is the identifier `io.github.115dkk.secs2r` from
        /// `apps/secs2r/tauri.conf.json`.
        #[must_use]
        pub fn default_token_path() -> PathBuf {
            config_dir()
                .join(APP_IDENTIFIER)
                .join(local_api::TOKEN_FILE)
        }

        /// Sends an untyped request value and returns its typed successful response.
        ///
        /// Error envelopes become [`Error::Api`].
        pub fn call_json(&self, request: serde_json::Value) -> Result<Response> {
            let body = serde_json::to_vec(&request)
                .map_err(|error| Error::Protocol(format!("request JSON: {error}")))?;
            let text = self.post(local_api::CALL_PATH, &body)?;
            decode_envelope(&text)
        }

        /// Reads the aggregate state of the running app.
        pub fn status(&self) -> Result<Status> {
            let text = self.get(local_api::STATUS_PATH)?;
            serde_json::from_str(&text)
                .map_err(|error| Error::Protocol(format!("status response: {error}")))
        }

        /// Opens a blocking iterator over local-API WebSocket events.
        pub fn events(&self) -> Result<Events> {
            let uri = format!("ws://127.0.0.1:{}{}", self.port, local_api::EVENTS_PATH)
                .parse()
                .map_err(|error| Error::Protocol(format!("event URL: {error}")))?;
            let request = ClientRequestBuilder::new(uri)
                .with_header("Authorization", format!("Bearer {}", self.token));
            let (socket, _) = tungstenite::client::connect_with_config(request, None, 0)
                .map_err(websocket_error)?;
            Ok(Events {
                socket,
                finished: false,
            })
        }

        fn get(&self, path: &str) -> Result<String> {
            let response = self
                .agent
                .get(self.url(path))
                .header("Authorization", self.authorization())
                .header("Host", self.host())
                .call()
                .map_err(|error| Error::Transport(error.to_string()))?;
            read_response(response)
        }

        fn post(&self, path: &str, body: &[u8]) -> Result<String> {
            let response = self
                .agent
                .post(self.url(path))
                .header("Authorization", self.authorization())
                .header("Host", self.host())
                .header("Content-Type", "application/json")
                .send(body)
                .map_err(|error| Error::Transport(error.to_string()))?;
            read_response(response)
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }

        fn host(&self) -> String {
            format!("127.0.0.1:{}", self.port)
        }

        fn authorization(&self) -> String {
            format!("Bearer {}", self.token)
        }
    }

    impl Secs for Client {
        fn call(&self, request: Request) -> Result<Response> {
            let value = serde_json::to_value(request)
                .map_err(|error| Error::Protocol(format!("request value: {error}")))?;
            self.call_json(value)
        }
    }

    /// Blocking iterator over events sent by the local API WebSocket.
    pub struct Events {
        socket: WebSocket<MaybeTlsStream<TcpStream>>,
        finished: bool,
    }

    impl std::fmt::Debug for Events {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("Events")
                .field("finished", &self.finished)
                .finish_non_exhaustive()
        }
    }

    impl Iterator for Events {
        type Item = Result<ServerEvent>;

        fn next(&mut self) -> Option<Self::Item> {
            if self.finished {
                return None;
            }
            loop {
                match self.socket.read() {
                    Ok(tungstenite::Message::Text(text)) => {
                        return Some(
                            serde_json::from_str(text.as_ref()).map_err(|error| {
                                Error::Protocol(format!("event response: {error}"))
                            }),
                        );
                    }
                    Ok(tungstenite::Message::Close(_)) => {
                        self.finished = true;
                        return None;
                    }
                    Ok(_) => {}
                    Err(tungstenite::Error::ConnectionClosed) => {
                        self.finished = true;
                        return None;
                    }
                    Err(error) => {
                        self.finished = true;
                        return Some(Err(websocket_error(error)));
                    }
                }
            }
        }
    }

    fn read_response(mut response: ureq::http::Response<ureq::Body>) -> Result<String> {
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(Error::Transport(format!("HTTP status {status}")));
        }
        response
            .body_mut()
            .with_config()
            .limit(BODY_LIMIT)
            .read_to_string()
            .map_err(|error| Error::Transport(error.to_string()))
    }

    fn decode_envelope(text: &str) -> Result<Response> {
        let envelope: WireEnvelope = serde_json::from_str(text)
            .map_err(|error| Error::Protocol(format!("call response: {error}")))?;
        match (envelope.ok, envelope.data, envelope.error) {
            (true, Some(response), None) => Ok(response),
            (false, None, Some(error)) => Err(Error::Api(error)),
            _ => Err(Error::Protocol(
                "call response has an inconsistent envelope".into(),
            )),
        }
    }

    fn websocket_error(error: tungstenite::Error) -> Error {
        match error {
            tungstenite::Error::Http(response) => {
                Error::Transport(format!("HTTP status {}", response.status().as_u16()))
            }
            other => Error::Transport(other.to_string()),
        }
    }

    #[cfg(target_os = "windows")]
    fn config_dir() -> PathBuf {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(PathBuf::from)
                    .map(|home| home.join("AppData").join("Roaming"))
            })
            .unwrap_or_else(|| PathBuf::from("."))
    }

    #[cfg(target_os = "macos")]
    fn config_dir() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library").join("Application Support"))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn config_dir() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

#[cfg(feature = "client")]
pub use client::{Client, Events};
