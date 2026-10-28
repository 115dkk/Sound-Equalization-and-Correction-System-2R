#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! `secs2r-cli`: the SECS 2R service without a screen.
//!
//! Commands call the application service and write JSON Lines to standard output.

use clap::{Parser, Subcommand, ValueEnum};
use secs_service::{Deps, EventSink, LocalTime, Service};
use secs_types::ipc::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::SystemTime;

#[derive(Debug, Parser)]
#[command(name = "secs2r-cli", version, about = "SECS 2R without a screen")]
struct Cli {
    /// Omit job-event lines while retaining every reply.
    #[arg(long, global = true)]
    quiet: bool,
    /// Use a fixed clock and UUID counter beginning at one for reproducible output.
    #[arg(long, global = true)]
    deterministic_uuids: bool,
    /// Choose MIMO computation for this process without changing desktop preferences.
    #[arg(long, global = true, value_enum)]
    compute: Option<ComputeChoice>,
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Subcommand)]
enum Action {
    /// Print compute preference, selected device, and availability.
    ComputeInfo,
    /// Print the version, upstream base, and platform.
    Bootstrap,
    /// Check platform and deterministic parallel-map operation.
    Check,
    /// Create or inspect a project.
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Load embedded or deterministic synthetic demo measurements.
    Sample {
        project: PathBuf,
        #[arg(long)]
        positions: Option<u32>,
    },
    /// Measure one project position.
    Measure {
        project: PathBuf,
        #[arg(long)]
        position: i64,
        #[arg(long)]
        method: Option<MeasureMethod>,
    },
    /// Import suggested assignments from a REW mdat or a Dirac Live project.
    ImportMdat {
        project: PathBuf,
        file: PathBuf,
        #[arg(long, default_value_t = 1)]
        position: i64,
    },
    /// Compute a calibration.
    Calibrate { project: PathBuf },
    /// Recommend crossover frequencies.
    RecommendXo { project: PathBuf },
    /// Calibrate first, then compute MIMO support paths.
    Mimo { project: PathBuf },
    /// Write one export bundle.
    Export {
        project: PathBuf,
        #[arg(long)]
        kind: ExportChoice,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, value_enum)]
        mdat_scope: Option<MdatScopeChoice>,
    },
    /// Run creation, filling, calibration, and all exports.
    Run {
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long, default_value = "sample")]
        fill: FillChoice,
        #[arg(long)]
        mimo: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ProjectAction {
    /// Create a project file.
    New {
        path: PathBuf,
        #[arg(long, default_value = "2.0")]
        preset: String,
        #[arg(long, default_value_t = 0)]
        subs: u8,
        #[arg(long)]
        sample: bool,
    },
    /// Open a project and print its information.
    Info { path: PathBuf },
    /// Set the calibration settings an experiment changes, then save the project.
    Set {
        path: PathBuf,
        /// Compute MIMO support paths after calibrating.
        #[arg(long, value_enum)]
        mimo: Option<Switch>,
        /// MIMO supports as FIR or as IIR (PEQ and all-pass) filters.
        #[arg(long, value_enum)]
        mimo_supports: Option<SupportKind>,
        /// PEQ derivative method (experimental analytic results differ from SECS 2).
        #[arg(long, value_enum)]
        peq_derivatives: Option<PeqDerivativeChoice>,
        /// MIMO IIR derivative method (experimental analytic results differ from SECS 2).
        #[arg(long, value_enum)]
        mimo_iir_derivatives: Option<MimoIirDerivativeChoice>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Switch {
    On,
    Off,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SupportKind {
    Fir,
    Iir,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PeqDerivativeChoice {
    TwoPoint,
    Analytic,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MimoIirDerivativeChoice {
    Central,
    Analytic,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ComputeChoice {
    Auto,
    Cpu,
    Gpu,
}
impl From<ComputeChoice> for secs_types::compute::ComputePreference {
    fn from(value: ComputeChoice) -> Self {
        match value {
            ComputeChoice::Auto => Self::Auto,
            ComputeChoice::Cpu => Self::Cpu,
            ComputeChoice::Gpu => Self::Gpu,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MeasureMethod {
    Mock,
    Native,
    Rew,
}
impl MeasureMethod {
    fn name(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Native => "native",
            Self::Rew => "rew",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum FillChoice {
    Sample,
    Mock,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ExportChoice {
    CamillaDsp,
    StagedMdat,
    BeforeAfterMdat,
    FilterText,
    EqualizerApo,
}
impl From<ExportChoice> for ExportKind {
    fn from(value: ExportChoice) -> Self {
        match value {
            ExportChoice::CamillaDsp => Self::CamillaDsp,
            ExportChoice::StagedMdat => Self::StagedMdat,
            ExportChoice::BeforeAfterMdat => Self::BeforeAfterMdat,
            ExportChoice::FilterText => Self::FilterText,
            ExportChoice::EqualizerApo => Self::EqualizerApo,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MdatScopeChoice {
    All,
    Measurements,
    Results,
}
impl From<MdatScopeChoice> for MdatScope {
    fn from(value: MdatScopeChoice) -> Self {
        match value {
            MdatScopeChoice::All => Self::All,
            MdatScopeChoice::Measurements => Self::Measurements,
            MdatScopeChoice::Results => Self::Results,
        }
    }
}

#[derive(Debug)]
struct Events {
    quiet: bool,
    finished: Mutex<Option<JobEvent>>,
    changed: Condvar,
}
impl Events {
    fn new(quiet: bool) -> Self {
        Self {
            quiet,
            finished: Mutex::new(None),
            changed: Condvar::new(),
        }
    }

    fn wait(&self) -> JobEvent {
        let mut finished = lock(&self.finished);
        loop {
            if let Some(event) = finished.take() {
                return event;
            }
            finished = self
                .changed
                .wait(finished)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
}
impl EventSink for Events {
    fn send(&self, event: JobEvent) {
        if !self.quiet {
            print_json(&serde_json::json!({"event": event}));
        }
        if matches!(event, JobEvent::Finished { .. }) {
            *lock(&self.finished) = Some(event);
            self.changed.notify_all();
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

struct Runner {
    service: Service,
    quiet: bool,
    ok: bool,
}
impl Runner {
    fn call(&mut self, request: Request) -> Option<Response> {
        if !self.ok {
            return None;
        }
        let events = Arc::new(Events::new(self.quiet));
        let envelope = self.service.call(request, events.clone());
        match envelope {
            Envelope::Err(error) => {
                self.ok = false;
                print_json(&serde_json::json!({"reply": Envelope::Err(error)}));
                None
            }
            Envelope::Ok(response) => {
                if response_job(&response).is_some() {
                    if self.finish_job(events.wait()) {
                        Some(*response)
                    } else {
                        None
                    }
                } else {
                    print_json(&serde_json::json!({"reply": Envelope::Ok(response.clone())}));
                    Some(*response)
                }
            }
        }
    }

    fn finish_job(&mut self, event: JobEvent) -> bool {
        let JobEvent::Finished {
            status,
            result,
            error,
            ..
        } = event
        else {
            unreachable!("event sink waits for a finished event")
        };
        if status == JobStatus::Succeeded {
            print_json(&serde_json::json!({"reply": {
                "ok": true,
                "data": result,
            }}));
            true
        } else {
            self.ok = false;
            print_json(&serde_json::json!({"reply": {
                "ok": false,
                "error": error.unwrap_or_else(|| IpcError::new(
                    ErrorCode::Internal,
                    format!("job ended with {status:?}"),
                )),
            }}));
            false
        }
    }

    fn open(&mut self, path: &Path) {
        self.call(Request::ProjectOpen(ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }));
    }

    fn save(&mut self) {
        self.call(Request::ProjectSave(ProjectSaveParams::default()));
    }

    fn configure_mock(&mut self) {
        self.call(Request::MeasureConfigure(MeasureConfigureParams {
            method: "mock".into(),
            ..Default::default()
        }));
    }

    fn calibrate(&mut self, settings: Option<secs_types::calibration::CalibrationSettings>) {
        self.call(Request::Calibrate(CalibrateParams {
            settings,
            ..Default::default()
        }));
    }

    fn export(&mut self, kind: ExportKind, folder: &Path, mdat_scope: Option<MdatScope>) {
        self.call(Request::Export(ExportParams {
            kind,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope,
        }));
    }
}

fn response_job(response: &Response) -> Option<JobStarted> {
    match response {
        Response::SampleLoad(value)
        | Response::Calibrate(value)
        | Response::RecommendXo(value)
        | Response::Mimo(value)
        | Response::Export(value)
        | Response::MeasurePosition(value)
        | Response::MdatImport(value) => Some(value.clone()),
        _ => None,
    }
}

fn deps(deterministic: bool, compute: Option<ComputeChoice>) -> Deps {
    let mut deps = Deps {
        app_version: env!("CARGO_PKG_VERSION").into(),
        acceleration: Arc::new(secs_gpu::select),
        compute_override: compute.map(Into::into),
        audio_backend: Arc::new(secs_sys::audio::default_backend),
        rew_transport: Arc::new(|| Box::new(secs_rew::HttpTransport::default())),
        process: Arc::new(secs_sys::process::OsProcessProbe),
        memory: Arc::new(secs_sys::memory::OsMemoryProbe),
        clock: Arc::new(|| {
            let now = SystemTime::now();
            LocalTime::from_system(now, secs_sys::clock::utc_offset_seconds(now))
        }),
        ..Deps::default()
    };
    if deterministic {
        let mut next = 1_u128;
        deps.uuid = Arc::new(Mutex::new(Box::new(move || {
            let value = next;
            next = next.wrapping_add(1);
            Ok(value)
        })));
        deps.clock = Arc::new(|| LocalTime {
            year: 2024,
            month: 1,
            day: 2,
            hour: 3,
            minute: 4,
            second: 5,
        });
        // 2024-01-02 03:04:05 UTC, the same instant as the fixed clock above.
        deps.unix_ms = Arc::new(|| 1_704_164_645_000);
    }
    deps
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut runner = Runner {
        service: Service::new(deps(cli.deterministic_uuids, cli.compute)),
        quiet: cli.quiet,
        ok: true,
    };
    run(cli.action, &mut runner);
    if runner.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(action: Action, runner: &mut Runner) {
    match action {
        Action::ComputeInfo => {
            runner.call(Request::ComputeStatus);
        }
        Action::Bootstrap => {
            runner.call(Request::Bootstrap);
        }
        Action::Check => check(runner),
        Action::Project { action } => match action {
            ProjectAction::New {
                path,
                preset,
                subs,
                sample,
            } => {
                runner.call(Request::ProjectNew(ProjectNewParams {
                    path: path.to_string_lossy().into_owned(),
                    preset,
                    sub_count: subs,
                    sample,
                    tutorial: false,
                }));
            }
            ProjectAction::Info { path } => {
                runner.open(&path);
                runner.call(Request::ProjectInfo);
            }
            ProjectAction::Set {
                path,
                mimo,
                mimo_supports,
                peq_derivatives,
                mimo_iir_derivatives,
            } => {
                use secs_types::settings_edit::SettingEdit;
                runner.open(&path);
                let mut edits = Vec::new();
                if let Some(mimo) = mimo {
                    edits.push(SettingEdit::MimoOn {
                        on: matches!(mimo, Switch::On),
                    });
                }
                if let Some(kind) = mimo_supports {
                    edits.push(SettingEdit::MimoFir {
                        on: matches!(kind, SupportKind::Fir),
                    });
                }
                if peq_derivatives.is_some() || mimo_iir_derivatives.is_some() {
                    edits.push(SettingEdit::AnalyticDerivatives {
                        peq: peq_derivatives
                            .map(|choice| matches!(choice, PeqDerivativeChoice::Analytic)),
                        mimo_iir: mimo_iir_derivatives
                            .map(|choice| matches!(choice, MimoIirDerivativeChoice::Analytic)),
                    });
                }
                let changed = !edits.is_empty();
                for edit in edits {
                    runner.call(Request::CalibrationSettingsEdit(
                        CalibrationSettingsEditParams { edit },
                    ));
                }
                if changed {
                    runner.save();
                }
                runner.call(Request::CalibrationSettingsGet);
            }
        },
        Action::Sample { project, positions } => {
            runner.open(&project);
            runner.call(Request::SampleLoad(SampleLoadParams { positions }));
            runner.save();
        }
        Action::Measure {
            project,
            position,
            method,
        } => {
            runner.open(&project);
            if let Some(method) = method {
                runner.call(Request::MeasureConfigure(MeasureConfigureParams {
                    method: method.name().into(),
                    ..Default::default()
                }));
            }
            runner.call(Request::MeasurePosition(MeasurePositionParams { position }));
            runner.save();
        }
        Action::ImportMdat {
            project,
            file,
            position,
        } => {
            runner.open(&project);
            let path = file.to_string_lossy().into_owned();
            let scan = runner.call(Request::MdatScan(MdatScanParams { path: path.clone() }));
            let assignments = match scan {
                Some(Response::MdatScan(result)) => untagged_rows_use_position(result),
                _ => Vec::new(),
            };
            runner.call(Request::MdatImport(MdatImportParams {
                path,
                assignments,
                position,
            }));
            runner.save();
        }
        Action::Calibrate { project } => {
            runner.open(&project);
            runner.calibrate(None);
            runner.call(Request::Results);
        }
        Action::RecommendXo { project } => {
            runner.open(&project);
            runner.call(Request::RecommendXo);
        }
        Action::Mimo { project } => {
            runner.open(&project);
            runner.calibrate(None);
            runner.call(Request::Mimo(MimoParams::default()));
            runner.call(Request::Results);
        }
        Action::Export {
            project,
            kind,
            out,
            mdat_scope,
        } => {
            runner.open(&project);
            runner.export(kind.into(), &out, mdat_scope.map(Into::into));
        }
        Action::Run {
            out,
            project,
            fill,
            mimo,
        } => full_run(runner, &out, project.as_deref(), fill, mimo),
    }
}

/// Rows whose title names no position keep `--position` (TD-36): the scan's
/// suggested position for them is only the current position guess.
fn untagged_rows_use_position(scan: MdatScanResult) -> Vec<MdatAssignment> {
    scan.suggestions
        .into_iter()
        .map(|mut assignment| {
            let untagged = assignment.index.is_some_and(|index| {
                scan.measurements
                    .iter()
                    .any(|row| row.index == index && !row.position_tagged)
            });
            if untagged {
                assignment.position = None;
            }
            assignment
        })
        .collect()
}

fn check(runner: &mut Runner) {
    let response = runner
        .service
        .call(Request::Bootstrap, Arc::new(Events::new(true)));
    let Envelope::Ok(response) = response else {
        runner.ok = false;
        print_json(&serde_json::json!({"reply": response}));
        return;
    };
    let Response::Bootstrap(info) = *response else {
        unreachable!("bootstrap has one response variant")
    };
    let available = std::thread::available_parallelism().map_or(1, usize::from);
    let serial =
        secs_compute::Context::serial().map((0_i64..32).collect(), |value| value * value + 3);
    let parallel_context = secs_compute::Context::new(
        available,
        secs_compute::MemoryBudget::new(u64::MAX),
        Arc::new(secs_compute::NoProgress),
        secs_compute::CancelToken::new(),
    );
    let parallel = parallel_context.map((0_i64..32).collect(), |value| value * value + 3);
    let parallel_status = match (serial, parallel) {
        (Ok(serial), Ok(parallel)) if serial == parallel => "ok".to_owned(),
        (Ok(_), Ok(_)) => {
            runner.ok = false;
            "wrong".to_owned()
        }
        (left, right) => {
            runner.ok = false;
            format!("error: {left:?} / {right:?}")
        }
    };
    print_json(&serde_json::json!({"reply": {"ok": runner.ok, "data": {
        "version": info.version,
        "upstream": info.upstream,
        "platform": info.platform,
        "available_parallelism": available,
        "rayon_threads": rayon::current_num_threads(),
        "parallel": parallel_status,
        "status": if runner.ok { "ok" } else { "error" }
    }}}));
}

fn full_run(runner: &mut Runner, out: &Path, project: Option<&Path>, fill: FillChoice, mimo: bool) {
    if let Some(project) = project {
        runner.open(project);
    } else {
        let path = out.join("project/sample.secs");
        runner.call(Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "7.1.4".into(),
            sub_count: 3,
            sample: true,
            tutorial: false,
        }));
        match fill {
            FillChoice::Sample => {
                runner.call(Request::SampleLoad(SampleLoadParams::default()));
            }
            FillChoice::Mock => {
                runner.configure_mock();
                runner.call(Request::MeasurePosition(MeasurePositionParams {
                    position: 1,
                }));
            }
        }
        runner.save();
    }
    // The project's own calibration settings apply; the CLI changes none.
    runner.calibrate(None);
    if mimo {
        runner.call(Request::Mimo(MimoParams { settings: None }));
    }
    let results = match runner.call(Request::Results) {
        Some(Response::Results(results)) => Some(results),
        _ => None,
    };
    let has_subs = results
        .as_ref()
        .and_then(|results| results.calibration.as_ref())
        .is_some_and(|calibration| !calibration.sub_roles.is_empty());
    let has_mimo = results.as_ref().is_some_and(|results| {
        results.mimo_export == secs_types::ipc::MimoExportDecision::Included
    });
    for (kind, folder) in [
        (ExportKind::CamillaDsp, "camilla-dsp"),
        (ExportKind::StagedMdat, "staged-mdat"),
        (ExportKind::BeforeAfterMdat, "before-after-mdat"),
        (ExportKind::FilterText, "filter-text"),
        (ExportKind::EqualizerApo, "equalizer-apo"),
    ] {
        let stale_cache = results
            .as_ref()
            .and_then(|results| results.calibration.as_ref())
            .is_some_and(|calibration| calibration.cached && calibration.stale);
        if let Some(block) = secs_types::ipc::export_block(kind, has_subs, has_mimo, stale_cache) {
            print_json(&serde_json::json!({"reply": {
                "ok": true,
                "data": {
                    "skipped": export_kind_name(kind),
                    "reason": block.name(),
                },
            }}));
            continue;
        }
        runner.export(kind, &out.join("export").join(folder), None);
    }
}

fn export_kind_name(kind: ExportKind) -> &'static str {
    match kind {
        ExportKind::CamillaDsp => "camilla-dsp",
        ExportKind::StagedMdat => "staged-mdat",
        ExportKind::BeforeAfterMdat => "before-after-mdat",
        ExportKind::FilterText => "filter-text",
        ExportKind::EqualizerApo => "equalizer-apo",
    }
}

fn print_json(value: &impl Serialize) {
    match serde_json::to_string(value) {
        Ok(line) => println!("{line}"),
        Err(error) => eprintln!("secs2r-cli: could not serialize output: {error}"),
    }
}
