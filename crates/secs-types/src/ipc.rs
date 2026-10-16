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

use crate::calibration::{CalibrationSettings, Mode};
use crate::chain::{MimoPaths, SignalChain};
pub use crate::settings_edit::CalibrationSettingsEditParams;

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
    /// Returns the measurement settings stored in the open project.
    MeasureConfigGet,
    /// Returns one recording-status entry per arranged position.
    MeasurementRecords,
    /// Returns stored per-role level and SPL values.
    LevelsGet,
    /// Fills only missing per-role level values with upstream defaults.
    LevelsFillDefaults,
    /// Stops the current 2.5-second level burst without stopping its monitor.
    LevelStop,
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
    /// Returns the editable speaker configuration.
    ConfigGet,
    /// Replaces the speaker configuration with a built-in preset.
    ConfigApplyPreset(ConfigPresetParams),
    /// Changes the number of configured subwoofers.
    ConfigSetSubCount(ConfigSubCountParams),
    /// Changes one speaker or one of its bands.
    ConfigSetSpeaker(ConfigSetSpeakerParams),
    /// Adds the first unused upstream speaker role.
    ConfigAddSpeaker,
    /// Removes one configured speaker.
    ConfigRemoveSpeaker(ConfigIndexParams),
    /// Renumbers outputs in configuration order.
    ConfigRenumber,
    /// Returns structured configuration validation notices.
    ConfigValidate,
    /// Writes the upstream patch-sheet CSV.
    PatchSheetCsv(PathParams),
    /// Returns the editable DSP profile.
    DspGet,
    /// Applies a DSP preset or manual fields.
    DspSet(DspSetParams),
    /// Returns measurement arrangement and timing options.
    LayoutGet,
    /// Changes measurement arrangement or timing options.
    LayoutSet(LayoutSetParams),
    /// Returns the current per-position and total estimate.
    PositionEstimate,
    /// Returns the target curve and presets.
    TargetGet,
    /// Replaces target control points.
    TargetSetPoints(TargetPointsParams),
    /// Applies a built-in target preset.
    TargetApplyPreset(TargetPresetParams),
    /// Saves the target curve as upstream JSON.
    TargetSave(PathParams),
    /// Loads the target curve from upstream JSON.
    TargetLoad(PathParams),
    /// Returns calibration settings stored in the project.
    CalibrationSettingsGet,
    /// Stores calibration settings in the project.
    CalibrationSettingsSet(CalibrationSettingsParams),
    /// Applies one upstream calibration-screen edit and stores the settings.
    CalibrationSettingsEdit(CalibrationSettingsEditParams),
    /// Filter warnings from current settings (gain warnings require a run).
    SubWarnings,
    /// Optional REW facts read through the injected transport.
    RewInfo,
    /// Returns numeric capabilities for one calibration mode.
    ModeSpec(ModeSpecParams),
    /// Returns application-local settings.
    AppSettingsGet,
    /// Stores application-local settings.
    AppSettingsSet(AppSettingsParams),
    /// Smoothed, normalized measurement curves and their shared frequency grid.
    MeasurementCurves(MeasurementCurvesParams),
    /// Ready-to-draw calibration curves and axis limits for one result tab.
    ResultCurves(ResultCurvesParams),
    /// Numeric metrics, filter counts, BA rows and coded result notices.
    ResultLines,
    /// Copies the last BA output alignment into the manual sub settings.
    SubTakeValues,
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
    /// Measurement settings stored in the open project.
    MeasureConfigGet(MeasureConfigResult),
    /// Recording status for every arranged position.
    MeasurementRecords(MeasurementRecordsResult),
    /// Stored per-role level and SPL values.
    LevelsGet(LevelsResult),
    /// Stored values after filling missing level defaults.
    LevelsFillDefaults(LevelsResult),
    /// Confirms that the current level burst was stopped.
    LevelStop(()),
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
    /// The `ConfigGet` Response variant.
    ConfigGet(ConfigResult),
    /// The `ConfigApplyPreset` Response variant.
    ConfigApplyPreset(ConfigMutationResult),
    /// The `ConfigSetSubCount` Response variant.
    ConfigSetSubCount(ConfigMutationResult),
    /// The `ConfigSetSpeaker` Response variant.
    ConfigSetSpeaker(ConfigResult),
    /// The `ConfigAddSpeaker` Response variant.
    ConfigAddSpeaker(ConfigResult),
    /// The `ConfigRemoveSpeaker` Response variant.
    ConfigRemoveSpeaker(ConfigResult),
    /// The `ConfigRenumber` Response variant.
    ConfigRenumber(ConfigResult),
    /// The `ConfigValidate` Response variant.
    ConfigValidate(Vec<Notice>),
    /// The `PatchSheetCsv` Response variant.
    PatchSheetCsv(PathResult),
    /// The `DspGet` Response variant.
    DspGet(DspResult),
    /// The `DspSet` Response variant.
    DspSet(DspResult),
    /// The `LayoutGet` Response variant.
    LayoutGet(LayoutResult),
    /// The `LayoutSet` Response variant.
    LayoutSet(LayoutMutationResult),
    /// The `PositionEstimate` Response variant.
    PositionEstimate(PositionEstimateResult),
    /// The `TargetGet` Response variant.
    TargetGet(TargetResult),
    /// The `TargetSetPoints` Response variant.
    TargetSetPoints(TargetResult),
    /// The `TargetApplyPreset` Response variant.
    TargetApplyPreset(TargetResult),
    /// The `TargetSave` Response variant.
    TargetSave(PathResult),
    /// The `TargetLoad` Response variant.
    TargetLoad(TargetResult),
    /// The `CalibrationSettingsGet` Response variant.
    CalibrationSettingsGet(CalibrationSettings),
    /// The `CalibrationSettingsSet` Response variant.
    CalibrationSettingsSet(CalibrationSettings),
    /// Settings after one edit.
    CalibrationSettingsEdit(CalibrationSettings),
    /// Current settings-only filter warnings.
    SubWarnings(Vec<Notice>),
    /// Current REW facts; unavailable fields are null.
    RewInfo(RewInfoResult),
    /// The `ModeSpec` Response variant.
    ModeSpec(ModeSpecResult),
    /// The `AppSettingsGet` Response variant.
    AppSettingsGet(AppSettings),
    /// The `AppSettingsSet` Response variant.
    AppSettingsSet(AppSettings),
    /// Measurement curves on the common display grid.
    MeasurementCurves(MeasurementCurvesResult),
    /// Calibration tab curves, ranges and staleness.
    ResultCurves(ResultCurvesResult),
    /// Numeric summary lines and BA table.
    ResultLines(ResultLinesResult),
    /// Stored settings after taking the last sub alignment.
    SubTakeValues(CalibrationSettings),
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

/// Optional measurement selection; an absent selection includes every recorded pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct MeasurementCurvesParams {
    /// Position numbers to include.
    #[ts(type = "number[] | null")]
    pub positions: Option<Vec<i64>>,
    /// Speaker roles to include.
    pub roles: Option<Vec<String>>,
}
/// A measurement curve, already smoothed and normalized to the mid-band mean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MeasurementCurve {
    /// Speaker role.
    pub role: String,
    /// Measurement position.
    #[ts(type = "number")]
    pub position: i64,
    /// Configuration kind: main, sub or top.
    pub kind: String,
    /// Levels in dB on the shared grid.
    pub values: Vec<f64>,
}
/// Display limits; both endpoints are data values, not logarithms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CurveAxes {
    /// Frequency bounds in Hz.
    pub frequency_hz: [f64; 2],
    /// Level or delay bounds.
    pub value: [f64; 2],
    /// Vertical unit: db or ms.
    pub unit: String,
}
/// Measurement graph data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MeasurementCurvesResult {
    /// Shared 280-point logarithmic frequency grid in Hz.
    pub grid: Vec<f64>,
    /// One curve per recorded role and position.
    pub curves: Vec<MeasurementCurve>,
    /// Upstream default axis bounds.
    pub axes: CurveAxes,
}
/// The calibration graph to display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ResultTab {
    /// Sound pressure level relative to the target reference.
    Spl,
    /// Group delay in milliseconds.
    Gd,
    /// Crossover preview.
    Xo,
    /// Bass alignment.
    Ba,
}
/// Selection for one ready-to-draw result graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ResultCurvesParams {
    /// Graph tab.
    pub tab: ResultTab,
    /// Selected main role; absent selects the first available channel.
    pub channel: Option<String>,
    /// Crossover group; absent selects the selected channel's group.
    pub group: Option<String>,
    /// Dragged crossover in Hz; absent uses the configured value.
    pub xo_hz: Option<f64>,
    /// Apply input correction to the XO preview; defaults to true.
    pub corrected: Option<bool>,
}
/// One named series on a result graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ResultCurve {
    /// Series identity: before, after, target, mimo, main, sub, sum_before or sum_after.
    pub kind: String,
    /// Speaker role for per-sub curves, otherwise absent.
    pub role: Option<String>,
    /// Values on the response's shared grid.
    pub values: Vec<f64>,
}
/// A result graph whose calculations are entirely owned by the service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ResultCurvesResult {
    /// Requested tab.
    pub tab: ResultTab,
    /// Shared frequency grid in Hz.
    pub grid: Vec<f64>,
    /// Ordered drawable series.
    pub curves: Vec<ResultCurve>,
    /// Upstream default axis bounds.
    pub axes: CurveAxes,
    /// Whether the retained result no longer matches current inputs.
    pub stale: bool,
    /// Whether the graph comes from a dedicated subs-only result.
    pub subs_only: bool,
    /// RC lower and upper bounds, with an unknown auto upper bound represented as null.
    pub rc_band_hz: Option<(f64, Option<f64>)>,
    /// Selected crossover frequency for the draggable line.
    pub xo_hz: Option<f64>,
}
/// Values behind one channel's metrics and filters lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChannelResultLine {
    /// Main role.
    pub role: String,
    /// Before and after percentage within one dB, 20–200 Hz.
    pub within_1db_pct: [f64; 2],
    /// Before and after group-delay spread, 20–100 Hz, in ms.
    pub gd_spread_ms: [f64; 2],
    /// Number of input PEQs.
    pub peq_count: usize,
    /// FIR taps, absent when no FIR is used.
    pub fir_taps: Option<usize>,
    /// Crossover frequency, absent without bass management.
    pub xo_hz: Option<f64>,
    /// Fresh, enabled MIMO support path count, absent when not shown.
    pub mimo_support_count: Option<usize>,
}
/// One sub's output alignment and fixed filters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubResultRow {
    /// Sub role.
    pub role: String,
    /// Alignment mode code.
    pub mode: String,
    /// Output gain in dB.
    pub gain_db: f64,
    /// Output delay in ms.
    pub delay_ms: f64,
    /// Output polarity, plus or minus one.
    pub polarity: i8,
    /// Ordered all-pass frequency and Q pairs.
    pub allpass: Vec<(f64, f64)>,
    /// High-pass frequency and filter type.
    pub high_pass: (f64, String),
    /// Low-pass frequency and filter type.
    pub low_pass: (f64, String),
    /// Fixed and generated peaking filters as frequency, Q and gain.
    pub peq: Vec<(f64, f64, f64)>,
}
/// A typed BA layout summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubLayoutLine {
    /// Bundle name.
    pub bundle: String,
    /// Layout code, dba or multi.
    pub kind: String,
    /// All members.
    pub members: Vec<String>,
    /// Front row roles.
    pub front: Vec<String>,
    /// Rear row roles.
    pub rear: Vec<String>,
    /// Rear delay in ms.
    pub rear_ms: f64,
    /// Automatic rear delay in ms, when available.
    pub auto_ms: Option<f64>,
    /// Manual rear trim in ms.
    pub trim_ms: f64,
    /// Shared peaking filters.
    pub peq: Vec<(f64, f64, f64)>,
}
/// BA metrics and table, independent of the selected main channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubResultLines {
    /// Whether this BA result is stale.
    pub stale: bool,
    /// Whether a dedicated subs-only run supplies these values.
    pub subs_only: bool,
    /// Metric band in Hz.
    pub band_hz: [f64; 2],
    /// Before and after flatness in dB.
    pub flatness_db: [f64; 2],
    /// Before and after across-seat spread, absent for a single seat.
    pub seat_spread_db: Option<[f64; 2]>,
    /// Number of complete sub positions.
    pub positions: usize,
    /// Output rows in configuration order.
    pub rows: Vec<SubResultRow>,
    /// Bundle layouts.
    pub layouts: Vec<SubLayoutLine>,
    /// Filter and gain warning codes and their arguments.
    pub warnings: Vec<Notice>,
}
/// Numeric calibration summary lines; no localized sentences cross IPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ResultLinesResult {
    /// Whether the full calibration result is stale.
    pub stale: bool,
    /// Per-main metrics and filter counts.
    pub channels: Vec<ChannelResultLine>,
    /// Total delay in milliseconds, absent before calibration.
    pub total_delay_ms: Option<f64>,
    /// Engine notes as codes and arguments.
    pub notes: Vec<Notice>,
    /// RC ceiling provenance and room-volume notices.
    pub rc: Vec<Notice>,
    /// BA metrics, table, layouts and warnings when at least two subs are available.
    pub ba: Option<SubResultLines>,
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
    /// Whether this is a non-writable tutorial session using the sample room.
    #[serde(default)]
    pub tutorial: bool,
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
    /// Whether this open project is a non-writable tutorial session.
    pub tutorial: bool,
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
    /// Designs the subwoofers only (upstream [서브만 계산]); the result is
    /// kept apart and shown by the BA tab until the next full run.
    #[serde(default)]
    pub subs_only: bool,
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
    /// Which input first differs: measurements, settings or target.
    pub stale_reason: Option<String>,
    /// Calibration mode used to compute this result.
    pub mode: Mode,
    /// Ultra-mode latency limit used to compute this result, in milliseconds.
    pub ultra_ms: f64,
    /// Values for the upstream verdict card.
    pub verdict: VerdictSummary,
    /// Per-channel before-and-after metrics.
    pub scorecard: Vec<ChannelScore>,
    /// Total filter latency in milliseconds.
    pub latency_ms: f64,
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
    /// Total FIR-tap budget across all generated paths.
    pub fir_taps_total: usize,
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
    /// Per-channel filter values as text plus FIR WAV files (upstream
    /// `calibration.export_filter_text`). Results with RC only.
    FilterText,
    /// A complete Equalizer APO configuration: `config.txt`, the filter file
    /// and FIR WAV files (upstream `calibration.export_eqapo`). RC and BA.
    EqualizerApo,
}

/// Why a filter export cannot carry the current result (upstream
/// `calibration.filter_export_block`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExportBlock {
    /// The result has subs (BA); filter text carries RC only.
    Subs,
    /// The result includes MIMO; filter text and Equalizer APO do not carry it.
    Mimo,
}

impl ExportBlock {
    /// The `params.reason` value of an [`ErrorCode::ExportScope`] error.
    pub fn name(self) -> &'static str {
        match self {
            Self::Subs => "subs",
            Self::Mimo => "mimo",
        }
    }
}

/// Upstream `filter_export_block`: whether `kind` can carry a result with
/// subs (`has_subs`) and MIMO support paths (`has_mimo`). CamillaDSP and the
/// mdat exports carry everything; filter text carries RC only (subs are
/// checked first, as upstream); Equalizer APO carries RC and BA. The screen
/// mirrors this rule (`screens/export/types.ts`).
pub fn filter_export_block(
    kind: ExportKind,
    has_subs: bool,
    has_mimo: bool,
) -> Option<ExportBlock> {
    match kind {
        ExportKind::FilterText if has_subs => Some(ExportBlock::Subs),
        ExportKind::FilterText | ExportKind::EqualizerApo if has_mimo => Some(ExportBlock::Mimo),
        _ => None,
    }
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
    /// Actual export folder written by the job.
    pub folder: String,
    /// Whether the exported calibration differed from current project inputs.
    pub stale: bool,
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
    /// Optional sweep repetition count; allowed values are 1, 2, 4 and 8.
    pub repetitions: Option<u8>,
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
    /// Active repetition count after timing-reference normalization.
    pub repetitions: u8,
    /// Sweep sample rate in Hz.
    pub sample_rate: u32,
    /// Supported sweep lengths in samples.
    pub sweep_lengths: Vec<usize>,
    /// Timing-reference mode.
    pub timing: String,
    /// Optional one-based loopback input channel.
    pub loopback_input_channel: Option<u16>,
    /// Optional acoustic timing-reference output channel.
    pub timing_ref_output: Option<u16>,
    /// Share preference: auto, exclusive, or shared.
    pub share_mode: String,
}
/// Recording status and upstream source values for one arranged position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PositionRecord {
    /// One-based arranged position number.
    #[ts(type = "number")]
    pub position: i64,
    /// Whether every planned role is recorded at this position.
    pub done: bool,
    /// Planned roles that have a recording at this position.
    pub roles: Vec<String>,
    /// Source file/value by recorded role; native and REW sweeps use `measured`.
    pub sources: BTreeMap<String, String>,
}
/// Recording status for the complete measurement arrangement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MeasurementRecordsResult {
    /// One entry per arranged position.
    pub records: Vec<PositionRecord>,
}
/// Stored level-check readings, keyed by measurement role (upstream `extra['levels']`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LevelsResult {
    /// Measured input level in dBFS by role: the meter reading while that channel played.
    pub levels: BTreeMap<String, f64>,
    /// Measured sound-pressure level in dB SPL by role.
    pub spl: BTreeMap<String, f64>,
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
/// Facts from REW, distinct from project-stored measurement preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct RewInfoResult {
    pub executable_path: Option<String>,
    pub running: bool,
    pub reachable: bool,
    pub version: Option<String>,
    /// User-confirmed license hint; the API has no license endpoint.
    pub pro: Option<bool>,
    pub blocking: Option<bool>,
    /// Empty means no file loaded; null means unknown or per-input calibration.
    pub input_calibration_file: Option<String>,
    pub input_devices: Option<Vec<RewDeviceInfo>>,
    pub output_devices: Option<Vec<RewDeviceInfo>>,
    pub input_channel_names: Option<Vec<String>>,
    pub sample_rate: Option<f64>,
    pub sweep: Option<RewSweepInfo>,
    pub level_dbfs: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RewDeviceInfo {
    pub id: String,
    pub name: String,
    pub channels: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RewSweepInfo {
    pub start_hz: Option<f64>,
    pub end_hz: Option<f64>,
    pub length: Option<String>,
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
    /// Whether the title explicitly names a position, including position 1.
    pub position_tagged: bool,
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

// Project editing request and response types.
/// A filesystem path used by a project editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PathParams {
    /// File system path chosen by the user.
    pub path: String,
}
/// A path written by an editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PathResult {
    /// File system path that was written.
    pub path: String,
}
/// A preset change which may need measurement-clearing confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigPresetParams {
    /// Preset name (`2.0`, `2.1`, `2.2`, `5.1`, `7.1.4`, `9.1.6`, or `custom` for an empty layout).
    pub preset: String,
    /// Clears existing measurements; without it a project with measurements answers `confirm_required` (U5).
    #[serde(default)]
    pub confirm: bool,
}
/// A sub-count change which may need measurement-clearing confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigSubCountParams {
    /// Number of subwoofers, 0 to 8.
    pub count: i64,
    /// Clears existing measurements; without it a project with measurements answers `confirm_required` (U5).
    #[serde(default)]
    pub confirm: bool,
}
/// One speaker index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigIndexParams {
    /// Speaker index in `ConfigResult.speakers`.
    pub index: usize,
}
/// Optional edits to one speaker or one of its bands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigSetSpeakerParams {
    /// Speaker index in `ConfigResult.speakers`.
    pub index: usize,
    /// New role code; a sub role forces a single SUB band (`step2_config.py:988-1002`).
    pub role: Option<String>,
    /// New size: `small` (crossover applied) or `large` (full range).
    pub size: Option<String>,
    /// Band the output and polarity edits apply to; absent means the first band.
    pub band_index: Option<usize>,
    /// New measurement output channel (1-based).
    pub out: Option<i64>,
    /// New polarity: `+` normal or `-` inverted.
    pub pol: Option<String>,
}
/// One configured output band.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigBand {
    /// Band name: `FULL`, `LF`, `MF`, `HF` or `SUB`.
    pub band: String,
    /// Measurement output channel (1-based).
    pub out: i64,
    /// Polarity: `+` normal or `-` inverted.
    pub pol: String,
}
/// One configured speaker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigSpeaker {
    /// Role code (`L`, `C`, `SW1`, `Tfl`, ...).
    pub role: String,
    /// `small` (crossover applied) or `large` (full range); subs are `sub`.
    pub size: String,
    /// Bands in order; one band unless bi-amped.
    pub bands: Vec<ConfigBand>,
}
/// One upstream speaker group in upstream order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigGroup {
    /// Stable presentation group id: front, center, sub, surround, back, wide, top, height or other.
    pub id: String,
    /// Upstream group name used as the project and crossover-map key.
    pub key: String,
    /// Indices into `ConfigResult.speakers` that belong to this group.
    pub speaker_indices: Vec<usize>,
}
/// Values derived from the complete speaker configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigSummary {
    /// Number of speakers.
    pub speaker_count: usize,
    /// Number of measurement outputs in use.
    pub outputs: Vec<i64>,
    /// Highest output channel in use.
    pub max_channel: i64,
    /// Number of output channels used twice or more.
    pub duplicate_outputs: Vec<i64>,
    /// Number of subwoofers.
    pub sub_count: usize,
}
/// Screen-facing editable speaker configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigResult {
    /// Speakers in table order.
    pub speakers: Vec<ConfigSpeaker>,
    /// Preset name, or `custom` after any manual edit.
    pub preset: String,
    /// Measurement output device name (`config.device`).
    pub device: String,
    /// Table group header rows in upstream order.
    pub groups: Vec<ConfigGroup>,
    /// Counts the summary line shows.
    pub summary: ConfigSummary,
}
/// Configuration after a destructive edit and its optional inverse token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigMutationResult {
    /// The configuration after the edit.
    pub config: ConfigResult,
    /// Token for `undo` when the edit cleared measurements (U5).
    pub undo_token: Option<String>,
}
/// The five manually editable DSP profile fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DspFields {
    /// FIR taps per channel.
    pub fir_taps: i64,
    /// Total FIR taps the DSP can run across channels.
    pub fir_budget: i64,
    /// IIR (PEQ) bands per channel.
    pub iir_bands: i64,
    /// All-pass bands per channel.
    pub allpass_bands: i64,
    /// Bass-management path delays the DSP offers.
    pub bm_path_delays: i64,
}
/// One DSP preset in the built-in table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DspPreset {
    /// Preset key (`camilla`, `tesira`, `qsys`, `minidsp`, `custom`).
    pub key: String,
    /// Device name shown in the device list.
    pub name: String,
    /// The preset's filter resources.
    pub fields: DspFields,
}
/// A preset selection or a complete manual edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct DspSetParams {
    /// Selects a preset; absent with `fields` set means a manual edit.
    pub preset: Option<String>,
    /// Manual values; any manual edit makes the profile `custom`.
    pub fields: Option<DspFields>,
}
/// Editable DSP state and derived resolution values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DspResult {
    /// Current profile key.
    pub profile: String,
    /// Current device name.
    pub name: String,
    /// Current filter resources.
    pub fields: DspFields,
    /// Every preset, for the device list.
    pub presets: Vec<DspPreset>,
    /// FIR window length the taps give, in ms (note line).
    pub window_ms: f64,
    /// Frequency resolution the taps give, in Hz (note line).
    pub resolution_hz: f64,
}
/// Measurement timing-reference settings stored under `extra.devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LayoutTiming {
    /// Timing reference: `acoustic`, `loopback` or `none`.
    pub reference: String,
    /// Reference speaker role for the acoustic reference (default `R`).
    pub role: String,
    /// Reference output channel for the loopback reference (1-based).
    pub output: i64,
}
/// One measurement microphone point in centimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LayoutPoint {
    /// Position number; 1 is the listening position.
    pub position: usize,
    /// Left-right offset from the centre seat, in cm (`layout_map.py`).
    pub x_cm: f64,
    /// Front-back offset from the centre seat, in cm (`layout_map.py`).
    pub y_cm: f64,
    /// Height step: 0 at ear height, +1 or -1 for ±20 cm.
    pub height: i8,
}
/// One supported measurement arrangement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LayoutArrangement {
    /// Number of positions: 1, 9, 13 or 17.
    pub count: usize,
    /// Microphone points of this arrangement.
    pub points: Vec<LayoutPoint>,
}
/// Editable arrangement and measurement options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LayoutResult {
    /// Chosen arrangement (`extra['array']`).
    pub count: usize,
    /// Start delay before each position: `off`, `5`, `10` or `15` seconds.
    pub start_delay: String,
    /// Timing reference and its reference speaker or output.
    pub timing: LayoutTiming,
    /// Every arrangement with its geometry, for the cards and maps.
    pub arrangements: Vec<LayoutArrangement>,
}
/// Optional arrangement and timing edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct LayoutSetParams {
    /// New arrangement; changing it clears measurements (U5).
    pub count: Option<usize>,
    /// New start delay.
    pub start_delay: Option<String>,
    /// New timing reference.
    pub timing: Option<LayoutTiming>,
    /// Clears existing measurements; without it a project with measurements answers `confirm_required` (U5).
    #[serde(default)]
    pub confirm: bool,
}
/// Arrangement after editing and its optional inverse token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LayoutMutationResult {
    /// The arrangement and options after the edit.
    pub layout: LayoutResult,
    /// Token for `undo` when the edit cleared measurements (U5).
    pub undo_token: Option<String>,
}
/// Position-duration estimate from upstream step 4 semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PositionEstimateResult {
    /// Number of positions in the arrangement.
    pub positions: usize,
    /// Estimated time per position, in seconds (`step4_layout.position_seconds`).
    pub per_position_seconds: f64,
    /// Estimated time for every position, without moving the microphone, in seconds.
    pub total_seconds: f64,
}
/// Target control points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TargetPointsParams {
    /// Target points as (Hz, dB); clamped to 10–24000 Hz and ±30 dB, at least two.
    pub points: Vec<(f64, f64)>,
}
/// One built-in target preset selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TargetPresetParams {
    /// Target preset: `flat`, `harman` or `cinema`.
    pub preset: String,
}
/// Target curve, selected preset, and built-in preset table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TargetResult {
    /// Target points as (Hz, dB).
    pub points: Vec<(f64, f64)>,
    /// Preset name, or absent when the points were edited (custom).
    pub preset: Option<String>,
    /// Preset names the target menu offers.
    pub presets: Vec<(String, Vec<(f64, f64)>)>,
}
/// Complete calibration settings replacement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CalibrationSettingsParams {
    /// The complete calibration settings to store.
    pub settings: CalibrationSettings,
}
/// Mode whose numeric specification should be returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModeSpecParams {
    /// Correction mode: `zero`, `mixed`, `linear` or `ultra`.
    pub mode: Mode,
    /// Latency limit for the ultra mode, in ms.
    pub ultra_ms: Option<f64>,
}
/// Presentation-neutral values underlying the upstream mode specification line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModeSpecResult {
    /// Room-correction filters: `iir` (IIR only) or `iir_fir` (IIR plus FIR).
    pub rc_filter: String,
    /// FIR taps the mode uses (0 without FIR).
    pub fir_taps: usize,
    /// PEQ bands per channel, when the mode uses IIR.
    pub peq_bands: Option<usize>,
    /// MIMO support filter kind (`iir` in every user mode).
    pub mimo_filter: String,
    /// Delay the FIR may add ahead of the main arrival (`lift_ms`), in ms.
    pub lift_ms: f64,
    /// Octave fraction of the excess-phase smoothing (6, or 24 in linear and ultra modes).
    pub phase_frac: f64,
    /// Upper latency bound of the mode, in ms.
    pub max_latency_ms: f64,
}
/// Application theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AppTheme {
    /// Follow the operating-system preference.
    #[default]
    System,
    /// Always use the light theme.
    Light,
    /// Always use the dark theme.
    Dark,
}
/// Application-local settings, independent of an open project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppSettings {
    /// Screen language: `ko` or `en`.
    pub language: String,
    /// Application theme preference.
    #[serde(default)]
    pub theme: AppTheme,
    /// Shows expert-only settings (U9).
    pub expert_display: bool,
    /// Recently opened project paths, newest first (at most 10).
    pub recent_projects: Vec<String>,
    /// Last window size in logical pixels.
    pub last_window_size: Option<(u32, u32)>,
}
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "ko".into(),
            theme: AppTheme::System,
            expert_display: false,
            recent_projects: Vec::new(),
            last_window_size: None,
        }
    }
}
/// Complete application-local settings replacement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppSettingsParams {
    /// The complete app settings to store.
    pub settings: AppSettings,
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
    /// The chosen export cannot carry the result (upstream
    /// `filter_export_block`); `params.reason` is an [`ExportBlock`] name.
    ExportScope,
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
    /// Tutorial sessions cannot save or export project data.
    TutorialBlocked,
    /// A destructive edit needs explicit confirmation while measurements exist.
    ConfirmRequired,
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
                tutorial: false,
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
            Request::MeasureConfigGet,
            Request::MeasurementRecords,
            Request::LevelsGet,
            Request::LevelsFillDefaults,
            Request::LevelStop,
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
            Request::ConfigGet,
            Request::ConfigApplyPreset(ConfigPresetParams {
                preset: "2.1".into(),
                confirm: false,
            }),
            Request::ConfigSetSubCount(ConfigSubCountParams {
                count: 2,
                confirm: false,
            }),
            Request::ConfigSetSpeaker(ConfigSetSpeakerParams {
                index: 0,
                role: Some("L".into()),
                size: None,
                band_index: Some(0),
                out: Some(1),
                pol: Some("+".into()),
            }),
            Request::ConfigAddSpeaker,
            Request::ConfigRemoveSpeaker(ConfigIndexParams { index: 0 }),
            Request::ConfigRenumber,
            Request::ConfigValidate,
            Request::PatchSheetCsv(PathParams {
                path: "patch.csv".into(),
            }),
            Request::DspGet,
            Request::DspSet(DspSetParams {
                preset: Some("tesira".into()),
                fields: None,
            }),
            Request::LayoutGet,
            Request::LayoutSet(LayoutSetParams {
                count: Some(9),
                ..Default::default()
            }),
            Request::PositionEstimate,
            Request::TargetGet,
            Request::TargetSetPoints(TargetPointsParams {
                points: vec![(20.0, 0.0), (20_000.0, 0.0)],
            }),
            Request::TargetApplyPreset(TargetPresetParams {
                preset: "flat".into(),
            }),
            Request::TargetSave(PathParams {
                path: "target.json".into(),
            }),
            Request::TargetLoad(PathParams {
                path: "target.json".into(),
            }),
            Request::CalibrationSettingsGet,
            Request::CalibrationSettingsEdit(CalibrationSettingsEditParams {
                edit: crate::settings_edit::SettingEdit::MimoOn { on: true },
            }),
            Request::SubWarnings,
            Request::RewInfo,
            Request::CalibrationSettingsSet(CalibrationSettingsParams {
                settings: CalibrationSettings::default(),
            }),
            Request::ModeSpec(ModeSpecParams {
                mode: Mode::Mixed,
                ultra_ms: None,
            }),
            Request::AppSettingsGet,
            Request::AppSettingsSet(AppSettingsParams {
                settings: AppSettings::default(),
            }),
            Request::MeasurementCurves(MeasurementCurvesParams::default()),
            Request::ResultCurves(ResultCurvesParams {
                tab: ResultTab::Spl,
                channel: None,
                group: None,
                xo_hz: None,
                corrected: None,
            }),
            Request::ResultLines,
            Request::SubTakeValues,
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

    #[test]
    fn filter_export_scope_follows_upstream() {
        use ExportKind::*;
        for kind in [CamillaDsp, StagedMdat, BeforeAfterMdat] {
            for (subs, mimo) in [(false, false), (true, false), (false, true), (true, true)] {
                assert_eq!(filter_export_block(kind, subs, mimo), None, "{kind:?}");
            }
        }
        assert_eq!(filter_export_block(FilterText, false, false), None);
        assert_eq!(
            filter_export_block(FilterText, true, false),
            Some(ExportBlock::Subs)
        );
        assert_eq!(
            filter_export_block(FilterText, true, true),
            Some(ExportBlock::Subs)
        );
        assert_eq!(
            filter_export_block(FilterText, false, true),
            Some(ExportBlock::Mimo)
        );
        assert_eq!(filter_export_block(EqualizerApo, true, false), None);
        assert_eq!(
            filter_export_block(EqualizerApo, false, true),
            Some(ExportBlock::Mimo)
        );
        assert_eq!(
            filter_export_block(EqualizerApo, true, true),
            Some(ExportBlock::Mimo)
        );
        assert_eq!(serde_json::to_value(FilterText).unwrap(), "filter_text");
        assert_eq!(serde_json::to_value(EqualizerApo).unwrap(), "equalizer_apo");
        assert_eq!(
            serde_json::to_value(ExportBlock::Subs).unwrap(),
            ExportBlock::Subs.name()
        );
        assert_eq!(
            serde_json::to_value(ExportBlock::Mimo).unwrap(),
            ExportBlock::Mimo.name()
        );
        assert_eq!(
            serde_json::to_value(ErrorCode::ExportScope).unwrap(),
            "export_scope"
        );
    }
}
