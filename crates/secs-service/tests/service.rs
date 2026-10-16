#![forbid(unsafe_code)]

#[path = "../examples/support/case_project.rs"]
mod case_project;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use secs_service::{Deps, EventSink, LocalTime, NoEvents, Service, VERSION};
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
        clock: Arc::new(|| LocalTime {
            year: 2024,
            month: 1,
            day: 2,
            hour: 3,
            minute: 4,
            second: 5,
        }),
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
                tutorial: false,
            }),
            Arc::new(NoEvents),
        );
        assert!(
            matches!(response, Response::ProjectNew(_)),
            "{}",
            fixture.id
        );
        let mut created =
            secs_project::Project::load_text(&fs::read_to_string(&path).unwrap()).unwrap();
        created.extra.retain(|(key, _)| key != "devices");
        assert_eq!(
            created.save_text().into_bytes(),
            fixture.blob(&fixture.outputs()["project"]),
            "{}",
            fixture.id
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn new_projects_store_none_measurement_method() {
    let root = temp_root("measurement-method-default");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("new.secs");
    let service = service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let project = secs_project::Project::load_text(&fs::read_to_string(&path).unwrap()).unwrap();
    let devices = project
        .extra
        .iter()
        .find(|(key, _)| key == "devices")
        .map(|(_, value)| value)
        .unwrap();
    let secs_project::PyJson::Object(devices) = devices else {
        panic!("devices must be an object");
    };
    assert_eq!(
        devices
            .iter()
            .find(|(key, _)| key == "method")
            .map(|(_, value)| value),
        Some(&secs_project::PyJson::Str("none".into()))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tutorial_projects_block_save_without_writing() {
    let root = temp_root("tutorial");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("tutorial.secs");
    let save_as = root.join("saved.secs");
    let service = service();
    let Response::ProjectNew(info) = call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: true,
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("tutorial must be created");
    };
    assert!(info.tutorial);
    assert!(!path.exists());
    let Envelope::Err(error) = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(save_as.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("tutorial save must fail");
    };
    assert_eq!(error.code, ErrorCode::TutorialBlocked);
    let export_folder = root.join("export");
    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::CamillaDsp,
            folder: export_folder.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("tutorial export must fail");
    };
    assert_eq!(error.code, ErrorCode::TutorialBlocked);
    assert!(!path.exists());
    assert!(!save_as.exists());
    assert!(!export_folder.exists());
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
            ..Default::default()
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
    let blocked_folder = root.join("blocked-filter-text-subs");
    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::FilterText,
            folder: blocked_folder.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("filter text with subs must fail synchronously");
    };
    assert_eq!(error.code, ErrorCode::ExportScope);
    assert_eq!(error.params.get("reason").map(String::as_str), Some("subs"));
    assert!(!blocked_folder.exists());

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
        ExportKind::EqualizerApo,
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
                    _ => unreachable!(),
                }
                .unwrap();
                assert_eq!(
                    fs::read(&export.files[0]).unwrap(),
                    fs::read(direct_file).unwrap()
                );
            }
            ExportKind::EqualizerApo => {
                let snapshot = snapshot_relative_files(&folder, &export.files);
                let direct_folder = folder.clone();
                let export_run = secs_export::ExportRun {
                    design: &direct.design,
                    measurements: &direct.measurements,
                    config: &direct.config,
                    settings: &settings,
                };
                let rc_pieces = secs_export::rc_pieces(&export_run);
                let label = settings.label();
                let filter_run = secs_filters::FilterRun {
                    chain: &direct.design.chain,
                    mimo: None,
                    mode: match settings.mode {
                        secs_types::calibration::Mode::Zero => "zero",
                        secs_types::calibration::Mode::Mixed => "mixed",
                        secs_types::calibration::Mode::Linear => "linear",
                        secs_types::calibration::Mode::Ultra => "ultra",
                    },
                    ultra_ms: settings.ultra_ms,
                    label: &label,
                    latency_ms: direct.design.chain.latency_ms,
                    rc_hi_hz: direct.design.rc_hi,
                    rc_pieces: &rc_pieces,
                    stamp: "2024-01-02 03:04",
                };
                let direct_files = secs_filters::export_eqapo(&filter_run, &direct_folder).unwrap();
                let mut direct_snapshot: Vec<_> = direct_files
                    .iter()
                    .map(|file| {
                        (
                            file.strip_prefix(&direct_folder).unwrap().to_owned(),
                            fs::read(file).unwrap(),
                        )
                    })
                    .collect();
                direct_snapshot.sort_by(|left, right| left.0.cmp(&right.0));
                assert_eq!(snapshot, direct_snapshot);
            }
            ExportKind::FilterText => unreachable!(),
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn filter_text_export_matches_direct_and_reopened_bytes_without_redesign() {
    let root = temp_root("filter-text");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("stereo.secs");
    let service = deterministic_service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    call_ok(
        &service,
        Request::MeasureConfigure(secs_types::ipc::MeasureConfigureParams {
            method: "mock".into(),
            ..Default::default()
        }),
        Arc::new(NoEvents),
    );
    let measure_events = Arc::new(Events::default());
    let Response::MeasurePosition(measure) = call_ok(
        &service,
        Request::MeasurePosition(secs_types::ipc::MeasurePositionParams { position: 1 }),
        measure_events.clone(),
    ) else {
        panic!("measurement must start");
    };
    finished_result(measure_events.wait(measure.job, Duration::from_secs(120)));
    let settings = CalibrationSettings::default();
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings.clone());
    let JobResult::Calibration(results) =
        finished_result(calibration_events.wait(calibration, Duration::from_secs(120)))
    else {
        panic!("calibration result must be returned");
    };
    let summary = results.calibration.unwrap();
    assert!(summary.chain.subs.is_empty());

    let folder = root.join("filter-text-export");
    let live = export_and_wait(
        &service,
        ExportKind::FilterText,
        &folder,
        Arc::new(Events::default()),
    );
    let live_snapshot = snapshot_relative_files(&folder, &live.files);
    let rc_note = summary
        .notes
        .iter()
        .find(|notice| notice.code == "rc_limit")
        .unwrap();
    let rc_hi_hz = rc_note.params["hz"].parse().unwrap();
    let rc_auto = rc_note.params["auto"].parse().unwrap();
    let schroeder_hz = rc_note
        .params
        .get("schroeder_hz")
        .map(|value| value.parse().unwrap());
    let rc_pieces = secs_export::rc_pieces_from_values(
        rc_hi_hz,
        rc_auto,
        schroeder_hz,
        settings.room_volume_m3,
        false,
    );
    let label = settings.label();
    let direct_run = secs_filters::FilterRun {
        chain: &summary.chain,
        mimo: None,
        mode: "mixed",
        ultra_ms: settings.ultra_ms,
        label: &label,
        latency_ms: summary.latency_ms,
        rc_hi_hz,
        rc_pieces: &rc_pieces,
        stamp: "2024-01-02 03:04",
    };
    let direct_files = secs_filters::export_filter_text(&direct_run, &folder).unwrap();
    compare_snapshot(&folder, &live_snapshot, &direct_files);

    let reopened = deterministic_service();
    open(&reopened, &path);
    let reopened_events = Arc::new(Events::default());
    let cached = export_and_wait(
        &reopened,
        ExportKind::FilterText,
        &folder,
        reopened_events.clone(),
    );
    assert_eq!(
        live_snapshot,
        snapshot_relative_files(&folder, &cached.files)
    );
    assert!(
        !lock(&reopened_events.values).iter().any(|event| {
            matches!(event, JobEvent::Progress { stage, .. } if stage == "design")
        })
    );
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

    for kind in [
        ExportKind::CamillaDsp,
        ExportKind::StagedMdat,
        ExportKind::EqualizerApo,
    ] {
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
fn stale_calibration_exports_with_flag_and_cache_survives_reopen() {
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
        Response::Results(results) => results.calibration.unwrap(),
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(stale.stale);
    assert_eq!(stale.stale_reason.as_deref(), Some("measurements"));
    let export_events = Arc::new(Events::default());
    let export = export_and_wait(
        &running_service,
        ExportKind::StagedMdat,
        &root.join("stale-export"),
        export_events,
    );
    assert!(export.stale);
    assert_eq!(Path::new(&export.folder), root.join("stale-export"));

    let changed_reopen = service();
    open(&changed_reopen, &path);
    let changed = match call_ok(&changed_reopen, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.calibration.unwrap(),
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(changed.stale);
    assert_eq!(changed.stale_reason.as_deref(), Some("measurements"));
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
    assert!(!summary.paths.paths.is_empty());

    for (kind, reason) in [
        (ExportKind::FilterText, "subs"),
        (ExportKind::EqualizerApo, "mimo"),
    ] {
        let blocked_folder = root.join(format!("blocked-mimo-{kind:?}"));
        let Envelope::Err(error) = service.call(
            Request::Export(ExportParams {
                kind,
                folder: blocked_folder.to_string_lossy().into_owned(),
            }),
            Arc::new(NoEvents),
        ) else {
            panic!("MIMO filter export must fail synchronously");
        };
        assert_eq!(error.code, ErrorCode::ExportScope);
        assert_eq!(error.params.get("reason").map(String::as_str), Some(reason));
        assert!(!blocked_folder.exists());
    }

    // The same live/cached result must produce identical explicit-folder bytes.
    let folder = root.join("explicit-mimo");
    let live = export_and_wait(
        &service,
        ExportKind::CamillaDsp,
        &folder,
        Arc::new(Events::default()),
    );
    assert_eq!(Path::new(&live.folder), folder);
    let live_bytes = snapshot_relative_files(&folder, &live.files);
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );
    let reopened = Service::new(Deps::default());
    open(&reopened, &path);
    let cached = export_and_wait(
        &reopened,
        ExportKind::CamillaDsp,
        &folder,
        Arc::new(Events::default()),
    );
    assert_eq!(live_bytes, snapshot_relative_files(&folder, &cached.files));

    for current in [&service, &reopened] {
        call_ok(
            current,
            Request::ExportFolderSet(secs_types::ipc::ExportFolderSetParams {
                folder: root.join("automatic").to_string_lossy().into_owned(),
            }),
            Arc::new(NoEvents),
        );
        let export_auto = |current: &Service| {
            let events = Arc::new(Events::default());
            let Response::Export(started) = call_ok(
                current,
                Request::Export(ExportParams {
                    kind: ExportKind::CamillaDsp,
                    folder: String::new(),
                }),
                events.clone(),
            ) else {
                panic!("export must start");
            };
            let JobResult::Export(result) =
                finished_result(events.wait(started.job, Duration::from_secs(120)))
            else {
                panic!("export result");
            };
            result
        };
        assert!(export_auto(current).folder.contains("_MIMO_"));
        let mut disabled = settings.clone();
        disabled.mimo_on = false;
        call_ok(
            current,
            Request::CalibrationSettingsSet(secs_types::ipc::CalibrationSettingsParams {
                settings: disabled,
            }),
            Arc::new(NoEvents),
        );
        let no_mimo = export_auto(current);
        assert!(!no_mimo.folder.contains("_MIMO_"));
        let yaml = no_mimo
            .files
            .iter()
            .find(|file| file.ends_with(".yml") || file.ends_with(".yaml"))
            .unwrap();
        assert!(!fs::read_to_string(yaml).unwrap().contains("mimo_"));
        let mut changed = settings.clone();
        changed.fsiso -= 1.0;
        call_ok(
            current,
            Request::CalibrationSettingsSet(secs_types::ipc::CalibrationSettingsParams {
                settings: changed,
            }),
            Arc::new(NoEvents),
        );
        assert!(!export_auto(current).folder.contains("_MIMO_"));
        let mut stale = settings.clone();
        stale.rc_lo += 1.0;
        call_ok(
            current,
            Request::CalibrationSettingsSet(secs_types::ipc::CalibrationSettingsParams {
                settings: stale,
            }),
            Arc::new(NoEvents),
        );
        let stale = export_auto(current);
        assert!(stale.stale);
        assert!(!stale.folder.contains("_MIMO_"));
    }

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
    // `rew_launch` reports the headless host's structured launch failure.
    for request in requests {
        let method = request.method();
        let Envelope::Err(error) = service.call(request, Arc::new(NoEvents)) else {
            continue;
        };
        assert_ne!(error.code, ErrorCode::NotImplemented, "{method}");
        if method == "rew_launch" {
            assert_eq!(error.code, ErrorCode::RewFailed, "{method}");
            assert_eq!(error.params["reason"], "cannot_launch");
        }
    }
}
