#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! The application service of SECS 2R.
//!
//! The service owns the open project, long-running jobs, cached design results,
//! and export assembly. Applications provide platform dependencies and an event
//! sink; they do not implement domain policy.

mod cache;
mod calibration;
mod clock;
mod export;
pub mod export_folder;
mod host;
mod ipc;
mod jobs;
pub mod mdat_import;
pub mod measure;
mod measurement;
mod monitor;
mod session;
mod watch;

use std::fmt::Debug;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::SystemTime;

use secs_compute::MemoryProbe;
use secs_types::ipc::{
    BootstrapInfo, Envelope, ErrorCode, IpcError, JobEvent, Platform, Request, UpstreamBase,
};

use calibration::Results;
pub use clock::LocalTime;
use jobs::JobRegistry;
use session::Session;

const UPSTREAM_TOML: &str = include_str!("../../../UPSTREAM.toml");

/// Product name shown to users.
pub const APP_NAME: &str = "SECS 2R";
/// The SECS 2R version (the workspace version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(serde::Deserialize)]
struct UpstreamFile {
    upstream: UpstreamBase,
}

/// The upstream release this build follows, parsed once from `UPSTREAM.toml`.
pub fn upstream() -> &'static UpstreamBase {
    static UPSTREAM: OnceLock<UpstreamBase> = OnceLock::new();
    UPSTREAM.get_or_init(|| {
        let file: UpstreamFile =
            toml::from_str(UPSTREAM_TOML).expect("UPSTREAM.toml is checked by the policy gates");
        file.upstream
    })
}

/// Receives pushed job events. Implementations must return quickly.
pub trait EventSink: Send + Sync + Debug {
    /// Pushes one event.
    fn send(&self, event: JobEvent);
}

/// Event sink for callers that deliberately ignore long-job events.
#[derive(Debug, Default)]
pub struct NoEvents;
impl EventSink for NoEvents {
    fn send(&self, _event: JobEvent) {}
}

/// Host facilities for dialogs, folder opening, paths, and REW launch.
pub trait Host: Send + Sync + Debug {
    /// Opens a project file picker.
    fn dialog_open_project(&self) -> Result<Option<String>, IpcError> {
        not_implemented("open-project dialog")
    }
    /// Opens a project save picker.
    fn dialog_save_project(&self, _suggested: &str) -> Result<Option<String>, IpcError> {
        not_implemented("save-project dialog")
    }
    /// Opens an mdat file picker.
    fn dialog_pick_mdat(&self) -> Result<Option<String>, IpcError> {
        not_implemented("mdat dialog")
    }
    /// Opens a folder picker.
    fn dialog_pick_folder(&self, _start: Option<&Path>) -> Result<Option<String>, IpcError> {
        not_implemented("folder dialog")
    }
    /// Opens a typed save-file picker.
    fn dialog_save_file(
        &self,
        _kind: secs_types::ipc::SaveFileKind,
        _suggested: &str,
    ) -> Result<Option<String>, IpcError> {
        not_implemented("save-file dialog")
    }
    /// Opens a typed file picker.
    fn dialog_open_file(
        &self,
        _kind: secs_types::ipc::OpenFileKind,
    ) -> Result<Option<String>, IpcError> {
        not_implemented("open-file dialog")
    }
    /// Reveals a folder in the operating-system file manager.
    fn open_folder(&self, _path: &Path) -> Result<(), IpcError> {
        not_implemented("open folder")
    }
    /// Returns the user's home directory for default export paths.
    fn home_dir(&self) -> Result<std::path::PathBuf, IpcError> {
        not_implemented("home directory")
    }
    /// Starts REW in API mode without a shell.
    fn launch_rew(&self) -> Result<(), IpcError> {
        not_implemented("host REW launch")
    }
}

fn not_implemented<T>(operation: &str) -> Result<T, IpcError> {
    Err(IpcError::new(
        ErrorCode::NotImplemented,
        format!("{operation} is not implemented"),
    ))
}
/// Host implementation used by headless callers.
#[derive(Debug, Default)]
pub struct NullHost;
impl Host for NullHost {}

/// A shared, stateful source of UUIDs for mdat measurements.
pub type UuidSource = Arc<Mutex<Box<dyn FnMut() -> Result<u128, secs_export::ExportError> + Send>>>;

/// Factories supplied by an application at the operating-system boundary.
///
/// P23a uses the memory probe for compute budgets. P23c connects the typed
/// audio, REW transport, and process seams to measurement handlers; injecting
/// them here keeps platform crates out of this service.
#[derive(Clone)]
pub struct Deps {
    /// Creates an audio backend adapter for one measurement operation.
    pub audio_backend: Arc<dyn Fn() -> Box<dyn secs_audio::AudioBackend> + Send + Sync>,
    /// Creates the sample-room measurer; tests may inject explicit delayed rooms.
    pub mock_measurer: Arc<dyn Fn() -> measure::MockMeasurer + Send + Sync>,
    /// Creates a REW transport adapter for one operation.
    pub rew_transport: Arc<dyn Fn() -> Box<dyn secs_rew::Transport> + Send + Sync>,
    /// Reports running processes without shelling out.
    pub process: Arc<dyn secs_rew::launch::ProcessProbe + Send + Sync>,
    /// Reports available process memory.
    pub memory: Arc<dyn MemoryProbe>,
    /// Application host operations.
    pub host: Arc<dyn Host>,
    /// Generates one UUID for each mdat measurement.
    pub uuid: UuidSource,
    /// The user's wall clock, for export folder names. The default reads UTC;
    /// the apps add the UTC offset `secs-sys` reports.
    pub clock: Arc<dyn Fn() -> LocalTime + Send + Sync>,
    /// Directory reserved for application settings and other durable host state.
    pub config_dir: std::path::PathBuf,
}

impl Debug for Deps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deps")
            .field("audio_backend", &"factory")
            .field("rew_transport", &"factory")
            .field("process", &"probe")
            .field("memory", &self.memory)
            .field("host", &self.host)
            .field("uuid", &"source")
            .field("clock", &"source")
            .field("config_dir", &self.config_dir)
            .finish()
    }
}

#[derive(Debug, Default)]
struct NoProcesses;
impl secs_rew::launch::ProcessProbe for NoProcesses {
    fn process_names(&self) -> Result<Vec<String>, secs_rew::RewError> {
        Ok(Vec::new())
    }
}

struct OfflineRew;
impl secs_rew::Transport for OfflineRew {
    fn request(
        &self,
        request: &secs_rew::Request,
    ) -> Result<secs_rew::Response, secs_rew::RewError> {
        Err(secs_rew::RewError::Connection {
            path: request.path.clone(),
            message: "no REW transport injected".into(),
        })
    }
}

fn random_uuid() -> Result<u128, secs_export::ExportError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| secs_export::ExportError::Random(error.to_string()))?;
    let mut value = u128::from_be_bytes(bytes);
    value &= !(0xf_u128 << 76);
    value |= 4_u128 << 76;
    value &= !(0x3_u128 << 62);
    value |= 0x2_u128 << 62;
    Ok(value)
}

impl Default for Deps {
    fn default() -> Self {
        Self {
            audio_backend: Arc::new(|| Box::new(secs_audio::NullBackend)),
            mock_measurer: Arc::new(measure::MockMeasurer::new),
            rew_transport: Arc::new(|| Box::new(OfflineRew)),
            process: Arc::new(NoProcesses),
            memory: Arc::new(secs_compute::FixedMemory(1024 * 1024 * 1024)),
            host: Arc::new(NullHost),
            uuid: Arc::new(Mutex::new(Box::new(random_uuid))),
            clock: Arc::new(|| LocalTime::from_system(SystemTime::now(), 0)),
            config_dir: std::path::PathBuf::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ServiceError {
    #[error("project is not open")]
    ProjectNotOpen,
    #[error("project I/O: {0}")]
    Project(String),
}

impl From<secs_project::ProjectError> for ServiceError {
    fn from(value: secs_project::ProjectError) -> Self {
        Self::Project(value.to_string())
    }
}
impl From<std::io::Error> for ServiceError {
    fn from(value: std::io::Error) -> Self {
        Self::Project(value.to_string())
    }
}
impl From<ServiceError> for IpcError {
    fn from(value: ServiceError) -> Self {
        match value {
            ServiceError::ProjectNotOpen => {
                IpcError::new(ErrorCode::ProjectNotOpen, value.to_string())
            }
            ServiceError::Project(_) => IpcError::new(ErrorCode::ProjectIo, value.to_string()),
        }
    }
}

/// The application service. One instance lives as long as the app.
#[derive(Debug)]
pub struct Service {
    deps: Deps,
    session: Arc<Mutex<Session>>,
    results_state: Arc<Mutex<Results>>,
    jobs: JobRegistry,
    level: monitor::LevelState,
    status_watch: watch::WatchState,
}

impl Service {
    /// Creates a service from platform dependency factories.
    pub fn new(deps: Deps) -> Self {
        Self {
            deps,
            session: Arc::new(Mutex::new(Session::default())),
            results_state: Arc::new(Mutex::new(Results::default())),
            jobs: JobRegistry::default(),
            level: monitor::LevelState::default(),
            status_watch: watch::WatchState::default(),
        }
    }

    /// Handles one parsed request. Long work starts a job and pushes events.
    pub fn call(&self, request: Request, events: Arc<dyn EventSink>) -> Envelope {
        ipc::dispatch(self, request, events)
    }

    /// Cancels every cancellable job and waits at most `timeout` for device access to stop.
    pub fn shutdown(&self, timeout: std::time::Duration) -> bool {
        self.jobs.cancel_all();
        self.jobs.wait_for_device(timeout)
    }

    /// Parses and handles an untyped request. Malformed input cannot cross the seam.
    pub fn call_value(&self, request: serde_json::Value, events: Arc<dyn EventSink>) -> Envelope {
        match serde_json::from_value::<Request>(request) {
            Ok(request) => self.call(request, events),
            Err(error) => {
                Envelope::Err(IpcError::new(ErrorCode::InvalidRequest, error.to_string()))
            }
        }
    }

    fn bootstrap(&self) -> BootstrapInfo {
        BootstrapInfo {
            app_name: APP_NAME.to_owned(),
            version: VERSION.to_owned(),
            prerelease: VERSION.starts_with("0."),
            upstream: upstream().clone(),
            platform: Platform {
                os: std::env::consts::OS.to_owned(),
                arch: std::env::consts::ARCH.to_owned(),
            },
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A prior panic poisons the advisory flag, not the protected value. Jobs
    // catch panics at their thread boundary, so retaining service state is safe.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
