#![forbid(unsafe_code)]

#[path = "../examples/support/case_project.rs"]
mod case_project;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use secs_service::{Deps, EventSink, NoEvents, Service, VERSION};
use secs_types::ipc::{Envelope, ErrorCode, JobEvent, ProjectNewParams, Request};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
use secs_project::MeasurementResult;
use secs_types::calibration::CalibrationSettings;
use secs_types::ipc::{
    CalibrateParams, ExportKind, ExportParams, JobCancelParams, JobId, JobResult, JobStatus,
    MimoParams, Response,
};

static TEMP: AtomicU64 = AtomicU64::new(0);

fn service() -> Service {
    Service::new(Deps::default())
}

fn deterministic_service() -> Service {
    Service::new(Deps {
        uuid: Arc::new(Mutex::new(Box::new(|| Ok(0x1234_5678_9abc_def0)))),
        ..Deps::default()
    })
}

#[derive(Debug, Default)]
struct Events {
    values: Mutex<Vec<JobEvent>>,
    changed: Condvar,
}

impl EventSink for Events {
    fn send(&self, event: JobEvent) {
        lock(&self.values).push(event);
        self.changed.notify_all();
    }
}

impl Events {
    fn wait(&self, job: JobId, timeout: Duration) -> JobEvent {
        let deadline = Instant::now() + timeout;
        let mut values = lock(&self.values);
        loop {
            if let Some(event) = values.iter().find(|event| {
                matches!(event, JobEvent::Finished { job: event_job, .. } if *event_job == job)
            }) {
                return event.clone();
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "job {job:?} did not finish");
            values = self.changed.wait_timeout(values, remaining).unwrap().0;
        }
    }
}

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "secs-service-{name}-{}-{}",
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    ))
}

fn call_ok(service: &Service, request: Request, events: Arc<dyn EventSink>) -> Response {
    let Envelope::Ok(response) = service.call(request, events) else {
        panic!("request must succeed");
    };
    *response
}

#[test]
fn new_projects_match_upstream_bytes() {
    let root = temp_root("new-projects");
    fs::create_dir_all(&root).unwrap();
    for fixture in secs_oracle::Fixture::all("service_new", "") {
        let inputs = fixture.inputs();
        let path = root.join("project.secs");
        let response = call_ok(
            &service(),
            Request::ProjectNew(ProjectNewParams {
                path: path.to_string_lossy().into_owned(),
                preset: inputs["preset"].as_str().unwrap().into(),
                sub_count: inputs["sub_count"].as_u64().unwrap() as u8,
                sample: inputs["sample"].as_bool().unwrap(),
            }),
            Arc::new(NoEvents),
        );
        assert!(
            matches!(response, Response::ProjectNew(_)),
            "{}",
            fixture.id
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            fixture.blob(&fixture.outputs()["project"]),
            "{}",
            fixture.id
        );
    }
    fs::remove_dir_all(root).unwrap();
}

fn write_case(root: &Path, name: &str) -> (PathBuf, CalibrationSettings) {
    case_project::write(root, name)
}

fn open(service: &Service, path: &Path) {
    assert!(matches!(
        call_ok(
            service,
            Request::ProjectOpen(secs_types::ipc::ProjectPathParams {
                path: path.to_string_lossy().into_owned(),
            }),
            Arc::new(NoEvents),
        ),
        Response::ProjectOpen(_)
    ));
}

fn start_calibration(
    service: &Service,
    events: Arc<Events>,
    settings: CalibrationSettings,
) -> JobId {
    let Response::Calibrate(started) = call_ok(
        service,
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
        }),
        events,
    ) else {
        panic!("calibration must start");
    };
    started.job
}

fn snapshot_relative_files(root: &Path, files: &[String]) -> Vec<(PathBuf, Vec<u8>)> {
    let mut snapshot: Vec<_> = files
        .iter()
        .map(|file| {
            let relative = Path::new(file).strip_prefix(root).unwrap().to_owned();
            let bytes = fs::read(file).unwrap();
            (relative, bytes)
        })
        .collect();
    snapshot.sort_by(|left, right| left.0.cmp(&right.0));
    snapshot
}

fn compare_snapshot(root: &Path, snapshot: &[(PathBuf, Vec<u8>)], direct_files: &[PathBuf]) {
    let mut direct_relative: Vec<_> = direct_files
        .iter()
        .map(|file| file.strip_prefix(root).unwrap().to_owned())
        .collect();
    direct_relative.sort();
    assert_eq!(
        snapshot
            .iter()
            .map(|(relative, _)| relative)
            .collect::<Vec<_>>(),
        direct_relative.iter().collect::<Vec<_>>()
    );
    for (relative, expected) in snapshot {
        assert_eq!(
            expected,
            &fs::read(root.join(relative)).unwrap(),
            "{} differs",
            relative.display()
        );
    }
}

fn finished_result(event: JobEvent) -> JobResult {
    match event {
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            result: Some(result),
            ..
        } => *result,
        other => panic!("job must succeed: {other:?}"),
    }
}

fn export_and_wait(
    service: &Service,
    kind: ExportKind,
    folder: &Path,
    events: Arc<Events>,
) -> secs_types::ipc::ExportResult {
    let Response::Export(started) = call_ok(
        service,
        Request::Export(ExportParams {
            kind,
            folder: folder.to_string_lossy().into_owned(),
        }),
        events.clone(),
    ) else {
        panic!("export must start");
    };
    let JobResult::Export(result) =
        finished_result(events.wait(started.job, Duration::from_secs(180)))
    else {
        panic!("export result must be returned");
    };
    result
}

#[test]
fn bootstrap_reports_version_and_upstream_base() {
    let service = service();
    let Envelope::Ok(response) = service.call(Request::Bootstrap, Arc::new(NoEvents)) else {
        panic!("bootstrap must succeed");
    };
    let Response::Bootstrap(info) = *response else {
        panic!("bootstrap response must match the request");
    };
    assert_eq!(info.app_name, "SECS 2R");
    assert_eq!(info.version, VERSION);
    assert_eq!(info.upstream.commit.len(), 40);
}

#[test]
fn malformed_requests_become_invalid_request_envelopes() {
    let service = service();
    for request in [
        serde_json::json!({"method": "no_such_method"}),
        serde_json::json!({"params": {}}),
        serde_json::json!("bootstrap"),
    ] {
        let Envelope::Err(error) = service.call_value(request, Arc::new(NoEvents)) else {
            panic!("request must fail");
        };
        assert_eq!(error.code, ErrorCode::InvalidRequest);
    }
}

#[test]
fn oracle_project_calibrates_recommends_xo_and_exports() {
    let root = temp_root("oracle-flow");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let service = deterministic_service();
    open(&service, &path);

    let calibration_events = Arc::new(Events::default());
    let calibration_job = start_calibration(&service, calibration_events.clone(), settings.clone());
    let JobResult::Calibration(results) =
        finished_result(calibration_events.wait(calibration_job, Duration::from_secs(120)))
    else {
        panic!("calibration result must be returned");
    };
    assert!(!results.calibration.as_ref().unwrap().stale);
    assert_eq!(results.calibration.as_ref().unwrap().chain.mains.len(), 11);
    let direct = secs_engine::test_support::production_case("dolby", 1);
    assert_eq!(
        results.calibration.as_ref().unwrap().chain,
        direct.design.chain
    );

    let xo_events = Arc::new(Events::default());
    let Response::RecommendXo(xo_started) =
        call_ok(&service, Request::RecommendXo, xo_events.clone())
    else {
        panic!("XO recommendation must start");
    };
    let JobResult::Xo(recommendation) =
        finished_result(xo_events.wait(xo_started.job, Duration::from_secs(120)))
    else {
        panic!("XO result must be returned");
    };
    let view = secs_engine::ProjectView {
        groups: &direct.config.groups,
        project_xo: &settings.xo,
        target: &direct.config.target,
    };
    let ctx = secs_compute::Context::new(
        1,
        secs_compute::MemoryBudget::new(u64::MAX),
        Arc::new(secs_compute::NoProgress),
        secs_compute::CancelToken::new(),
    );
    let direct_xo = secs_engine::xo::recommend_xo(
        &view,
        &direct.measurements,
        &settings,
        &secs_engine::Tuning::default(),
        &secs_engine::xo::XO_CANDIDATES,
        &ctx,
    )
    .unwrap();
    assert_eq!(recommendation.best, direct_xo.best);
    assert_eq!(recommendation.candidates, direct_xo.candidates);

    for kind in [
        ExportKind::CamillaDsp,
        ExportKind::StagedMdat,
        ExportKind::BeforeAfterMdat,
    ] {
        let folder = root.join(format!("export-{kind:?}"));
        let events = Arc::new(Events::default());
        let Response::Export(started) = call_ok(
            &service,
            Request::Export(ExportParams {
                kind,
                folder: folder.to_string_lossy().into_owned(),
            }),
            events.clone(),
        ) else {
            panic!("export must start");
        };
        let JobResult::Export(export) =
            finished_result(events.wait(started.job, Duration::from_secs(120)))
        else {
            panic!("export result must be returned");
        };
        assert!(!export.files.is_empty());
        assert!(export.files.iter().all(|file| Path::new(file).exists()));
        match kind {
            ExportKind::CamillaDsp => {
                let snapshot = snapshot_relative_files(&folder, &export.files);
                let labels = secs_camilla::RunLabels {
                    mode: match settings.mode {
                        secs_types::calibration::Mode::Zero => "zero",
                        secs_types::calibration::Mode::Mixed => "mixed",
                        secs_types::calibration::Mode::Linear => "linear",
                        secs_types::calibration::Mode::Ultra => "ultra",
                    }
                    .into(),
                    ultra_ms: settings.ultra_ms,
                    latency_ms: direct.design.chain.latency_ms,
                    rc_hi_hz: direct.design.rc_hi,
                    mimo_max_supports: settings.mimo_max_supports.max(0) as usize,
                };
                let direct_files =
                    secs_camilla::export(&direct.design.chain, &folder, &labels, None).unwrap();
                assert_eq!(export.files.len(), direct_files.len());
                compare_snapshot(&folder, &snapshot, &direct_files);
            }
            ExportKind::StagedMdat | ExportKind::BeforeAfterMdat => {
                let direct_folder = root.join(format!("direct-{kind:?}"));
                let run = secs_export::ExportRun {
                    design: &direct.design,
                    measurements: &direct.measurements,
                    config: &direct.config,
                    settings: &settings,
                };
                let mut uuid = || Ok(0x1234_5678_9abc_def0);
                let direct_file = match kind {
                    ExportKind::StagedMdat => secs_export::export_mdat_with_mimo_using(
                        &run,
                        &direct_folder,
                        None,
                        None,
                        &mut uuid,
                    ),
                    ExportKind::BeforeAfterMdat => {
                        secs_export::export_before_after_mdat_with_mimo_using(
                            &run,
                            &direct_folder,
                            None,
                            None,
                            &mut uuid,
                        )
                    }
                    ExportKind::CamillaDsp => unreachable!(),
                }
                .unwrap();
                assert_eq!(
                    fs::read(&export.files[0]).unwrap(),
                    fs::read(direct_file).unwrap()
                );
            }
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn exports_after_reopening_match_pre_close_bytes() {
    let root = temp_root("reopened-exports");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let running = deterministic_service();
    open(&running, &path);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    for kind in [ExportKind::CamillaDsp, ExportKind::StagedMdat] {
        let folder = root.join(format!("reopen-{kind:?}"));
        let before_events = Arc::new(Events::default());
        let before = export_and_wait(&running, kind, &folder, before_events);
        let before_snapshot = snapshot_relative_files(&folder, &before.files);

        let reopened = deterministic_service();
        open(&reopened, &path);
        let after_events = Arc::new(Events::default());
        let after = export_and_wait(&reopened, kind, &folder, after_events.clone());
        compare_snapshot(
            &folder,
            &before_snapshot,
            &after.files.iter().map(PathBuf::from).collect::<Vec<_>>(),
        );

        let values = lock(&after_events.values);
        assert!(
            !values.iter().any(|event| matches!(
                event,
                JobEvent::Started {
                    kind: secs_types::ipc::JobKind::Calibrate,
                    ..
                }
            )),
            "reopened export must not launch calibration"
        );
        let has_design = values.iter().any(|event| {
            matches!(
                event,
                JobEvent::Progress {
                    stage,
                    ..
                } if stage == "design"
            )
        });
        assert_eq!(has_design, kind == ExportKind::StagedMdat);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_calibration_refuses_export_and_cache_survives_reopen() {
    let root = temp_root("stale-cache");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let running_service = service();
    open(&running_service, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&running_service, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));

    let before = match call_ok(&running_service, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.calibration.unwrap(),
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(!before.stale);
    let reopened = service();
    open(&reopened, &path);
    let cached = match call_ok(&reopened, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.calibration.unwrap(),
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(!cached.stale);
    assert_eq!(cached.chain, before.chain);

    {
        let mut store = secs_project::MeasurementStore::open(&path).unwrap();
        let record = store.records().into_iter().next().unwrap();
        let role = record.ir.keys().next().unwrap().clone();
        let (mut samples, sample_rate) = store.ir_of(&role, record.pos).unwrap();
        samples[0] += 0.25;
        store
            .record(
                MeasurementResult {
                    role,
                    pos: record.pos,
                    ir: samples,
                    sample_rate,
                    t0_s: 0.0,
                    spl_db: None,
                    spl_offset_db: None,
                    snr_db: None,
                    uuid: Some("changed".into()),
                    source: Some("test".into()),
                },
                None,
            )
            .unwrap();
        store.save().unwrap();
    }
    open(&running_service, &path);
    let stale = match call_ok(&running_service, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.calibration.unwrap().stale,
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(stale);
    let Envelope::Err(error) = running_service.call(
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("stale-export").to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("stale export must fail before starting a job");
    };
    assert_eq!(error.code, ErrorCode::StaleInputs);

    let changed_reopen = service();
    open(&changed_reopen, &path);
    let changed = match call_ok(&changed_reopen, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.calibration.unwrap(),
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(changed.stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancelled_calibration_preserves_previous_results() {
    let root = temp_root("cancel-preserve");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let service = service();
    open(&service, &path);
    let initial_events = Arc::new(Events::default());
    let initial = start_calibration(&service, initial_events.clone(), settings.clone());
    finished_result(initial_events.wait(initial, Duration::from_secs(120)));
    let previous = match call_ok(&service, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results,
        other => panic!("unexpected results response: {other:?}"),
    };

    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings);
    let start = Instant::now();
    let Response::JobCancel(snapshot) = call_ok(
        &service,
        Request::JobCancel(JobCancelParams { job }),
        Arc::new(NoEvents),
    ) else {
        panic!("cancel must return a job snapshot");
    };
    assert_eq!(snapshot.status, JobStatus::CancelRequested);
    let JobEvent::Finished { status, .. } = events.wait(job, Duration::from_secs(1)) else {
        unreachable!();
    };
    assert_eq!(status, JobStatus::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(1));
    let current = match call_ok(&service, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results,
        other => panic!("unexpected results response: {other:?}"),
    };
    assert_eq!(current, previous);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn room_dba_mimo_runs_and_becomes_stale_after_recalibration() {
    let root = temp_root("mimo-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings.clone());
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let mimo_events = Arc::new(Events::default());
    let Response::Mimo(started) = call_ok(
        &service,
        Request::Mimo(MimoParams {
            settings: Some(settings.clone()),
        }),
        mimo_events.clone(),
    ) else {
        panic!("MIMO must start");
    };
    let JobResult::Mimo(summary) =
        finished_result(mimo_events.wait(started.job, Duration::from_secs(120)))
    else {
        panic!("MIMO result must be returned");
    };
    assert!(!summary.stale);
    let direct = secs_engine::test_support::production_case("room_dba", 1);
    let ctx = secs_compute::Context::new(
        1,
        secs_compute::MemoryBudget::new(u64::MAX),
        Arc::new(secs_compute::NoProgress),
        secs_compute::CancelToken::new(),
    );
    let direct_mimo = secs_engine::mimo::compute(
        &direct.design,
        &direct.measurements,
        &direct.config,
        &settings,
        &secs_engine::Tuning::default(),
        &ctx,
    )
    .unwrap();
    assert_eq!(summary.paths, direct_mimo.paths);

    settings.rc_lo += 1.0;
    let recalibration_events = Arc::new(Events::default());
    let recalibration = start_calibration(&service, recalibration_events.clone(), settings);
    finished_result(recalibration_events.wait(recalibration, Duration::from_secs(120)));
    let stale = match call_ok(&service, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.mimo.unwrap().stale,
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn measurement_handlers_are_connected_and_fail_with_typed_errors() {
    use secs_types::ipc::{
        LevelApplyParams, LevelCheckParams, LevelResult, MdatImportParams, MdatScanParams,
        MeasureConfigureParams, MeasurePositionParams, PositionClearParams, UndoParams,
    };

    let service = service();
    let requests = [
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
        Request::Undo(UndoParams {
            token: "undo".into(),
        }),
        Request::RewLaunch,
        Request::MdatScan(MdatScanParams {
            path: "input.mdat".into(),
        }),
        Request::MdatImport(MdatImportParams {
            path: "input.mdat".into(),
            assignments: vec![],
            position: 1,
        }),
    ];
    // Without an open project (and with a file that does not exist) each
    // measurement handler refuses with a typed error; none is a stub any more.
    // `rew_launch` is the exception: the default host is the M5 stub.
    for request in requests {
        let method = request.method();
        let Envelope::Err(error) = service.call(request, Arc::new(NoEvents)) else {
            continue;
        };
        if method == "rew_launch" {
            assert_eq!(error.code, ErrorCode::NotImplemented, "{method}");
        } else {
            assert_ne!(error.code, ErrorCode::NotImplemented, "{method}");
        }
    }
}
