//! IPC between the screen and the service.
//!
//! The screen sends a [`Request`] as `{"method": "...", "params": {...}}` and
//! always receives an envelope: `{"ok": true, "data": Response}` or
//! `{"ok": false, "error": IpcError}`. The service never throws across this
//! seam. Long jobs report progress through the event sink supplied with the call.
//!
//! Errors carry a code and parameters, not prose: the screen turns them into
//! a sentence in the current language and keeps `detail` (the raw text) for
//! the details view and the log.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::calibration::CalibrationSettings;
use crate::chain::{MimoPaths, SignalChain};

/// A call from the screen.
///
/// The method names come from the variant names in one place: serde and
/// strum both rename them to snake_case, and `method_names_match_the_wire`
/// checks that the two agree.
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, TS, strum::VariantNames, strum::IntoStaticStr,
)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum Request {
    /// Version, the upstream release this build is ported from, and the
    /// platform. The first call the screen makes.
    Bootstrap,
    /// The `ProjectNew` Request variant.
    ProjectNew(ProjectNewParams),
    /// The `ProjectOpen` Request variant.
    ProjectOpen(ProjectPathParams),
    /// The `ProjectSave` Request variant.
    ProjectSave(ProjectSaveParams),
    /// The `ProjectInfo` Request variant.
    ProjectInfo,
    /// The `SampleLoad` Request variant.
    SampleLoad(SampleLoadParams),
    /// The `Calibrate` Request variant.
    Calibrate(CalibrateParams),
    /// The `RecommendXo` Request variant.
    RecommendXo,
    /// The `Mimo` Request variant.
    Mimo(MimoParams),
    /// The `Results` Request variant.
    Results,
    /// The `Export` Request variant.
    Export(ExportParams),
    /// The `JobCancel` Request variant.
    JobCancel(JobCancelParams),
    /// The `Jobs` Request variant.
    Jobs,
    /// The `MeasureMethods` Request variant.
    MeasureMethods,
    /// The `MeasureConfigure` Request variant.
    MeasureConfigure(MeasureConfigureParams),
    /// The `LevelCheck` Request variant.
    LevelCheck(LevelCheckParams),
    /// The `LevelApply` Request variant.
    LevelApply(LevelApplyParams),
    /// The `MeasurePosition` Request variant.
    MeasurePosition(MeasurePositionParams),
    /// The `PositionClear` Request variant.
    PositionClear(PositionClearParams),
    /// The `Undo` Request variant.
    Undo(UndoParams),
    /// The `RewLaunch` Request variant.
    RewLaunch,
    /// The `MdatScan` Request variant.
    MdatScan(MdatScanParams),
    /// The `MdatImport` Request variant.
    MdatImport(MdatImportParams),
    /// Opens a project-file picker through the application host.
    DialogOpenProject,
    /// Opens a project save-file picker through the application host.
    DialogSaveProject(DialogSuggestedParams),
    /// Opens an mdat-file picker through the application host.
    DialogPickMdat,
    /// Opens a folder picker through the application host.
    DialogPickFolder(DialogPickFolderParams),
    /// Opens a typed save-file picker through the application host.
    DialogSaveFile(DialogSaveFileParams),
    /// Opens a typed file picker through the application host.
    DialogOpenFile(DialogOpenFileParams),
    /// Opens a folder in the operating-system file manager.
    OpenFolder(OpenFolderParams),
    /// Starts the live input level monitor.
    LevelMonitorStart(LevelMonitorStartParams),
    /// Stops the live input level monitor.
    LevelMonitorStop,
    /// Plays a role's level-check signal for 2.5 seconds.
    LevelPlay(LevelPlayParams),
    /// Starts watching measurement-method status.
    StatusWatchStart,
    /// Stops watching measurement-method status.
    StatusWatchStop,
    /// Returns the stored or derived export base folder.
    ExportFolderGet,
    /// Stores the export base folder in the open project.
    ExportFolderSet(ExportFolderSetParams),
}

impl Request {
    /// Every method name, as it appears on the wire, derived from the
    /// variants. `secs-policy` checks that each one is registered in
    /// `features.toml` as `ipc.<method>`.
    pub const METHODS: &'static [&'static str] = <Self as strum::VariantNames>::VARIANTS;

    /// The wire name of this request's method.
    pub fn method(&self) -> &'static str {
        self.into()
    }
}

/// The successful result of a [`Request`], tagged with the same method name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "method", content = "data", rename_all = "snake_case")]
#[ts(export)]
pub enum Response {
    /// The `Bootstrap` Response variant.
    Bootstrap(BootstrapInfo),
    /// The `ProjectNew` Response variant.
    ProjectNew(ProjectInfo),
    /// The `ProjectOpen` Response variant.
    ProjectOpen(ProjectInfo),
    /// The `ProjectSave` Response variant.
    ProjectSave(ProjectInfo),
    /// The `ProjectInfo` Response variant.
    ProjectInfo(ProjectInfo),
    /// The `SampleLoad` Response variant.
    SampleLoad(JobStarted),
    /// The `Calibrate` Response variant.
    Calibrate(JobStarted),
    /// The `RecommendXo` Response variant.
    RecommendXo(JobStarted),
    /// The `Mimo` Response variant.
    Mimo(JobStarted),
    /// The `Results` Response variant.
    Results(ResultsSummary),
    /// The `Export` Response variant.
    Export(JobStarted),
    /// The `JobCancel` Response variant.
    JobCancel(JobSnapshot),
    /// The `Jobs` Response variant.
    Jobs(Vec<JobSnapshot>),
    /// The `MeasureMethods` Response variant.
    MeasureMethods(MeasureMethodsResult),
    /// The `MeasureConfigure` Response variant.
    MeasureConfigure(MeasureConfigResult),
    /// The `LevelCheck` Response variant.
    LevelCheck(JobStarted),
    /// The `LevelApply` Response variant.
    LevelApply(MeasureConfigResult),
    /// The `MeasurePosition` Response variant.
    MeasurePosition(JobStarted),
    /// The `PositionClear` Response variant.
    PositionClear(UndoResult),
    /// The `Undo` Response variant.
    Undo(ProjectInfo),
    /// The `RewLaunch` Response variant.
    RewLaunch(RewStatusResult),
    /// The `MdatScan` Response variant.
    MdatScan(MdatScanResult),
    /// The `MdatImport` Response variant.
    MdatImport(JobStarted),
    /// The chosen project path, or null when cancelled.
    DialogOpenProject(Option<String>),
    /// The chosen project destination, or null when cancelled.
    DialogSaveProject(Option<String>),
    /// The chosen mdat path, or null when cancelled.
    DialogPickMdat(Option<String>),
    /// The chosen folder, or null when cancelled.
    DialogPickFolder(Option<String>),
    /// The chosen save destination, or null when cancelled.
    DialogSaveFile(Option<String>),
    /// The chosen source path, or null when cancelled.
    DialogOpenFile(Option<String>),
    /// Confirms that the folder-open request reached the host.
    OpenFolder(()),
    /// The live level-monitor job.
    LevelMonitorStart(JobStarted),
    /// The stopped live level-monitor job, when one was running.
    LevelMonitorStop(Option<JobSnapshot>),
    /// Confirms that role playback was accepted.
    LevelPlay(()),
    /// The measurement-status watch job.
    StatusWatchStart(JobStarted),
    /// The stopped status-watch job, when one was running.
    StatusWatchStop(Option<JobSnapshot>),
    /// The current export base folder.
    ExportFolderGet(ExportFolderResult),
    /// The stored export base folder.
    ExportFolderSet(ExportFolderResult),
}

/// What the screen needs before it draws anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BootstrapInfo {
    /// Product name shown in the title bar and the about view.
    pub app_name: String,
    /// The SECS 2R version (the workspace version).
    pub version: String,
    /// True while the major version is 0: every 0.x build is a prerelease.
    pub prerelease: bool,
    /// The SECS 2 release this build follows.
    pub upstream: UpstreamBase,
    /// The operating-system platform running this build.
    pub platform: Platform,
}
/// The upstream SECS 2 release a build is ported from (`UPSTREAM.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpstreamBase {
    /// Upstream product name from `UPSTREAM.toml`.
    pub name: String,
    /// Upstream release version from `UPSTREAM.toml`.
    pub version: String,
    /// Forty-character upstream Git commit from `UPSTREAM.toml`.
    pub commit: String,
    /// Upstream snapshot date from `UPSTREAM.toml`.
    pub date: String,
}
/// The operating-system platform running the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Platform {
    /// `windows`, `macos` or `linux` (`std::env::consts::OS`).
    pub os: String,
    /// `x86_64` or `aarch64` (`std::env::consts::ARCH`).
    pub arch: String,
}

/// Inputs for creating a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectNewParams {
    /// Destination `.secs` file path.
    pub path: String,
    /// Speaker-layout preset name.
    pub preset: String,
    /// Number of subwoofers.
    pub sub_count: u8,
    /// Whether to create the fixed 7.1.4 sample-room project.
    pub sample: bool,
}
/// Inputs for loading embedded or synthetic sample measurements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct SampleLoadParams {
    /// Synthetic position count; absent loads the one-position embedded room.
    pub positions: Option<u32>,
}
/// A project file path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectPathParams {
    /// Source `.secs` file path.
    pub path: String,
}
/// Inputs for saving the open project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct ProjectSaveParams {
    /// Optional save-as `.secs` path; absent saves in place.
    pub path: Option<String>,
}
/// Screen-facing information about the open project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectInfo {
    /// Open `.secs` file path.
    pub path: String,
    /// Project display name.
    pub name: String,
    /// Speaker-layout preset name.
    pub preset: String,
    /// Number of configured subwoofers.
    pub sub_count: usize,
    /// Measured position numbers.
    pub positions: Vec<i64>,
    /// Whether the in-memory project has unsaved changes.
    pub dirty: bool,
    /// Warnings produced while loading legacy project entries.
    pub migration_warnings: Vec<MigrationWarning>,
}
/// A project migration warning code and affected entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MigrationWarning {
    /// Stable warning code translated by the screen.
    pub code: String,
    /// Project entry affected by the warning.
    pub entry: String,
}

/// Optional settings for a calibration run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct CalibrateParams {
    /// Settings override; absent uses the open project.
    pub settings: Option<CalibrationSettings>,
}
/// Optional settings for a MIMO run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct MimoParams {
    /// Settings override; absent uses the open project.
    pub settings: Option<CalibrationSettings>,
}

/// A service-assigned long-running job identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export)]
pub struct JobId(#[ts(type = "number")] pub u64);
/// The operation performed by a long-running job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum JobKind {
    /// Checks the level of one configured speaker role.
    LevelCheck,
    /// Records all configured roles at one measurement position.
    Measure,
    /// Loads embedded or deterministic synthetic sample measurements.
    SampleLoad,
    /// Imports assigned measurements from a REW mdat file.
    MdatImport,
    /// Computes a calibration design from project measurements.
    Calibrate,
    /// Scores crossover-frequency candidates.
    RecommendXo,
    /// Computes MIMO support paths.
    Mimo,
    /// Writes one selected export bundle.
    Export,
    /// Streams microphone levels and controls level-check playback.
    LevelMonitor,
    /// Watches measurement-method status changes.
    StatusWatch,
}
/// The current lifecycle state of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum JobStatus {
    /// The worker is still running and may accept cancellation.
    Running,
    /// Cancellation was requested and the worker has not stopped yet.
    CancelRequested,
    /// The worker completed and produced a result.
    Succeeded,
    /// The worker stopped with an error.
    Failed,
    /// The caller cancelled the operation.
    Cancelled,
}
impl JobStatus {
    /// Whether the job can no longer change state.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}
/// The immediate response after a job starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct JobStarted {
    /// Identifier assigned to the job.
    pub job: JobId,
    /// Operation the job performs.
    pub kind: JobKind,
}
/// The latest observable state of a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct JobSnapshot {
    /// Identifier assigned to the job.
    pub job: JobId,
    /// Operation the job performs.
    pub kind: JobKind,
    /// Current lifecycle state.
    pub status: JobStatus,
    /// Whether a cancellation request can still prevent commit.
    pub cancellable: bool,
    /// Most recently reported stage progress.
    pub progress: Option<JobProgress>,
    /// Successful result once finished.
    pub result: Option<JobResult>,
    /// Failure once finished unsuccessfully.
    pub error: Option<IpcError>,
}
/// Progress within one named job stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct JobProgress {
    /// Stable stage code translated by the screen.
    pub stage: String,
    #[ts(type = "number")]
    /// Completed work units in this stage.
    pub done: u64,
    #[ts(type = "number")]
    /// Total work units in this stage.
    pub total: u64,
}
/// A pushed lifecycle event for a long-running job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "snake_case")]
#[ts(export)]
pub enum JobEvent {
    /// Announces a newly accepted job and its operation.
    Started { job: JobId, kind: JobKind },
    /// Reports completed and total work units for a stable stage code.
    Progress {
        job: JobId,
        stage: String,
        #[ts(type = "number")]
        done: u64,
        #[ts(type = "number")]
        total: u64,
    },
    /// Reports a presentation-neutral log code and template parameters.
    Log {
        job: JobId,
        code: String,
        params: BTreeMap<String, String>,
    },
    /// Reports a terminal status with either a result or an error.
    Finished {
        job: JobId,
        status: JobStatus,
        result: Option<Box<JobResult>>,
        error: Option<IpcError>,
    },
    /// Reports a live input level at the monitor cadence.
    Meter {
        job: JobId,
        rms_dbfs: f64,
        peak_dbfs: f64,
        spl_db: Option<f64>,
        clipping: bool,
    },
    /// Reports a changed measurement-method status snapshot.
    MeasureStatus {
        job: JobId,
        status: MeasureMethodsResult,
    },
}
/// The successful result produced by a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
#[ts(export)]
pub enum JobResult {
    /// Calibration result plus the latest compatible MIMO result.
    Calibration(Box<ResultsSummary>),
    /// Crossover recommendation computed for the current project.
    Xo(XoRecommendation),
    /// MIMO paths computed for the current calibration.
    Mimo(MimoSummary),
    /// Paths written by an exporter.
    Export(ExportResult),
    /// Measured levels and recommended sweep level.
    Level(LevelResult),
    /// Project state after a completed measurement.
    Measurement(ProjectInfo),
    /// Project state after an mdat import.
    MdatImport(ProjectInfo),
    /// A long-lived monitor or watcher stopped without a payload.
    Stopped,
}
/// The job to cancel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct JobCancelParams {
    /// Identifier of the job to cancel.
    pub job: JobId,
}

/// The latest calibration and MIMO results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ResultsSummary {
    /// Latest calibration result, including stale state.
    pub calibration: Option<CalibrationSummary>,
    /// Latest MIMO result, including stale state.
    pub mimo: Option<MimoSummary>,
}
/// A presentation-neutral note or warning code with template parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Notice {
    /// Stable snake_case code derived from the engine note variant.
    pub code: String,
    /// String values for the screen message template.
    pub params: BTreeMap<String, String>,
}

/// The screen-facing summary of one calibration design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CalibrationSummary {
    /// Whether current project inputs differ from this result.
    pub stale: bool,
    /// Values for the upstream verdict card.
    pub verdict: VerdictSummary,
    /// Per-channel before-and-after metrics.
    pub scorecard: Vec<ChannelScore>,
    /// Total filter latency in milliseconds.
    pub latency_ms: f64,
    /// Warning codes and values translated by the screen.
    pub warnings: Vec<Notice>,
    /// Informational codes and values translated by the screen.
    pub notes: Vec<Notice>,
    /// Designed DSP signal chain used by exporters.
    pub chain: SignalChain,
}
/// Values shown by the upstream `step_calibration.py` verdict card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VerdictSummary {
    /// Share within ±1 dB before correction, in percent.
    pub within_1db_before_pct: f64,
    /// Share within ±1 dB after correction, in percent.
    pub within_1db_after_pct: f64,
    /// Low-band group-delay spread before correction, in milliseconds.
    pub low_band_time_spread_before_ms: f64,
    /// Low-band group-delay spread after correction, in milliseconds.
    pub low_band_time_spread_after_ms: f64,
    /// Delay added by correction, in milliseconds.
    pub added_delay_ms: f64,
    /// Across-position level spread after correction, in dB.
    pub position_spread_db: f64,
}
/// Before-and-after scorecard metrics for one speaker role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChannelScore {
    /// Internal speaker role.
    pub role: String,
    /// Metrics before correction.
    pub before: MetricsSummary,
    /// Metrics after correction.
    pub after: MetricsSummary,
}
/// The upstream `Metrics` fields for one response.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MetricsSummary {
    /// Reference level from upstream `Metrics.level`, in dB.
    pub level: f64,
    /// Share within ±1 dB from 20 Hz, in percent.
    pub in1_20: f64,
    /// Share within ±2 dB from 20 Hz, in percent.
    pub in2_20: f64,
    /// Share within ±1 dB from 200 Hz, in percent.
    pub in1_200: f64,
    /// Share within ±2 dB from 200 Hz, in percent.
    pub in2_200: f64,
    /// Share within ±1 dB from 2,000 Hz, in percent.
    pub in1_2000: f64,
    /// Share within ±2 dB from 2,000 Hz, in percent.
    pub in2_2000: f64,
    /// Phase rotation from upstream `Metrics.turns`, in turns.
    pub turns: f64,
    /// Group-delay spread near 20 Hz, in milliseconds.
    pub gd_20: f64,
    /// Group-delay spread near 100 Hz, in milliseconds.
    pub gd_100: f64,
    /// Group-delay spread near 500 Hz, in milliseconds.
    pub gd_500: f64,
    /// Largest absolute 1/24-octave group-delay peak, in milliseconds.
    pub gd24_abs: f64,
    /// Frequency of the largest absolute group-delay peak, in Hz.
    pub gd24_abs_hz: f64,
    /// Largest signed 1/24-octave group-delay peak, in milliseconds.
    pub gd24_max: f64,
    /// Span of 1/24-octave group delay, in milliseconds.
    pub gd24_span: f64,
    /// Group-delay spread near 30 Hz, in milliseconds.
    pub gd_30: f64,
    /// Pre-ringing level from upstream `Metrics.pre_db`, in dB.
    pub pre_db: f64,
}
/// The screen-facing summary of one MIMO run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MimoSummary {
    /// Whether current calibration inputs differ from this MIMO result.
    pub stale: bool,
    /// Realization kind, `iir` or `fir`.
    pub kind: String,
    /// Upper support frequency in Hz.
    pub fsiso: f64,
    /// Number of generated support paths.
    pub support_count: usize,
    /// Informational codes and values translated by the screen.
    pub notes: Vec<Notice>,
    /// DSP-ready support paths.
    pub paths: MimoPaths,
}
/// Crossover candidates and the best value for each group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct XoRecommendation {
    /// Best crossover frequency in Hz for each speaker group.
    pub best: Vec<(String, f64)>,
    /// Evaluated crossover frequencies in Hz.
    pub candidates: Vec<f64>,
}

/// The supported export bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExportKind {
    /// CamillaDSP configuration and FIR coefficient WAV files.
    CamillaDsp,
    /// REW mdat containing the upstream staged responses.
    StagedMdat,
    /// REW mdat containing before-and-after responses.
    BeforeAfterMdat,
}
/// The export bundle and destination folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExportParams {
    /// Export bundle to write.
    pub kind: ExportKind,
    /// Destination folder path. An empty path uses the project's export folder
    /// and applies the upstream timestamped subfolder convention.
    pub folder: String,
}
/// Files written by a completed export job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExportResult {
    /// Paths of all files written by the exporter.
    pub files: Vec<String>,
}

// P28 host, live-monitor, watch and export-folder types. Keep this block
// contiguous so the parallel M5 IPC additions merge without interleaving.
/// Optional suggested name for a save dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct DialogSuggestedParams {
    /// Suggested file name.
    pub suggested: String,
}
/// Optional starting folder for a folder picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct DialogPickFolderParams {
    /// Starting folder, when known.
    pub start: Option<String>,
}
/// The kind of file a host save dialog accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SaveFileKind {
    /// UTF-8 patch-sheet CSV.
    PatchSheet,
    /// Target-curve JSON.
    Target,
}
/// Typed save-file dialog parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DialogSaveFileParams {
    /// File type and filter.
    pub kind: SaveFileKind,
    /// Suggested file name.
    pub suggested: String,
}
/// The kind of file a host open dialog accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum OpenFileKind {
    /// Target-curve JSON.
    Target,
}
/// Typed open-file dialog parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DialogOpenFileParams {
    /// File type and filter.
    pub kind: OpenFileKind,
}
/// Folder to reveal in the operating-system file manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OpenFolderParams {
    /// Folder path.
    pub path: String,
}
/// Optional role to associate with a level-monitor start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct LevelMonitorStartParams {
    /// Speaker role initially selected for playback.
    pub role: Option<String>,
}
/// Role and output level for one 2.5-second level-check burst.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LevelPlayParams {
    /// Internal speaker role.
    pub role: String,
    /// Master output level in dBFS.
    pub master_dbfs: f64,
}
/// Export base-folder update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExportFolderSetParams {
    /// Folder path to store in the open project.
    pub folder: String,
}
/// Current export base folder and whether it came from project storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExportFolderResult {
    /// Export base folder.
    pub folder: String,
    /// True when `extra.export.folder` supplied this value.
    pub stored: bool,
}

// Measurement request and response types.
/// Measurement methods available on this host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct MeasureMethodsResult {
    /// Stable identifiers for available measurement methods.
    pub methods: Vec<String>,
    /// Native endpoints with their channel counts.
    pub native_devices: Vec<MeasureDevice>,
    /// Native enumeration failure, without hiding the other methods.
    pub native_error: Option<IpcError>,
    /// REW process, API, and license status.
    pub rew: RewStatusResult,
    /// Positions present in the unmodified sample room.
    pub mock_positions: Vec<i64>,
    /// Roles present in the unmodified sample room.
    pub mock_roles: Vec<String>,
}
/// An enumerated native audio endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MeasureDevice {
    /// Backend endpoint identifier.
    pub id: String,
    /// Backend endpoint display name.
    pub name: String,
    /// Maximum number of input channels.
    pub input_channels: usize,
    /// Maximum number of output channels.
    pub output_channels: usize,
}
/// Measurement method and device settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct MeasureConfigureParams {
    /// Measurement method identifier.
    pub method: String,
    /// Optional input device identifier.
    pub input_device: Option<String>,
    /// Optional output device identifier.
    pub output_device: Option<String>,
    /// Optional one-based input channel number.
    pub input_channel: Option<u16>,
    /// One-based output channels in measurement-plan role order; empty preserves wiring.
    #[serde(default)]
    pub output_channels: Vec<u16>,
    /// Optional sweep length in samples.
    pub sweep_length: Option<usize>,
    /// Optional sweep level in dBFS.
    pub sweep_level_dbfs: Option<f64>,
    /// Optional timing-reference mode.
    pub timing: Option<String>,
    /// Optional one-based loopback input channel.
    pub loopback_input_channel: Option<u16>,
    /// Optional acoustic timing-reference output channel.
    pub timing_ref_output: Option<u16>,
    /// Optional share preference: auto, exclusive, or shared.
    pub share_mode: Option<String>,
}
/// The active measurement settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct MeasureConfigResult {
    /// Active measurement method identifier.
    pub method: String,
    /// Active sweep level in dBFS.
    pub sweep_level_dbfs: Option<f64>,
    /// Active input endpoint identifier; empty selects the default.
    pub input_device: String,
    /// Active output endpoint identifier; empty selects the default.
    pub output_device: String,
    /// One-based microphone input channel.
    pub input_channel: u16,
    /// Roles in the same order as output_channels.
    pub roles: Vec<String>,
    /// One-based output channels in measurement-plan role order.
    pub output_channels: Vec<u16>,
    /// Sweep length in samples.
    pub sweep_length: usize,
    /// Timing-reference mode.
    pub timing: String,
    /// Optional one-based loopback input channel.
    pub loopback_input_channel: Option<u16>,
    /// Optional acoustic timing-reference output channel.
    pub timing_ref_output: Option<u16>,
    /// Share preference: auto, exclusive, or shared.
    pub share_mode: String,
}
/// The speaker role used for a level check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LevelCheckParams {
    /// Internal speaker role to check.
    pub role: String,
}
/// A measured level to apply to sweep setup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LevelApplyParams {
    /// Measured level result to apply.
    pub level: LevelResult,
}
/// Measured signal levels and the recommended sweep level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LevelResult {
    /// Measured RMS level in dBFS.
    pub rms_dbfs: f64,
    /// Measured peak level in dBFS.
    pub peak_dbfs: f64,
    /// Optional measured sound pressure level in dB SPL.
    pub spl_db: Option<f64>,
    /// Recommended sweep level in dBFS.
    pub recommended_sweep_dbfs: f64,
}
/// The measurement position to record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MeasurePositionParams {
    /// One-based measurement position number.
    pub position: i64,
}
/// The measurement position to clear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PositionClearParams {
    /// One-based measurement position number.
    pub position: i64,
}
/// A service-issued undo token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UndoParams {
    /// Opaque service-issued undo token.
    pub token: String,
}
/// An undo token and the project state it restores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UndoResult {
    /// Opaque token for the inverse action.
    pub token: String,
    /// Project state after the action.
    pub project: ProjectInfo,
}
/// Whether REW is running, reachable, and licensed as Pro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct RewStatusResult {
    /// Whether an REW process is running.
    pub running: bool,
    /// Whether the REW API responds.
    pub reachable: bool,
    /// Whether REW reports a Pro license.
    pub pro: bool,
}
/// The REW mdat file to scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MdatScanParams {
    /// Source `.mdat` file path.
    pub path: String,
}
/// Measurements, suggested assignments, and problem codes found in an mdat file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct MdatScanResult {
    /// Measurements in file order, including assignment guesses.
    pub measurements: Vec<MdatScanMeasurement>,
    /// Suggested measurement-to-role assignments.
    pub suggestions: Vec<MdatAssignment>,
    /// Stable problem codes and template parameters.
    pub problems: Vec<Notice>,
}
/// One scanned measurement with an unambiguous row identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MdatScanMeasurement {
    /// Zero-based row index, also usable when titles repeat.
    pub index: usize,
    /// Original measurement title.
    pub title: String,
    /// Suggested internal role, or no assignment.
    pub role: Option<String>,
    /// Guessed one-based measurement position.
    pub position: i64,
    /// Low-band classifier result, absent when no IR is available.
    pub looks_like_sub: Option<bool>,
}
/// An mdat measurement assigned to a speaker role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MdatAssignment {
    /// Measurement title from the mdat file.
    pub measurement: String,
    /// Internal speaker role.
    pub role: String,
    /// Optional zero-based row index to disambiguate repeated titles.
    pub index: Option<usize>,
    /// Optional per-row position overriding the import destination.
    pub position: Option<i64>,
}
/// An mdat file, its assignments, and destination position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MdatImportParams {
    /// Source `.mdat` file path.
    pub path: String,
    /// Accepted measurement-to-role assignments.
    pub assignments: Vec<MdatAssignment>,
    /// Destination measurement position number.
    pub position: i64,
}

/// Why a call failed. The screen maps each code to a written message with the
/// cause and the next action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorCode {
    /// The request did not parse or named an unknown method.
    InvalidRequest,
    /// A bug: the service failed in a way it does not expect.
    Internal,
    /// The method is registered but not connected yet.
    NotImplemented,
    /// The call requires an open project.
    ProjectNotOpen,
    /// Opening, reading, or writing project data failed.
    ProjectIo,
    /// Another job in the same group is running.
    JobBusy,
    /// The named job does not exist.
    JobNotFound,
    /// The caller cancelled the operation.
    Cancelled,
    /// Project measurements or settings are not valid engine inputs.
    EngineInput,
    /// The requested exporter failed.
    ExportFailed,
    /// Cached results no longer match project inputs.
    StaleInputs,
    /// The audio backend or selected endpoint is unavailable.
    DeviceUnavailable,
    /// The backend refused a pinned audio share mode.
    ShareModeRefused,
    /// The REW API cannot be reached.
    RewUnreachable,
    /// REW requires a Pro license for this operation.
    RewProRequired,
    /// REW returned an HTTP or schema failure.
    RewFailed,
    /// A measurement setting or plan field is invalid.
    MeasurementInput,
}
/// A coded IPC failure with template parameters and diagnostic detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IpcError {
    /// Stable error code translated by the screen.
    pub code: ErrorCode,
    /// Values for the message template of `code` (file names, counts, ...).
    pub params: BTreeMap<String, String>,
    /// Whether trying the same call again can succeed.
    pub retryable: bool,
    /// The raw error text, for the details view and the log only.
    pub detail: String,
}
impl IpcError {
    /// Creates a non-retryable error without template parameters.
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            params: BTreeMap::new(),
            retryable: false,
            detail: detail.into(),
        }
    }
    /// Adds one screen message-template parameter.
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

/// The reply to every call: `{"ok": true, "data": ...}` or
/// `{"ok": false, "error": ...}`.
///
/// The TypeScript side declares the same union in `src/ipc/client.ts`
/// (`Envelope`); `envelope_wire_shape` below pins the JSON both sides rely on.
#[derive(Debug, Clone, PartialEq)]
pub enum Envelope {
    /// The `Ok` Envelope variant.
    Ok(Box<Response>),
    /// The `Err` Envelope variant.
    Err(IpcError),
}
impl Serialize for Envelope {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Envelope", 2)?;
        match self {
            Self::Ok(data) => {
                state.serialize_field("ok", &true)?;
                state.serialize_field("data", data)?;
            }
            Self::Err(error) => {
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", error)?;
            }
        }
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One value of every variant. A new variant fails to compile here until
    /// it gets an example, which `method_names_match_the_wire` then checks.
    fn examples() -> Vec<Request> {
        vec![
            Request::Bootstrap,
            Request::ProjectNew(ProjectNewParams {
                path: "x.secs".into(),
                preset: "2.1".into(),
                sub_count: 1,
                sample: false,
            }),
            Request::ProjectOpen(ProjectPathParams {
                path: "x.secs".into(),
            }),
            Request::ProjectSave(ProjectSaveParams::default()),
            Request::ProjectInfo,
            Request::SampleLoad(SampleLoadParams::default()),
            Request::Calibrate(CalibrateParams::default()),
            Request::RecommendXo,
            Request::Mimo(MimoParams::default()),
            Request::Results,
            Request::Export(ExportParams {
                kind: ExportKind::CamillaDsp,
                folder: ".".into(),
            }),
            Request::JobCancel(JobCancelParams { job: JobId(1) }),
            Request::Jobs,
            Request::MeasureMethods,
            Request::MeasureConfigure(MeasureConfigureParams::default()),
            Request::LevelCheck(LevelCheckParams { role: "L".into() }),
            Request::LevelApply(LevelApplyParams {
                level: LevelResult {
                    rms_dbfs: -20.0,
                    peak_dbfs: -10.0,
                    spl_db: None,
                    recommended_sweep_dbfs: -20.0,
                },
            }),
            Request::MeasurePosition(MeasurePositionParams { position: 1 }),
            Request::PositionClear(PositionClearParams { position: 1 }),
            Request::Undo(UndoParams { token: "x".into() }),
            Request::RewLaunch,
            Request::MdatScan(MdatScanParams {
                path: "x.mdat".into(),
            }),
            Request::MdatImport(MdatImportParams {
                path: "x.mdat".into(),
                assignments: vec![],
                position: 1,
            }),
            Request::DialogOpenProject,
            Request::DialogSaveProject(DialogSuggestedParams {
                suggested: "x.secs".into(),
            }),
            Request::DialogPickMdat,
            Request::DialogPickFolder(DialogPickFolderParams { start: None }),
            Request::DialogSaveFile(DialogSaveFileParams {
                kind: SaveFileKind::PatchSheet,
                suggested: "x.csv".into(),
            }),
            Request::DialogOpenFile(DialogOpenFileParams {
                kind: OpenFileKind::Target,
            }),
            Request::OpenFolder(OpenFolderParams { path: ".".into() }),
            Request::LevelMonitorStart(LevelMonitorStartParams::default()),
            Request::LevelMonitorStop,
            Request::LevelPlay(LevelPlayParams {
                role: "L".into(),
                master_dbfs: -20.0,
            }),
            Request::StatusWatchStart,
            Request::StatusWatchStop,
            Request::ExportFolderGet,
            Request::ExportFolderSet(ExportFolderSetParams { folder: ".".into() }),
        ]
    }
    #[test]
    fn method_names_match_the_wire() {
        let examples = examples();
        assert_eq!(examples.len(), Request::METHODS.len());
        for request in examples {
            let wire = serde_json::to_value(&request).unwrap();
            assert_eq!(wire["method"], request.method());
        }
    }
    #[test]
    fn every_method_name_is_listed_and_parses() {
        for request in examples() {
            let wire = serde_json::to_value(&request).unwrap();
            let parsed: Request = serde_json::from_value(wire).unwrap();
            assert_eq!(parsed.method(), request.method());
        }
    }
    #[test]
    fn envelope_wire_shape() {
        let ok = Envelope::Ok(Box::new(Response::Bootstrap(BootstrapInfo {
            app_name: "SECS 2R".into(),
            version: "0.0.1".into(),
            prerelease: true,
            upstream: UpstreamBase {
                name: "SECS".into(),
                version: "Beta".into(),
                commit: "0".repeat(40),
                date: "2026-10-12".into(),
            },
            platform: Platform {
                os: "linux".into(),
                arch: "x86_64".into(),
            },
        })));
        let value = serde_json::to_value(ok).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["data"]["method"], "bootstrap");
        let value = serde_json::to_value(Envelope::Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "bad",
        )))
        .unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "invalid_request");
    }
}
