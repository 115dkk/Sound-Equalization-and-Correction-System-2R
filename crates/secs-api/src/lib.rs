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
//! use secs::types::local_api::DEFAULT_PORT;
//! use secs::{Client, Event};
//!
//! let client = Client::from_token_file(DEFAULT_PORT, Client::default_token_path())?;
//! let status = client.status()?;
//! println!("{} {}", status.app_name, status.app_version);
//!
//! let subscription = client.subscribe()?;
//! for event in subscription.receiver.iter().take(10) {
//!     match event {
//!         Event::Job(event) => println!("job: {event:?}"),
//!         Event::Changed { method } => println!("changed: {method}"),
//!         Event::Lagged { missed } => println!("missed: {missed}"),
//!         Event::Closed { reason } => println!("closed: {reason}"),
//!     }
//! }
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "client"))]
//! # fn main() {}
//! ```

use std::fmt;

pub use secs_types as types;
pub use secs_types::ipc::{ErrorCode, IpcError, JobEvent, JobId, Request, Response};

/// An error returned by the embedded engine or local-API client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A structured error returned by the SECS service.
    Api(IpcError),
    /// An HTTP, WebSocket, file, or socket operation failed.
    Transport(String),
    /// A peer returned data that does not follow the local-API protocol.
    Protocol(String),
    /// A job did not finish before the requested deadline.
    Timeout {
        /// Job whose wait expired.
        job: JobId,
    },
    /// A job is neither running nor retained by the adapter.
    JobNotFound {
        /// Job absent from the adapter's running and retained jobs.
        job: JobId,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&WireError::from(self).message)
    }
}

/// Serializable error shared by the language bindings.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireError {
    /// Stable snake-case error code.
    pub code: String,
    /// Single-line diagnostic message, beginning with the code.
    pub message: String,
    /// Original diagnostic detail, when available.
    pub detail: Option<String>,
    /// Structured error parameters.
    pub params: std::collections::BTreeMap<String, String>,
    /// Whether retrying the same operation can succeed.
    pub retryable: bool,
}

impl From<&Error> for WireError {
    fn from(error: &Error) -> Self {
        let (code, detail, params, retryable) = match error {
            Error::Api(error) => (
                serde_json::to_value(error.code)
                    .expect("error code serialization")
                    .as_str()
                    .expect("string error code")
                    .to_owned(),
                error.detail.clone(),
                error.params.clone(),
                error.retryable,
            ),
            Error::Transport(detail) => (
                "transport_error".into(),
                detail.clone(),
                Default::default(),
                false,
            ),
            Error::Protocol(detail) => (
                "protocol_error".into(),
                detail.clone(),
                Default::default(),
                false,
            ),
            Error::Timeout { job } => (
                "timeout".into(),
                format!("job {}", job.0),
                Default::default(),
                false,
            ),
            Error::JobNotFound { job } => (
                "job_not_found".into(),
                format!("job {}", job.0),
                Default::default(),
                false,
            ),
        };
        let message = format!(
            "{code}: {}",
            detail.split_whitespace().collect::<Vec<_>>().join(" ")
        );
        Self {
            code,
            message,
            detail: Some(detail),
            params,
            retryable,
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

/// One event delivered by either public API adapter.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "event", rename_all = "snake_case")]
pub enum Event {
    /// A service job event.
    Job(JobEvent),
    /// A successful request changed observable state.
    Changed {
        /// Stable IPC method name.
        method: String,
    },
    /// Events were lost before this subscriber could receive them.
    Lagged {
        /// Number of events no longer available.
        missed: u64,
    },
    /// The event source ended the subscription.
    Closed {
        /// Peer-provided or local close reason.
        reason: String,
    },
}

/// Shared count of events dropped from one bounded subscription queue.
#[derive(Clone, Debug, Default)]
pub struct DroppedEventCount(std::sync::Arc<std::sync::atomic::AtomicU64>);

impl DroppedEventCount {
    /// Returns how many events were dropped because the queue was full.
    #[must_use]
    pub fn get(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn increment(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Handle that interrupts a blocked subscription receiver.
#[derive(Clone)]
pub struct SubscriptionCloser(std::sync::Arc<dyn Fn() + Send + Sync>);

impl std::fmt::Debug for SubscriptionCloser {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SubscriptionCloser")
            .finish_non_exhaustive()
    }
}

impl SubscriptionCloser {
    fn new(close: impl Fn() + Send + Sync + 'static) -> Self {
        Self(std::sync::Arc::new(close))
    }

    /// Ends the subscription and wakes a blocked receiver.
    pub fn close(&self) {
        (self.0)();
    }
}

/// Bounded event subscription returned by both public API adapters.
#[derive(Debug)]
pub struct Subscription {
    /// Event receiver.
    pub receiver: std::sync::mpsc::Receiver<Event>,
    /// Count of non-terminal events dropped while the queue was full.
    pub dropped: DroppedEventCount,
    /// Handle used to interrupt a blocked receiver.
    pub closer: SubscriptionCloser,
}

/// Common request interface implemented by the embedded engine and local client.
pub trait Secs {
    /// Handles one typed request and turns an error envelope into [`Error::Api`].
    fn call(&self, request: Request) -> Result<Response>;
    /// Waits for one job's terminal event.
    fn wait_job(&self, job: JobId, timeout: std::time::Duration) -> Result<JobEvent>;
    /// Subscribes to events emitted after this call.
    fn subscribe(&self) -> Result<Subscription>;
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
    use std::collections::{HashMap, VecDeque};
    use std::fmt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
    use std::time::{Duration, Instant};

    use secs_service::{Deps, EventSink, NoEvents, Service};
    use secs_types::compute::ComputePreference;
    use secs_types::ipc::{Envelope, JobEvent, JobId, Request, Response};

    use crate::{
        DroppedEventCount, Error, Event, Result, Secs, Subscription, SubscriptionCloser,
        envelope_result,
    };

    type AudioFactory = Arc<dyn Fn() -> Box<dyn secs_audio::AudioBackend> + Send + Sync>;
    type RewFactory = Arc<dyn Fn() -> Box<dyn secs_rew::Transport> + Send + Sync>;

    const TERMINAL_RETENTION: usize = 32;
    const SUBSCRIPTION_CAPACITY: usize = 1024;

    struct Subscriber {
        id: u64,
        sender: mpsc::SyncSender<Event>,
        dropped: DroppedEventCount,
    }

    #[derive(Default)]
    struct EventState {
        subscribers: Vec<Subscriber>,
        terminal: HashMap<JobId, JobEvent>,
        terminal_order: VecDeque<JobId>,
    }

    struct Observer {
        state: Mutex<EventState>,
        changed: Condvar,
        next_subscriber: AtomicU64,
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
            let terminal_job = match &event {
                JobEvent::Finished { job, .. } => Some(*job),
                _ => None,
            };
            let wrapped = Event::Job(event.clone());
            let mut state = lock(&self.state);
            state.subscribers.retain(|subscriber| {
                let result = if terminal_job.is_some() {
                    subscriber
                        .sender
                        .send(wrapped.clone())
                        .map_err(|_| mpsc::TrySendError::Disconnected(wrapped.clone()))
                } else {
                    subscriber.sender.try_send(wrapped.clone())
                };
                match result {
                    Ok(()) => true,
                    Err(mpsc::TrySendError::Full(_)) => {
                        subscriber.dropped.increment();
                        true
                    }
                    Err(mpsc::TrySendError::Disconnected(_)) => false,
                }
            });
            if let Some(job) = terminal_job {
                if state.terminal.insert(job, event).is_none() {
                    state.terminal_order.push_back(job);
                }
                while state.terminal_order.len() > TERMINAL_RETENTION {
                    if let Some(expired) = state.terminal_order.pop_front() {
                        state.terminal.remove(&expired);
                    }
                }
                self.changed.notify_all();
            }
        }
    }

    /// Builder for an embedded [`Engine`].
    pub struct EngineBuilder {
        deps: Deps,
    }

    impl fmt::Debug for EngineBuilder {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("EngineBuilder")
                .field("deps", &self.deps)
                .finish()
        }
    }

    impl Default for EngineBuilder {
        fn default() -> Self {
            Self::from_deps(Deps {
                app_version: env!("CARGO_PKG_VERSION").into(),
                rew_transport: Arc::new(|| Box::new(secs_rew::HttpTransport::default())),
                ..Deps::default()
            })
        }
    }

    impl EngineBuilder {
        /// Creates a builder from a complete service dependency set.
        ///
        /// # Stability
        ///
        /// [`Deps`] and the internal-crate traits stored in it are an unstable
        /// escape hatch and are not part of this crate's compatibility promise.
        #[must_use]
        pub fn from_deps(deps: Deps) -> Self {
            Self { deps }
        }

        /// Stores application settings at `path` instead of keeping them in memory.
        #[must_use]
        pub fn app_settings_path(mut self, path: impl Into<PathBuf>) -> Self {
            self.deps.app_settings_path = Some(path.into());
            self
        }

        /// Overrides the persisted compute preference for this engine process.
        #[must_use]
        pub fn compute_preference(mut self, preference: ComputePreference) -> Self {
            self.deps.compute_override = Some(preference);
            self
        }

        /// Unstable: supplies an internal audio backend factory for measurement requests.
        ///
        /// [`secs_audio::AudioBackend`] may change without a compatibility promise
        /// (ADR 0016). Typed [`Secs`] calls are the stable surface.
        #[must_use]
        pub fn audio_backend_factory(mut self, factory: AudioFactory) -> Self {
            self.deps.audio_backend = factory;
            self
        }

        /// Unstable: supplies the internal REW transport factory for measurement requests.
        ///
        /// [`secs_rew::Transport`] may change without a compatibility promise
        /// (ADR 0016). Typed [`Secs`] calls are the stable surface.
        #[must_use]
        pub fn rew_transport_factory(mut self, factory: RewFactory) -> Self {
            self.deps.rew_transport = factory;
            self
        }

        /// Supplies the process probe used to find REW.
        #[must_use]
        pub fn process(
            mut self,
            process: Arc<dyn secs_rew::launch::ProcessProbe + Send + Sync>,
        ) -> Self {
            self.deps.process = process;
            self
        }

        /// Supplies the memory probe used for compute budgets.
        #[must_use]
        pub fn memory(mut self, memory: Arc<dyn secs_compute::MemoryProbe>) -> Self {
            self.deps.memory = memory;
            self
        }

        /// Supplies the DSP apply adapter.
        #[must_use]
        pub fn apply(mut self, apply: Arc<dyn secs_service::ApplyAdapter>) -> Self {
            self.deps.apply = apply;
            self
        }

        /// Supplies the managed-host adapter.
        #[must_use]
        pub fn managed_host(
            mut self,
            managed_host: Arc<dyn secs_service::ManagedHostAdapter>,
        ) -> Self {
            self.deps.managed_host = managed_host;
            self
        }

        /// Supplies the Equalizer APO installation detector.
        #[must_use]
        pub fn eqapo_install(
            mut self,
            eqapo_install: Arc<dyn Fn() -> Option<secs_types::ipc::EqApoInstall> + Send + Sync>,
        ) -> Self {
            self.deps.eqapo_install = eqapo_install;
            self
        }

        /// Supplies a local wall clock and its offset east of UTC.
        #[must_use]
        pub fn clock(
            mut self,
            clock: Arc<dyn Fn() -> secs_service::LocalTime + Send + Sync>,
            offset_seconds: Arc<dyn Fn() -> i32 + Send + Sync>,
        ) -> Self {
            self.deps.clock = clock;
            self.deps.clock_offset_seconds = offset_seconds;
            self
        }

        /// Supplies milliseconds since the Unix epoch.
        #[must_use]
        pub fn unix_ms(mut self, unix_ms: Arc<dyn Fn() -> i64 + Send + Sync>) -> Self {
            self.deps.unix_ms = unix_ms;
            self
        }

        /// Supplies UUID values for exported measurements.
        #[must_use]
        pub fn uuid(mut self, uuid: secs_service::UuidSource) -> Self {
            self.deps.uuid = uuid;
            self
        }

        /// Supplies the compute acceleration selector.
        #[must_use]
        pub fn acceleration(
            mut self,
            acceleration: Arc<
                dyn Fn(ComputePreference) -> secs_engine::AccelerationSelection + Send + Sync,
            >,
        ) -> Self {
            self.deps.acceleration = acceleration;
            self
        }

        /// Exposes the application log directory through bootstrap.
        #[must_use]
        pub fn log_dir(mut self, log_dir: impl Into<PathBuf>) -> Self {
            self.deps.log_dir = Some(log_dir.into());
            self
        }

        /// Adds an observer that receives every job event in service order.
        #[must_use]
        pub fn observer(mut self, observer: Arc<dyn EventSink>) -> Self {
            self.deps.observer = observer;
            self
        }

        /// Builds the embedded engine.
        #[must_use]
        pub fn build(mut self) -> Engine {
            let observer = Arc::new(Observer {
                state: Mutex::new(EventState::default()),
                changed: Condvar::new(),
                next_subscriber: AtomicU64::new(1),
                external: Some(self.deps.observer.clone()),
            });
            self.deps.observer = observer.clone();
            Engine {
                service: Service::new(self.deps),
                observer,
            }
        }
    }

    /// Embedded, file-based SECS 2R service.
    ///
    /// Dropping the engine requests cancellation of its jobs and waits at most
    /// three seconds for device access to stop. Terminal events after drop are
    /// not guaranteed to reach subscribers; use `wait_job` before dropping the
    /// engine when the terminal result is required.
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

    impl Drop for Engine {
        fn drop(&mut self) {
            self.service.shutdown(Duration::from_secs(3));
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

        /// Subscribes to events emitted after this call.
        pub fn subscribe(&self) -> Result<Subscription> {
            let (sender, receiver) = mpsc::sync_channel(SUBSCRIPTION_CAPACITY);
            let dropped = DroppedEventCount::default();
            let id = self
                .observer
                .next_subscriber
                .fetch_add(1, Ordering::Relaxed);
            lock(&self.observer.state).subscribers.push(Subscriber {
                id,
                sender,
                dropped: dropped.clone(),
            });
            let observer = Arc::downgrade(&self.observer);
            let closer = SubscriptionCloser::new(move || {
                if let Some(observer) = observer.upgrade() {
                    lock(&observer.state)
                        .subscribers
                        .retain(|subscriber| subscriber.id != id);
                }
            });
            Ok(Subscription {
                receiver,
                dropped,
                closer,
            })
        }

        /// Waits for the terminal event of `job_id` until `timeout` expires.
        ///
        /// Returns [`Error::Timeout`] with `job: job_id` when the deadline expires.
        /// Returns [`Error::JobNotFound`] with `job: job_id` if the job is neither
        /// running nor among the retained terminal events.
        pub fn wait_job(&self, job_id: JobId, timeout: Duration) -> Result<JobEvent> {
            let deadline = Instant::now() + timeout;
            // Query the service before locking the observer. A finishing job may
            // hold the service state while it publishes its terminal event.
            let running = self
                .service
                .status()
                .jobs
                .iter()
                .any(|job| job.job == job_id);
            let mut state = lock(&self.observer.state);
            if !state.terminal.contains_key(&job_id) && !running {
                return Err(Error::JobNotFound { job: job_id });
            }
            loop {
                if let Some(event) = state.terminal.get(&job_id) {
                    return Ok(event.clone());
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(Error::Timeout { job: job_id });
                }
                let (next, wait) = self
                    .observer
                    .changed
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state = next;
                if wait.timed_out() && !state.terminal.contains_key(&job_id) {
                    return Err(Error::Timeout { job: job_id });
                }
            }
        }

        /// Unstable: returns the internal service for operations not wrapped by this crate.
        ///
        /// Its items may change without a compatibility promise (ADR 0016).
        /// Typed [`Secs`] calls are the stable surface.
        #[must_use]
        pub fn service(&self) -> &Service {
            &self.service
        }
    }

    impl Secs for Engine {
        fn call(&self, request: Request) -> Result<Response> {
            envelope_result(self.service.call(request, Arc::new(NoEvents)))
        }

        fn wait_job(&self, job: JobId, timeout: Duration) -> Result<JobEvent> {
            Engine::wait_job(self, job, timeout)
        }

        fn subscribe(&self) -> Result<Subscription> {
            Engine::subscribe(self)
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
        fn a_full_subscription_counts_drops_without_blocking_td127() {
            let observer = Observer {
                state: Mutex::new(EventState::default()),
                changed: Condvar::new(),
                next_subscriber: AtomicU64::new(1),
                external: None,
            };
            let (sender, receiver) = mpsc::sync_channel(1024);
            let dropped = DroppedEventCount::default();
            lock(&observer.state).subscribers.push(Subscriber {
                id: 1,
                sender,
                dropped: dropped.clone(),
            });
            for id in 0..=1024 {
                observer.send(JobEvent::Started {
                    job: JobId(u64::try_from(id).unwrap()),
                    kind: secs_types::ipc::JobKind::Export,
                });
            }
            assert_eq!(dropped.get(), 1);
            assert_eq!(receiver.try_iter().count(), 1024);
        }

        #[test]
        fn terminal_events_keep_only_the_service_retention_td127() {
            let observer = Observer {
                state: Mutex::new(EventState::default()),
                changed: Condvar::new(),
                next_subscriber: AtomicU64::new(1),
                external: None,
            };
            for id in 0..40 {
                observer.send(JobEvent::Finished {
                    job: JobId(id),
                    status: secs_types::ipc::JobStatus::Succeeded,
                    result: None,
                    error: None,
                });
            }
            let state = lock(&observer.state);
            assert_eq!(state.terminal.len(), 32);
            assert!(!state.terminal.contains_key(&JobId(7)));
            assert!(state.terminal.contains_key(&JobId(8)));
            assert!(state.terminal.contains_key(&JobId(39)));
        }
    }
}

#[cfg(feature = "engine")]
pub use engine::{Engine, EngineBuilder};

#[cfg(feature = "client")]
mod client {
    use std::fs;
    use std::net::{SocketAddr, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, Instant};

    use secs_types::ipc::{IpcError, JobEvent, JobId, Request, Response};
    use secs_types::local_api::{self, ServerEvent, Status};
    use serde::Deserialize;
    use tungstenite::client::ClientRequestBuilder;
    use tungstenite::protocol::WebSocket;
    use tungstenite::stream::MaybeTlsStream;

    use crate::{DroppedEventCount, Error, Event, Result, Secs, Subscription, SubscriptionCloser};

    const APP_IDENTIFIER: &str = "io.github.115dkk.secs2r";
    const BODY_LIMIT: u64 = 16 * 1024 * 1024;
    // The app answers on the loopback at once; this only bounds a server that hangs.
    const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
    /// How long a quiet event socket rests before it looks again for an event
    /// and for its closer's flag.
    const EVENT_POLL: Duration = Duration::from_millis(10);
    /// The reason of the `Closed` event a subscription ends with when its
    /// closer ended it.
    const CLOSED_BY_CALLER: &str = "subscription closed";

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
        fn open_events(&self) -> Result<RawEvents> {
            let uri = format!("ws://127.0.0.1:{}{}", self.port, local_api::EVENTS_PATH)
                .parse()
                .map_err(|error| Error::Protocol(format!("event URL: {error}")))?;
            let request = ClientRequestBuilder::new(uri)
                .with_header("Authorization", format!("Bearer {}", self.token));
            // A server that accepts the connection but never answers the
            // upgrade must not hold the caller forever, so connecting and the
            // handshake have a time limit. The events themselves then wait as
            // long as the server is quiet; the closer ends that wait.
            let socket_error =
                |error: std::io::Error| Error::Transport(format!("event socket: {error}"));
            let address = SocketAddr::from(([127, 0, 0, 1], self.port));
            let stream =
                TcpStream::connect_timeout(&address, HANDSHAKE_TIMEOUT).map_err(socket_error)?;
            stream
                .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
                .map_err(socket_error)?;
            stream
                .set_write_timeout(Some(HANDSHAKE_TIMEOUT))
                .map_err(socket_error)?;
            let (socket, _) = tungstenite::client::client_with_config(
                request,
                MaybeTlsStream::Plain(stream),
                None,
            )
            .map_err(|error| match error {
                tungstenite::HandshakeError::Failure(error) => websocket_error(error),
                tungstenite::HandshakeError::Interrupted(_) => {
                    Error::Transport("event socket handshake timed out".into())
                }
            })?;
            if let MaybeTlsStream::Plain(stream) = socket.get_ref() {
                stream.set_read_timeout(None).map_err(socket_error)?;
                stream.set_write_timeout(None).map_err(socket_error)?;
                // From here the socket does not block. A read blocked on a
                // quiet server could be ended only through the socket, and
                // neither way holds everywhere. On Windows shutting a socket
                // down is not sure to wake a blocked read (std's own test of
                // it, `close_read_wakes_up`, is off there), so the wait can
                // last until the server closes its side; and a receive that
                // times out leaves the connection in an indeterminate state
                // (Winsock, SO_RCVTIMEO). The reader looks again every
                // EVENT_POLL instead and ends on the closer's flag, with no
                // help from the server.
                stream.set_nonblocking(true).map_err(socket_error)?;
            }
            Ok(RawEvents {
                socket,
                closed: Arc::new(AtomicBool::new(false)),
                finished: false,
            })
        }

        /// Opens a bounded event subscription.
        pub fn subscribe(&self) -> Result<Subscription> {
            const CAPACITY: usize = 1024;
            let events = self.open_events()?;
            let raw_closer = events.closer()?;
            let (sender, receiver) = mpsc::sync_channel(CAPACITY);
            let dropped = DroppedEventCount::default();
            let pump_dropped = dropped.clone();
            std::thread::spawn(move || pump(events, &sender, &pump_dropped));
            let closer = SubscriptionCloser::new(move || raw_closer.close());
            Ok(Subscription {
                receiver,
                dropped,
                closer,
            })
        }

        /// Waits for a terminal event, or reports an unknown job or timeout.
        pub fn wait_job(&self, job: JobId, timeout: Duration) -> Result<JobEvent> {
            let subscription = self.subscribe()?;
            let snapshot = self.status()?.jobs.into_iter().find(|item| item.job == job);
            let Some(snapshot) = snapshot else {
                subscription.closer.close();
                return Err(Error::JobNotFound { job });
            };
            if snapshot.status.is_terminal() {
                subscription.closer.close();
                return Ok(JobEvent::Finished {
                    job,
                    status: snapshot.status,
                    result: None,
                    error: snapshot.error,
                });
            }
            let deadline = Instant::now() + timeout;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    subscription.closer.close();
                    return Err(Error::Timeout { job });
                }
                match subscription.receiver.recv_timeout(remaining) {
                    Ok(Event::Job(event @ JobEvent::Finished { job: found, .. }))
                        if found == job =>
                    {
                        subscription.closer.close();
                        return Ok(event);
                    }
                    Ok(Event::Closed { reason }) => {
                        return Err(Error::Transport(format!("event stream closed: {reason}")));
                    }
                    Ok(_) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        subscription.closer.close();
                        return Err(Error::Timeout { job });
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(Error::Transport("event stream closed".into()));
                    }
                }
            }
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

        fn wait_job(&self, job: JobId, timeout: Duration) -> Result<JobEvent> {
            Client::wait_job(self, job, timeout)
        }

        fn subscribe(&self) -> Result<Subscription> {
            Client::subscribe(self)
        }
    }

    struct RawEvents {
        socket: WebSocket<MaybeTlsStream<TcpStream>>,
        /// Set by the closer; the reader ends at its next look.
        closed: Arc<AtomicBool>,
        finished: bool,
    }

    impl std::fmt::Debug for RawEvents {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("RawEvents")
                .field("finished", &self.finished)
                .finish_non_exhaustive()
        }
    }

    impl RawEvents {
        fn closer(&self) -> Result<RawEventsCloser> {
            match self.socket.get_ref() {
                MaybeTlsStream::Plain(stream) => stream
                    .try_clone()
                    .map(|stream| RawEventsCloser {
                        stream,
                        closed: Arc::clone(&self.closed),
                    })
                    .map_err(|error| Error::Transport(format!("event socket: {error}"))),
                _ => Err(Error::Protocol(
                    "event socket is not a plain TCP stream".into(),
                )),
            }
        }
    }

    #[derive(Debug)]
    struct RawEventsCloser {
        stream: TcpStream,
        closed: Arc<AtomicBool>,
    }

    impl RawEventsCloser {
        /// Ends the reader through its flag, then tells the server by
        /// shutting the socket down.
        pub fn close(&self) {
            self.closed.store(true, Ordering::Release);
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
        }
    }

    impl Iterator for RawEvents {
        type Item = Result<Event>;

        fn next(&mut self) -> Option<Self::Item> {
            if self.finished {
                return None;
            }
            loop {
                let read = self.socket.read();
                if self.closed.load(Ordering::Acquire) {
                    // The caller ended the subscription: that is why the
                    // stream ends, whatever the socket said last.
                    self.finished = true;
                    return Some(Ok(Event::Closed {
                        reason: CLOSED_BY_CALLER.into(),
                    }));
                }
                match read {
                    Ok(tungstenite::Message::Text(text)) => {
                        return Some(
                            serde_json::from_str::<ServerEvent>(text.as_ref())
                                .map(map_server_event)
                                .map_err(|error| {
                                    Error::Protocol(format!("event response: {error}"))
                                }),
                        );
                    }
                    Ok(tungstenite::Message::Close(frame)) => {
                        self.finished = true;
                        let reason = frame
                            .map(|frame| {
                                if u16::from(frame.code) == 1013 && frame.reason.is_empty() {
                                    "server closed a lagged subscription".to_owned()
                                } else {
                                    frame.reason.to_string()
                                }
                            })
                            .unwrap_or_else(|| "server closed the subscription".into());
                        return Some(Ok(Event::Closed { reason }));
                    }
                    Ok(_) => {}
                    // Nothing to read yet (a frame may be half arrived;
                    // tungstenite keeps what it has): rest, then look again.
                    Err(tungstenite::Error::Io(error))
                        if error.kind() == std::io::ErrorKind::WouldBlock =>
                    {
                        std::thread::sleep(EVENT_POLL);
                    }
                    Err(tungstenite::Error::ConnectionClosed) => {
                        self.finished = true;
                        return Some(Ok(Event::Closed {
                            reason: "connection closed".into(),
                        }));
                    }
                    Err(error) => {
                        self.finished = true;
                        return Some(Err(websocket_error(error)));
                    }
                }
            }
        }
    }

    fn map_server_event(event: ServerEvent) -> Event {
        match event {
            ServerEvent::Job { event } => Event::Job(event),
            ServerEvent::Changed { method } => Event::Changed { method },
            ServerEvent::Lagged { missed } => Event::Lagged { missed },
        }
    }

    fn pump(
        events: impl Iterator<Item = Result<Event>>,
        sender: &mpsc::SyncSender<Event>,
        dropped: &DroppedEventCount,
    ) {
        for item in events {
            let event = match item {
                Ok(event) => event,
                Err(error) => Event::Closed {
                    reason: error.to_string(),
                },
            };
            let terminal = matches!(
                event,
                Event::Lagged { .. } | Event::Closed { .. } | Event::Job(JobEvent::Finished { .. })
            );
            if terminal {
                if sender.send(event).is_err() {
                    return;
                }
            } else {
                match sender.try_send(event) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Full(_)) => dropped.increment(),
                    Err(mpsc::TrySendError::Disconnected(_)) => return,
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::net::TcpListener;

        /// A server that upgrades the connection and then neither sends, reads
        /// nor closes until the test lets go of it.
        fn mute_server() -> (u16, mpsc::Sender<()>, std::thread::JoinHandle<()>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let (release, held) = mpsc::channel::<()>();
            let server = std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                let socket = tungstenite::accept(stream).unwrap();
                let _ = held.recv();
                drop(socket);
            });
            (port, release, server)
        }

        #[test]
        fn closing_a_subscription_ends_its_reader_without_the_server() {
            let (port, release, server) = mute_server();
            let subscription = Client::new(port, "token").subscribe().unwrap();
            let closer = subscription.closer.clone();
            let (finished, waiting) = mpsc::channel();
            let reader = std::thread::spawn(move || {
                let last = subscription.receiver.recv();
                let _ = finished.send((last, subscription.receiver));
            });
            // By now the reader waits on a quiet stream.
            std::thread::sleep(Duration::from_millis(200));
            closer.close();
            // The server never lets go, so only the closer can end this wait;
            // the limit is wide for a runner that is slow to schedule threads.
            let (last, receiver) = waiting
                .recv_timeout(Duration::from_secs(10))
                .expect("closing must end the reader while the server still holds the connection");
            assert_eq!(
                last,
                Ok(Event::Closed {
                    reason: "subscription closed".into()
                })
            );
            // The thread that fed the receiver is gone as well.
            assert_eq!(
                receiver.recv_timeout(Duration::from_secs(10)),
                Err(mpsc::RecvTimeoutError::Disconnected)
            );
            reader.join().unwrap();
            let _ = release.send(());
            server.join().unwrap();
        }
    }
}

#[cfg(feature = "client")]
pub use client::Client;
