#![forbid(unsafe_code)]

#[path = "../examples/support/case_project.rs"]
mod case_project;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use secs_service::{Deps, EventSink, LocalTime, NoEvents, Service};
use secs_types::ipc::{Envelope, ErrorCode, JobEvent, ProjectNewParams, Request};
use sha2::{Digest, Sha256};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
use secs_project::MeasurementResult;
use secs_types::calibration::CalibrationSettings;
use secs_types::ipc::{
    CalibrateParams, DspSetParams, ExportKind, ExportParams, JobCancelParams, JobId, JobResult,
    JobStatus, MdatScope, MimoParams, OutputRateChoice, Response,
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
        unix_ms: Arc::new(|| FIXED_MS),
        ..Deps::default()
    })
}

const FIXED_MS: i64 = 1_704_164_645_000;

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
        self.try_wait(job, timeout)
            .unwrap_or_else(|| panic!("job {job:?} did not finish"))
    }

    fn try_wait(&self, job: JobId, timeout: Duration) -> Option<JobEvent> {
        let deadline = Instant::now() + timeout;
        let mut values = lock(&self.values);
        loop {
            if let Some(event) = values.iter().find(|event| {
                matches!(event, JobEvent::Finished { job: event_job, .. } if *event_job == job)
            }) {
                return Some(event.clone());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
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
fn analytic_derivative_edits_persist_and_mark_results_stale() {
    use secs_types::settings_edit::SettingEdit;

    let root = temp_root("analytic-derivatives");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let initial = service();
    open(&initial, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&initial, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert!(!read_results(&initial).calibration.unwrap().stale);

    // Each part alone, as the CLI sends it; the screen sends both at once.
    for edit in [
        SettingEdit::AnalyticDerivatives {
            peq: Some(true),
            mimo_iir: None,
        },
        SettingEdit::AnalyticDerivatives {
            peq: None,
            mimo_iir: Some(true),
        },
    ] {
        call_ok(
            &initial,
            Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
                edit,
            }),
            Arc::new(NoEvents),
        );
    }
    let results = read_results(&initial);
    let calibration = results.calibration.unwrap();
    assert!(calibration.stale);
    assert_eq!(calibration.stale_reason.as_deref(), Some("settings"));
    call_ok(
        &initial,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );

    let reopened = service();
    open(&reopened, &path);
    let Response::CalibrationSettingsGet(settings) = call_ok(
        &reopened,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!("expected reopened settings")
    };
    assert!(settings.peq_analytic_derivatives);
    assert!(settings.mimo_iir_analytic_derivatives);
    assert!(read_results(&reopened).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

fn read_results(service: &Service) -> secs_types::ipc::ResultsSummary {
    let Response::Results(results) = call_ok(service, Request::Results, Arc::new(NoEvents)) else {
        panic!()
    };
    results
}

fn cached_field<T: serde::de::DeserializeOwned>(project: &Path, name: &str) -> T {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(project.with_extension("irs").join("secs_run_cache.json")).unwrap(),
    )
    .unwrap();
    serde_json::from_value(value["results"][name].clone()).unwrap()
}
fn cached_chain(project: &Path) -> secs_types::chain::SignalChain {
    cached_field(project, "calibration_chain")
}
fn cached_paths(project: &Path) -> secs_types::chain::MimoPaths {
    cached_field(project, "mimo_paths")
}

#[test]
fn observer_receives_every_caller_event_in_the_same_order() {
    let root = temp_root("observer");
    fs::create_dir_all(&root).unwrap();
    let observer = Arc::new(Events::default());
    let service = Service::new(Deps {
        observer: observer.clone(),
        ..Deps::default()
    });
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: root.join("sample.secs").to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: true,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let caller = Arc::new(Events::default());
    let Response::SampleLoad(started) = call_ok(
        &service,
        Request::SampleLoad(Default::default()),
        caller.clone(),
    ) else {
        panic!("sample load must start")
    };
    observer.wait(started.job, Duration::from_secs(10));
    caller.wait(started.job, Duration::from_secs(10));
    assert_eq!(*lock(&caller.values), *lock(&observer.values));
    fs::remove_dir_all(root).unwrap();
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
        let saved = created.save_text();
        let legacy = saved
            .lines()
            .filter(|line| !line.trim_start().starts_with("\"output_rate\":"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(
            legacy.into_bytes(),
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
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
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

fn stored_titles_and_rates(path: &Path) -> Vec<(u32, String, u32)> {
    let mut store = secs_project::MeasurementStore::open(path).unwrap();
    let mut records = store.records();
    records.sort_by_key(|record| record.pos);
    let plan_roles = secs_project::plan_from_project(store.project(), std::iter::empty()).roles;
    let mut stored = Vec::new();
    for record in records {
        let position = u32::try_from(record.pos).unwrap();
        let mut roles = record.ir.keys().cloned().collect::<Vec<_>>();
        roles.sort_by_key(|role| {
            plan_roles
                .iter()
                .position(|planned| planned == role)
                .unwrap_or(plan_roles.len())
        });
        for role in roles {
            let (_, sample_rate) = store.ir_of(&role, record.pos).unwrap();
            stored.push((position, format!("{role}_P{position}"), sample_rate));
        }
    }
    stored
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
    events: Arc<dyn EventSink>,
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

fn assert_snapshots_equal(left: &[(PathBuf, Vec<u8>)], right: &[(PathBuf, Vec<u8>)]) {
    assert_eq!(
        left.iter().map(|(path, _)| path).collect::<Vec<_>>(),
        right.iter().map(|(path, _)| path).collect::<Vec<_>>()
    );
    for ((path, left_bytes), (_, right_bytes)) in left.iter().zip(right) {
        assert!(left_bytes == right_bytes, "{} differs", path.display());
    }
}

fn compare_snapshot(root: &Path, snapshot: &[(PathBuf, Vec<u8>)], direct_files: &[PathBuf]) {
    let mut direct_relative: Vec<_> = direct_files
        .iter()
        .map(|file| file.strip_prefix(root).unwrap().to_owned())
        .filter(|relative| relative != Path::new("secs-design.json"))
        .collect();
    direct_relative.sort();
    let comparable: Vec<_> = snapshot
        .iter()
        .filter(|(relative, _)| relative != Path::new("secs-design.json"))
        .collect();
    assert_eq!(
        comparable
            .iter()
            .map(|(relative, _)| relative)
            .collect::<Vec<_>>(),
        direct_relative.iter().collect::<Vec<_>>()
    );
    for (relative, expected) in comparable {
        assert!(
            expected == &fs::read(root.join(relative)).unwrap(),
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
    export_scope_and_wait(service, kind, folder, None, events)
}

fn export_scope_and_wait(
    service: &Service,
    kind: ExportKind,
    folder: &Path,
    mdat_scope: Option<secs_types::ipc::MdatScope>,
    events: Arc<Events>,
) -> secs_types::ipc::ExportResult {
    let Response::Export(started) = call_ok(
        service,
        Request::Export(ExportParams {
            kind,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
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
fn changing_only_output_rate_keeps_calibration_fresh_for_export() {
    let root = temp_root("output-rate-fresh");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let service = deterministic_service();
    open(&service, &path);

    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let Response::DspSet(camilla) = call_ok(
        &service,
        Request::DspSet(DspSetParams {
            preset: Some("camilla".into()),
            fields: None,
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("CamillaDSP preset must be selected")
    };
    let mut fields = camilla.fields;
    fields.output_rate = OutputRateChoice::Fixed(96_000);
    let Response::DspSet(changed) = call_ok(
        &service,
        Request::DspSet(DspSetParams {
            preset: None,
            fields: Some(fields),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("output rate must change")
    };
    assert_eq!(changed.fields.output_rate, OutputRateChoice::Fixed(96_000));

    let result = export_and_wait(
        &service,
        ExportKind::CamillaDsp,
        &root.join("export"),
        Arc::new(Events::default()),
    );
    assert!(!result.stale);
    assert_eq!(result.output_rate, 96_000);
    assert!(result.notes.is_empty());
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(Path::new(&result.folder).join("secs-design.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["sample_rate"], 96_000);
    assert_eq!(manifest["design_sample_rate"], 48_000);
    assert_eq!(manifest["output_rate_auto"], false);
    fs::remove_dir_all(root).unwrap();
}

fn calibrate_and_run_mimo(service: &Service, settings: &CalibrationSettings) {
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(service, calibration_events.clone(), settings.clone());
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));
    let mimo_events = Arc::new(Events::default());
    let Response::Mimo(started) = call_ok(
        service,
        Request::Mimo(MimoParams {
            settings: Some(settings.clone()),
        }),
        mimo_events.clone(),
    ) else {
        panic!("MIMO must start");
    };
    finished_result(mimo_events.wait(started.job, Duration::from_secs(120)));
}

fn replace_measurement(service: &Service, role: &str) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/goldens/mdat/build_3_real_48000_bytes.bin");
    let Response::MdatScan(scan) = call_ok(
        service,
        Request::MdatScan(secs_types::ipc::MdatScanParams {
            path: source.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("replacement mdat must scan");
    };
    let replacement = scan.measurements.first().unwrap();
    let events = Arc::new(Events::default());
    let Response::MdatImport(started) = call_ok(
        service,
        Request::MdatImport(secs_types::ipc::MdatImportParams {
            path: source.to_string_lossy().into_owned(),
            assignments: vec![secs_types::ipc::MdatAssignment {
                measurement: replacement.title.clone(),
                role: role.into(),
                index: Some(replacement.index),
                position: Some(1),
            }],
            position: 1,
        }),
        events.clone(),
    ) else {
        panic!("replacement mdat import must start");
    };
    finished_result(events.wait(started.job, Duration::from_secs(120)));
}

fn automatic_camilla_has_mimo(service: &Service, root: &Path) -> bool {
    call_ok(
        service,
        Request::ExportFolderSet(secs_types::ipc::ExportFolderSetParams {
            folder: root.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    );
    let result = export_and_wait(
        service,
        ExportKind::CamillaDsp,
        Path::new(""),
        Arc::new(Events::default()),
    );
    result.folder.contains("_MIMO_")
}

#[test]
fn bootstrap_reports_version_and_upstream_base() {
    let service = Service::new(Deps {
        app_version: "42.0.0".into(),
        ..Deps::default()
    });
    let Envelope::Ok(response) = service.call(Request::Bootstrap, Arc::new(NoEvents)) else {
        panic!("bootstrap must succeed");
    };
    let Response::Bootstrap(info) = *response else {
        panic!("bootstrap response must match the request");
    };
    assert_eq!(info.app_name, "SECS 2R");
    assert_eq!(info.version, "42.0.0");
    assert!(!info.prerelease);
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
fn resampled_calibration_and_cached_mdat_export_preserve_original_ir() {
    use secs_types::calibration::SampleRatePolicy;
    let root = temp_root("resample-flow");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    let ir_path = secs_project::ir_file(&path, "L", 1);
    let (samples, _) = secs_project::read_wav_float32(&fs::read(&ir_path).unwrap()).unwrap();
    fs::write(&ir_path, secs_project::write_wav_float32(&samples, 96_000)).unwrap();
    let original = fs::read(&ir_path).unwrap();
    let original_project: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let service = deterministic_service();
    open(&service, &path);

    let rejected = Arc::new(Events::default());
    let job = start_calibration(&service, rejected.clone(), settings.clone());
    let JobEvent::Finished {
        status,
        error: Some(error),
        ..
    } = rejected.wait(job, Duration::from_secs(120))
    else {
        panic!("default policy must reject the mismatched measurement")
    };
    assert_eq!(status, JobStatus::Failed);
    assert_eq!(
        error.params.get("actual_hz").map(String::as_str),
        Some("96000")
    );

    settings.sample_rate_policy = SampleRatePolicy::Resample;
    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert_eq!(fs::read(&ir_path).unwrap(), original);
    let summary = read_results(&service).calibration.unwrap();
    let notice = summary
        .notes
        .iter()
        .find(|notice| notice.code == "measurement_resampled")
        .expect("converted measurements must be visible in the calculation notes");
    assert_eq!(notice.params["count"], "1");
    assert_eq!(notice.params["target_hz"], "48000");
    let labels: serde_json::Value = cached_field(&path, "labels");
    assert_eq!(labels["sample_rate_policy"], "resample");
    let xo_events = Arc::new(Events::default());
    let Response::RecommendXo(xo) = call_ok(&service, Request::RecommendXo, xo_events.clone())
    else {
        panic!("XO recommendation must start with the resampling policy")
    };
    assert!(matches!(
        finished_result(xo_events.wait(xo.job, Duration::from_secs(120))),
        JobResult::Xo(_)
    ));

    // Saving another preparation policy makes the result stale. A stale cached
    // design cannot be rebuilt for mdat until calibration runs again.
    call_ok(
        &service,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::SampleRatePolicy {
                policy: SampleRatePolicy::Reject,
            },
        }),
        Arc::new(NoEvents),
    );
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );
    let reopened = deterministic_service();
    open(&reopened, &path);
    assert!(read_results(&reopened).calibration.unwrap().stale);
    let Envelope::Err(error) = reopened.call(
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("cached-mdat").to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("stale cached mdat export must fail")
    };
    assert_eq!(error.code, ErrorCode::ExportScope);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("stale_cache")
    );
    let originals = export_scope_and_wait(
        &reopened,
        ExportKind::BeforeAfterMdat,
        &root.join("cached-originals"),
        Some(secs_types::ipc::MdatScope::Measurements),
        Arc::new(Events::default()),
    );
    assert!(!originals.stale);
    let loaded = secs_mdat::load_mdat_project(
        &fs::read(&originals.files[0]).unwrap(),
        &secs_jos::Limits::default(),
    )
    .unwrap();
    assert_eq!(
        loaded
            .measurements
            .iter()
            .find(|measurement| measurement.title == "L_P1")
            .unwrap()
            .sample_rate,
        96_000
    );
    assert_eq!(fs::read(&ir_path).unwrap(), original);
    assert_eq!(secs_project::read_wav_float32(&original).unwrap().1, 96_000);
    let saved_project: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        saved_project["measurements"],
        original_project["measurements"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn measurement_mdat_exports_every_position_without_a_result() {
    let root = temp_root("measurement-mdat");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room_dba");
    let stored = stored_titles_and_rates(&path);
    assert!(
        stored
            .iter()
            .map(|(position, _, _)| *position)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 2
    );
    let service = deterministic_service();
    open(&service, &path);
    assert!(read_results(&service).calibration.is_none());
    let base = root.join("automatic");
    call_ok(
        &service,
        Request::ExportFolderSet(secs_types::ipc::ExportFolderSetParams {
            folder: base.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    );
    let result = export_scope_and_wait(
        &service,
        ExportKind::BeforeAfterMdat,
        Path::new(""),
        Some(MdatScope::Measurements),
        Arc::new(Events::default()),
    );
    assert!(!result.stale);
    assert_eq!(result.output_rate, 48_000);
    assert!(result.notes.is_empty());
    let folder = PathBuf::from(&result.folder);
    assert_eq!(folder.parent(), Some(base.as_path()));
    assert_eq!(
        folder.file_name().and_then(|name| name.to_str()),
        Some("SECS_mdat_measurements_20240102-030405")
    );
    assert_eq!(result.files.len(), 1);
    let file = Path::new(&result.files[0]);
    assert_eq!(
        file.file_name().and_then(|name| name.to_str()),
        Some("SECS 원 측정.mdat")
    );
    assert!(!folder.join("compute-provenance.json").exists());
    let loaded =
        secs_mdat::load_mdat_project(&fs::read(file).unwrap(), &secs_jos::Limits::default())
            .unwrap();
    assert_eq!(loaded.measurements.len(), stored.len());
    for (measurement, (_, title, sample_rate)) in loaded.measurements.iter().zip(&stored) {
        assert_eq!(&measurement.title, title);
        assert_eq!(measurement.sample_rate, i64::from(*sample_rate));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rew_mdat_scope_names_say_what_the_file_holds() {
    let root = temp_root("rew-mdat-scopes");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room_dba");
    let stored = stored_titles_and_rates(&path);
    let service = deterministic_service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(calibration, Duration::from_secs(120)));

    let all = export_scope_and_wait(
        &service,
        ExportKind::BeforeAfterMdat,
        &root.join("all"),
        Some(MdatScope::All),
        Arc::new(Events::default()),
    );
    let label = settings.label();
    let all_filename = format!("SECS 원 측정+보정 결과 {label}.mdat");
    assert_eq!(
        Path::new(&all.files[0])
            .file_name()
            .and_then(|name| name.to_str()),
        Some(all_filename.as_str())
    );
    let all_loaded = secs_mdat::load_mdat_project(
        &fs::read(&all.files[0]).unwrap(),
        &secs_jos::Limits::default(),
    )
    .unwrap();
    let direct = secs_engine_testkit::production_case("room_dba", 1);
    let run = secs_export::ExportRun {
        design: &direct.design,
        settings: &settings,
    };
    let main_position = direct.design.replay.config().main_position;
    let before_after =
        secs_export::rew_mdat_items(Some(&run), MdatScope::All, &[], None, None).unwrap();
    assert_eq!(
        all_loaded.measurements[..before_after.len()]
            .iter()
            .map(|measurement| measurement.title.as_str())
            .collect::<Vec<_>>(),
        before_after
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>()
    );
    let other_titles = stored
        .iter()
        .filter(|(position, _, _)| *position != main_position)
        .map(|(_, title, _)| title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        all_loaded.measurements[before_after.len()..]
            .iter()
            .map(|measurement| measurement.title.as_str())
            .collect::<Vec<_>>(),
        other_titles
    );

    let results = export_scope_and_wait(
        &service,
        ExportKind::BeforeAfterMdat,
        &root.join("results"),
        Some(MdatScope::Results),
        Arc::new(Events::default()),
    );
    let results_filename = format!("SECS 보정 결과 {label}.mdat");
    assert_eq!(
        Path::new(&results.files[0])
            .file_name()
            .and_then(|name| name.to_str()),
        Some(results_filename.as_str())
    );
    let results_loaded = secs_mdat::load_mdat_project(
        &fs::read(&results.files[0]).unwrap(),
        &secs_jos::Limits::default(),
    )
    .unwrap();
    let expected_results =
        secs_export::rew_mdat_items(Some(&run), MdatScope::Results, &[], None, None).unwrap();
    assert_eq!(
        results_loaded
            .measurements
            .iter()
            .map(|measurement| measurement.title.as_str())
            .collect::<Vec<_>>(),
        expected_results
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>()
    );
    assert!(results_loaded.measurements.iter().all(|measurement| {
        !measurement.title.contains("_1 ")
            && (measurement.title.contains("_2 ") || measurement.title.contains("_3 "))
    }));

    let default = export_scope_and_wait(
        &service,
        ExportKind::BeforeAfterMdat,
        &root.join("default"),
        None,
        Arc::new(Events::default()),
    );
    assert_eq!(
        fs::read(&default.files[0]).unwrap(),
        fs::read(&all.files[0]).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn exported_measurements_keep_their_curves_and_results_drop_them() {
    use secs_mdat::{CalInfo, SourceInfo};

    let root = temp_root("rew-mdat-curves");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room_dba");
    let direct = secs_engine_testkit::production_case("room_dba", 1);
    let position = direct.design.replay.config().main_position;
    let role = direct.design.chain.mains[0].role.clone();
    let ir_path = secs_project::ir_file(&path, &role, i64::from(position));
    let (raw, _) = secs_project::read_wav_float32(&fs::read(&ir_path).unwrap()).unwrap();
    secs_project::save_source(
        &ir_path,
        Some(&SourceInfo {
            strings: vec![("outputName".into(), "calibrated microphone".into())],
            cal: vec![(
                "meterCal".into(),
                CalInfo {
                    inverse_c_selected: false,
                    source_name: "mic.cal".into(),
                    source_path: "mic.cal".into(),
                    freq: Some(vec![20.0, 20_000.0]),
                    gain: Some(vec![6.0, 6.0]),
                    phase: None,
                    boxed: Vec::new(),
                },
            )],
            ..SourceInfo::default()
        }),
    )
    .unwrap();
    let service = deterministic_service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(calibration, Duration::from_secs(120)));
    let export = export_scope_and_wait(
        &service,
        ExportKind::BeforeAfterMdat,
        &root.join("all"),
        Some(MdatScope::All),
        Arc::new(Events::default()),
    );
    let loaded = secs_mdat::load_mdat_project(
        &fs::read(&export.files[0]).unwrap(),
        &secs_jos::Limits::default(),
    )
    .unwrap();
    let measured = loaded
        .measurements
        .iter()
        .find(|measurement| measurement.title.contains(&format!("_{role}_1 ")))
        .unwrap();
    assert!(
        measured
            .source_info
            .cal
            .iter()
            .any(|(name, _)| name == "meterCal")
    );
    let (exported, sample_rate) = loaded.ir_provider.get_ir(&measured.uuid).unwrap();
    assert_eq!(sample_rate, 48_000);
    let expected = raw
        .iter()
        .map(|&sample| f64::from((f64::from(sample) * 0.01) as f32 / 0.01_f32))
        .collect::<Vec<_>>();
    assert_eq!(
        exported
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>()
    );
    let computed = loaded
        .measurements
        .iter()
        .find(|measurement| measurement.title.contains(&format!("_{role}_2 ")))
        .unwrap();
    let meter = computed
        .source_info
        .cal
        .iter()
        .find(|(name, _)| name == "meterCal")
        .map(|(_, meter)| meter)
        .unwrap();
    assert!(meter.source_name.is_empty());
    assert!(meter.source_path.is_empty());
    assert!(meter.freq.is_none() && meter.gain.is_none() && meter.phase.is_none());
    assert_eq!(
        computed
            .source_info
            .strings
            .iter()
            .find(|(name, _)| name == "outputName")
            .map(|(_, value)| value.as_str()),
        Some("calibrated microphone")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mdat_scope_is_refused_for_other_kinds() {
    let root = temp_root("mdat-scope-invalid");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("room.secs");
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
    for kind in [
        ExportKind::CamillaDsp,
        ExportKind::StagedMdat,
        ExportKind::FilterText,
        ExportKind::EqualizerApo,
        ExportKind::Convolution,
    ] {
        let Envelope::Err(error) = service.call(
            Request::Export(ExportParams {
                kind,
                folder: root
                    .join(format!("{kind:?}"))
                    .to_string_lossy()
                    .into_owned(),
                mdat_scope: Some(MdatScope::All),
                design_name: None,
                camilla_devices: None,
                extra_rates: Vec::new(),
            }),
            Arc::new(NoEvents),
        ) else {
            panic!("{kind:?} must refuse mdat_scope");
        };
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert_eq!(
            error.params.get("reason").map(String::as_str),
            Some("mdat_scope")
        );
    }
    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::BeforeAfterMdat,
            folder: root.join("empty").to_string_lossy().into_owned(),
            mdat_scope: Some(MdatScope::Measurements),
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("an empty project must refuse a measurement-only export");
    };
    assert_eq!(error.code, ErrorCode::EngineInput);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("no_measurements")
    );
    fs::remove_dir_all(root).unwrap();
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
    let JobResult::Calibration =
        finished_result(calibration_events.wait(calibration_job, Duration::from_secs(120)))
    else {
        panic!("calibration completion must be returned");
    };
    let results = read_results(&service);
    assert!(!results.calibration.as_ref().unwrap().stale);
    assert!(
        results
            .calibration
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .all(|notice| notice.code != "measurement_resampled")
    );
    assert_eq!(results.calibration.as_ref().unwrap().main_roles.len(), 11);
    let direct = secs_engine_testkit::production_case("dolby", 1);
    assert_eq!(cached_chain(&path), direct.design.chain);
    let blocked_folder = root.join("blocked-filter-text-subs");
    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::FilterText,
            folder: blocked_folder.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
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
    let Response::CalibrationSettingsGet(applied) = call_ok(
        &service,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(applied.xo_rec, recommendation.best);
    for (group, hz) in &recommendation.best {
        assert_eq!(
            applied.xo.iter().find(|(name, _)| name == group),
            Some(&(group.clone(), *hz))
        );
    }
    assert_eq!(
        read_results(&service).calibration.unwrap().stale_reason,
        Some("settings".into())
    );

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
                mdat_scope: None,
                design_name: None,
                camilla_devices: None,
                extra_rates: Vec::new(),
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
                    mode: export_mode(&settings),
                    note: secs_camilla::note::camilla_note(
                        settings.mode,
                        direct.design.chain.latency_ms,
                        direct.design.rc_hi,
                        None,
                    ),
                    latency_ms: direct.design.chain.latency_ms,
                };
                let project_name = {
                    let store = secs_project::MeasurementStore::open(&path).unwrap();
                    (!store.project().name.trim().is_empty())
                        .then(|| store.project().name.trim().to_owned())
                };
                let design_id = secs_graph::design::design_id(
                    &direct.design.chain,
                    None,
                    secs_graph::LfeChoice::DOLBY,
                )
                .unwrap();
                let direct_files = secs_camilla::export_with_options(
                    &direct.design.chain,
                    &folder,
                    &labels,
                    None,
                    secs_camilla::Options {
                        design: Some(secs_camilla::DesignLabels {
                            project_name,
                            design_name: secs_types::calibration::mode_label(
                                settings.mode,
                                Some(settings.ultra_ms),
                            ),
                            design_id,
                            bypass: false,
                        }),
                        headroom: true,
                        labels: true,
                        bypass: true,
                        ..secs_camilla::Options::default()
                    },
                )
                .unwrap();
                assert_eq!(export.files.len(), direct_files.len() + 1);
                compare_snapshot(&folder, &snapshot, &direct_files);
            }
            ExportKind::StagedMdat | ExportKind::BeforeAfterMdat => {
                let direct_folder = root.join(format!("direct-{kind:?}"));
                let run = secs_export::ExportRun {
                    design: &direct.design,
                    settings: &settings,
                };
                let mut uuid = || Ok(0x1234_5678_9abc_def0);
                let direct_file = match kind {
                    ExportKind::StagedMdat => secs_export::export_mdat_with_mimo_using(
                        &run,
                        &direct_folder,
                        None,
                        None,
                        FIXED_MS,
                        &mut uuid,
                    ),
                    ExportKind::BeforeAfterMdat => {
                        secs_export::export_before_after_mdat_with_mimo_using(
                            &run,
                            &direct_folder,
                            None,
                            None,
                            None,
                            FIXED_MS,
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
                    settings: &settings,
                };
                let rc_pieces = secs_export::rc_pieces(&export_run);
                let label = settings.label();
                let filter_run = secs_filters::FilterRun {
                    chain: &direct.design.chain,
                    mimo: None,
                    mode: &export_mode(&settings),
                    note: &secs_camilla::note::eqapo_note(
                        &export_mode(&settings),
                        direct.design.chain.latency_ms,
                        direct.design.rc_hi,
                        None,
                    ),
                    label: &label,
                    latency_ms: direct.design.chain.latency_ms,
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
                let snapshot: Vec<_> = snapshot
                    .into_iter()
                    .filter(|(path, _)| path != Path::new("secs-design.json"))
                    .collect();
                assert_eq!(snapshot, direct_snapshot);
            }
            ExportKind::FilterText | ExportKind::Convolution => unreachable!(),
        }
    }
    let status = service.status();
    let last = status.last_export.expect("successful export is remembered");
    assert_eq!(last.kind, ExportKind::EqualizerApo);
    assert_eq!(
        last.folder,
        root.join("export-EqualizerApo").to_string_lossy()
    );
    assert!(!last.mimo);
    assert_eq!(last.mode, "mixed");
    assert_eq!(last.time, "2024-01-02T03:04:05Z");
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
    let JobResult::Calibration =
        finished_result(calibration_events.wait(calibration, Duration::from_secs(120)))
    else {
        panic!("calibration completion must be returned");
    };
    let results = read_results(&service);
    let summary = results.calibration.unwrap();
    assert!(summary.sub_roles.is_empty());
    let chain = cached_chain(&path);

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
        chain: &chain,
        mimo: None,
        mode: "mixed",
        note: &secs_camilla::note::eqapo_note("mixed", summary.latency_ms, rc_hi_hz, None),
        label: &label,
        latency_ms: summary.latency_ms,
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
fn named_filter_bundles_replace_heal_index_hash_every_file_and_share_design_id() {
    let root = temp_root("named-filter-bundles");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("stereo.secs");
    let base = root.join("exports");
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
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(
        &service,
        calibration_events.clone(),
        CalibrationSettings::default(),
    );
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));
    call_ok(
        &service,
        Request::ExportFolderSet(secs_types::ipc::ExportFolderSetParams {
            folder: base.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    );

    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::FilterText,
            folder: String::new(),
            mdat_scope: None,
            design_name: Some("bad/name".into()),
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("an invalid design name must fail synchronously");
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("design_name_character")
    );
    assert!(!base.exists());

    let export = |kind, design_name: Option<&str>| {
        let events = Arc::new(Events::default());
        let Response::Export(started) = call_ok(
            &service,
            Request::Export(ExportParams {
                kind,
                folder: String::new(),
                mdat_scope: None,
                design_name: design_name.map(str::to_owned),
                camilla_devices: None,
                extra_rates: Vec::new(),
            }),
            events.clone(),
        ) else {
            panic!("filter export must start");
        };
        let JobResult::Export(result) =
            finished_result(events.wait(started.job, Duration::from_secs(120)))
        else {
            panic!("filter export must finish");
        };
        result
    };

    let first = export(ExportKind::CamillaDsp, Some("Cinema"));
    let named_folder = PathBuf::from(&first.folder);
    fs::write(named_folder.join("obsolete.txt"), b"old").unwrap();
    fs::write(base.join("designs.json"), b"corrupt").unwrap();
    let second = export(ExportKind::CamillaDsp, Some("Cinema"));
    assert_eq!(first.folder, second.folder);
    assert!(!named_folder.join("obsolete.txt").exists());
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(base.join("designs.json")).unwrap()).unwrap();
    assert_eq!(index.as_array().unwrap().len(), 1);
    assert_eq!(index[0]["design_name"], "Cinema");

    fn count_files(folder: &Path) -> usize {
        fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .map(|path| if path.is_dir() { count_files(&path) } else { 1 })
            .sum()
    }

    let mut design_ids = Vec::new();
    for result in [
        second,
        export(ExportKind::FilterText, Some("Cinema")),
        export(ExportKind::EqualizerApo, Some("Cinema")),
        export(ExportKind::Convolution, Some("Cinema")),
    ] {
        assert_eq!(result.output_rate, 48_000);
        assert!(result.notes.is_empty());
        let folder = PathBuf::from(&result.folder);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(folder.join("secs-design.json")).unwrap()).unwrap();
        assert_eq!(manifest["sample_rate"], 48_000);
        assert_eq!(manifest["design_sample_rate"], 48_000);
        assert_eq!(manifest["output_rate_auto"], false);
        design_ids.push(manifest["design_id"].as_str().unwrap().to_owned());
        let files = manifest["files"].as_array().unwrap();
        assert_eq!(files.len() + 1, count_files(&folder));
        for record in files {
            let bytes = fs::read(folder.join(record["path"].as_str().unwrap())).unwrap();
            assert_eq!(record["bytes"], bytes.len() as u64);
            let hash: String = Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(record["sha256"], hash);
        }
    }
    assert!(design_ids.windows(2).all(|pair| pair[0] == pair[1]));

    let unnamed = export(ExportKind::FilterText, None);
    assert!(
        Path::new(&unnamed.folder)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("20240102-030405")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn exports_after_reopening_match_pre_close_bytes() {
    let root = temp_root("reopened-exports");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let measured = secs_project::MeasurementStore::open(&path).unwrap();
    let first = measured.records().into_iter().next().unwrap();
    let pos = u32::try_from(first.pos).unwrap();
    let role = first.ir.keys().next().unwrap().clone();
    let wav = path
        .with_extension("irs")
        .join(format!("{role}_P{pos}.wav"));
    secs_project::save_source(
        &wav,
        Some(&secs_mdat::SourceInfo {
            strings: vec![("outputName".into(), "원 측정 장치".into())],
            date: Some(secs_mdat::SourceDate::Int(FIXED_MS)),
            ..Default::default()
        }),
    )
    .unwrap();
    let running = deterministic_service();
    open(&running, &path);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    for kind in [
        ExportKind::CamillaDsp,
        ExportKind::StagedMdat,
        ExportKind::BeforeAfterMdat,
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
        assert_eq!(
            has_design,
            matches!(kind, ExportKind::StagedMdat | ExportKind::BeforeAfterMdat)
        );
        if kind == ExportKind::BeforeAfterMdat {
            let loaded = secs_mdat::load_mdat_project(
                &fs::read(&after.files[0]).unwrap(),
                &secs_jos::Limits::default(),
            )
            .unwrap();
            let before = loaded
                .measurements
                .iter()
                .find(|measurement| {
                    measurement.title.contains(&format!("_{role}_1 "))
                        && measurement.title.contains("보정 전")
                })
                .unwrap();
            assert_eq!(
                before
                    .source_info
                    .strings
                    .iter()
                    .find(|(key, _)| key == "outputName")
                    .map(|(_, value)| value.as_str()),
                Some("원 측정 장치")
            );
            let after = loaded
                .measurements
                .iter()
                .find(|measurement| {
                    measurement.title.contains(&format!("_{role}_2 "))
                        && measurement.title.contains("보정 후")
                })
                .unwrap();
            assert_eq!(
                after
                    .source_info
                    .strings
                    .iter()
                    .find(|(key, _)| key == "outputName")
                    .map(|(_, value)| value.as_str()),
                Some("원 측정 장치")
            );
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_as_failure_and_nested_destination_leave_projects_untouched() {
    let root = temp_root("save-as-stage");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.secs");
    let service = service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: source.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let source_irs = source.with_extension("irs");
    fs::create_dir_all(&source_irs).unwrap();
    fs::write(source_irs.join("marker"), b"source").unwrap();

    let nested = source_irs.join("inner.secs");
    let Envelope::Err(error) = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(nested.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("nested save as must fail");
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(
        error.detail,
        "cannot save a project inside its own measurement folder"
    );
    assert!(!nested.exists());
    assert!(!nested.with_extension("irs").exists());

    let target = root.join("target.secs");
    fs::create_dir(&target).unwrap();
    let target_irs = target.with_extension("irs");
    fs::create_dir(&target_irs).unwrap();
    fs::write(target_irs.join("marker"), b"destination").unwrap();
    let Envelope::Err(error) = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(target.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("unwritable project path must fail");
    };
    assert_eq!(error.code, ErrorCode::ProjectIo);
    let Response::ProjectInfo(info) = call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!();
    };
    assert_eq!(info.path, source.to_string_lossy());
    assert_eq!(fs::read(target_irs.join("marker")).unwrap(), b"destination");
    assert_eq!(fs::read(source_irs.join("marker")).unwrap(), b"source");
    assert_eq!(
        fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count(),
        0
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_as_preserves_reopened_cached_results_and_export() {
    let root = temp_root("save-as-cache");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let target = root.join("saved-as.secs");
    let service = deterministic_service();
    open(&service, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    open(&service, &source);
    assert!(read_results(&service).calibration.is_some());
    let Response::ProjectSave(info) = call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(target.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("save as must succeed");
    };
    assert_eq!(info.path, target.to_string_lossy());
    assert!(read_results(&service).calibration.is_some());
    export_and_wait(
        &service,
        ExportKind::CamillaDsp,
        &root.join("export"),
        Arc::new(Events::default()),
    );
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
    let reopened = Service::new(Deps::default());
    open(&reopened, &path);
    let cached = match call_ok(&reopened, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.calibration.unwrap(),
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(!cached.stale);
    assert!(cached.cached);
    assert_eq!(
        cached,
        secs_types::ipc::CalibrationSummary {
            cached: true,
            ..before
        }
    );

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
                    source_info: None,
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
    assert!(stale.cached);
    let Envelope::Err(error) = running_service.call(
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("stale-export").to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("stale cached mdat export must fail")
    };
    assert_eq!(error.code, ErrorCode::ExportScope);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("stale_cache")
    );

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
fn removing_a_speaker_marks_live_and_cached_calibration_stale() {
    let root = temp_root("speaker-config-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room_dba");
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(calibration, Duration::from_secs(120)));

    call_ok(
        &service,
        Request::ConfigRemoveSpeaker(secs_types::ipc::ConfigIndexParams { index: 0 }),
        Arc::new(NoEvents),
    );
    let live = read_results(&service).calibration.unwrap();
    assert!(live.stale);
    assert_eq!(live.stale_reason.as_deref(), Some("config"));
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );

    let reopened = Service::new(Deps::default());
    open(&reopened, &path);
    let cached = read_results(&reopened).calibration.unwrap();
    assert!(cached.stale);
    assert_eq!(cached.stale_reason.as_deref(), Some("config"));

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
fn mimo_becomes_stale_when_recalibration_uses_replaced_measurements() {
    let root = temp_root("mimo-measurement-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    calibrate_and_run_mimo(&service, &settings);

    replace_measurement(&service, "C");
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(calibration, Duration::from_secs(120)));

    assert!(read_results(&service).mimo.unwrap().stale);
    assert!(!automatic_camilla_has_mimo(
        &service,
        &root.join("automatic-live")
    ));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_mimo_becomes_stale_when_recalibration_uses_replaced_measurements() {
    let root = temp_root("cached-mimo-measurement-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    calibrate_and_run_mimo(&service, &settings);

    replace_measurement(&service, "C");
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(calibration, Duration::from_secs(120)));

    let reopened = Service::new(Deps::default());
    open(&reopened, &path);
    assert!(read_results(&reopened).mimo.unwrap().stale);
    assert!(!automatic_camilla_has_mimo(
        &reopened,
        &root.join("automatic-cached")
    ));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn old_cache_without_run_key_keeps_calibration_but_marks_mimo_stale() {
    let root = temp_root("old-cache-run-key");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    calibrate_and_run_mimo(&service, &settings);

    let cache_path = path.with_extension("irs").join("secs_run_cache.json");
    let mut cache: serde_json::Value =
        serde_json::from_slice(&fs::read(&cache_path).unwrap()).unwrap();
    let results = cache["results"].as_object_mut().unwrap();
    results.remove("run_key");
    results.insert("mimo_calibration_key".into(), results["engine_key"].clone());
    fs::write(&cache_path, serde_json::to_vec(&cache).unwrap()).unwrap();

    let reopened = Service::new(Deps::default());
    open(&reopened, &path);
    let results = read_results(&reopened);
    assert!(!results.calibration.unwrap().stale);
    assert!(results.mimo.unwrap().stale);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn convolution_export_with_subs_and_mimo_reopens_byte_identically() {
    let root = temp_root("convolution-reopen");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = deterministic_service();
    open(&service, &path);
    calibrate_and_run_mimo(&service, &settings);
    call_ok(
        &service,
        Request::ExportFolderSet(secs_types::ipc::ExportFolderSetParams {
            folder: root.join("automatic").to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    );
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );

    let export = export_and_wait(
        &service,
        ExportKind::Convolution,
        Path::new(""),
        Arc::new(Events::default()),
    );
    let folder = PathBuf::from(&export.folder);
    assert!(
        folder
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("SECS_convolution_RC_BA_MIMO_")
    );
    let routing: serde_json::Value =
        serde_json::from_slice(&fs::read(folder.join("routing.json")).unwrap()).unwrap();
    assert!(
        routing["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|input| input == "LFE")
    );
    let paths = routing["paths"].as_array().unwrap();
    assert!(paths.iter().any(|path| path["input"] == "LFE"));
    assert!(
        paths
            .iter()
            .any(|path| path["input"] != path["output"] && path["input"] != "LFE")
    );
    let before = snapshot_relative_files(&folder, &export.files);

    let reopened = deterministic_service();
    open(&reopened, &path);
    let events = Arc::new(Events::default());
    let after = export_and_wait(
        &reopened,
        ExportKind::Convolution,
        Path::new(""),
        events.clone(),
    );
    assert_eq!(Path::new(&after.folder), folder);
    assert_eq!(before, snapshot_relative_files(&folder, &after.files));
    assert!(
        !lock(&events.values).iter().any(|event| {
            matches!(event, JobEvent::Progress { stage, .. } if stage == "design")
        })
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn room_dba_mimo_runs_and_becomes_stale_after_recalibration() {
    let root = temp_root("mimo-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = deterministic_service();
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
    let JobResult::Mimo = finished_result(mimo_events.wait(started.job, Duration::from_secs(120)))
    else {
        panic!("MIMO completion must be returned");
    };
    let results = read_results(&service);
    let calibration_summary = results.calibration.unwrap();
    let summary = results.mimo.unwrap();
    assert!(!summary.stale);
    let direct = secs_engine_testkit::production_case("room_dba", 1);
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
    assert_eq!(cached_paths(&path), direct_mimo.paths);
    assert!(summary.support_count > 0);
    let expected_rows = direct.design.spectra.positions.len() * direct.design.chain.mains.len();
    assert_eq!(calibration_summary.late_energy.len(), expected_rows);
    assert_eq!(summary.scorecard.len(), direct.design.chain.mains.len());
    assert_eq!(summary.late_energy.len(), expected_rows);
    for score in &direct_mimo.scorecard {
        let restored = summary
            .scorecard
            .iter()
            .find(|row| row.role == score.role)
            .unwrap();
        assert_eq!(
            restored.after_mimo.in1_20.to_bits(),
            score.after_mimo.in1_20.to_bits()
        );
        assert_eq!(
            restored.after_mimo.gd_20.to_bits(),
            score.after_mimo.gd_20.to_bits()
        );
    }
    let main_position = direct.design.spectra.main_position;
    for role in direct_mimo
        .paths
        .paths
        .iter()
        .map(|path| path.input.as_str())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let after = calibration_summary
            .late_energy
            .iter()
            .find(|row| row.position == main_position && row.role == role)
            .unwrap();
        let after_mimo = summary
            .late_energy
            .iter()
            .find(|row| row.position == main_position && row.role == role)
            .unwrap();
        assert!(after_mimo.after_mimo_db < after.after_db);
    }

    let blocked_folder = root.join("blocked-mimo-FilterText");
    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::FilterText,
            folder: blocked_folder.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("MIMO filter text export must fail synchronously");
    };
    assert_eq!(error.code, ErrorCode::ExportScope);
    assert_eq!(error.params.get("reason").map(String::as_str), Some("subs"));
    assert!(!blocked_folder.exists());

    let eqapo_folder = root.join("mimo-eqapo");
    let eqapo = export_and_wait(
        &service,
        ExportKind::EqualizerApo,
        &eqapo_folder,
        Arc::new(Events::default()),
    );
    let body = eqapo
        .files
        .iter()
        .find(|path| {
            let path = Path::new(path);
            path.extension().is_some_and(|extension| extension == "txt")
                && path.parent().is_some_and(|parent| parent != eqapo_folder)
                && path.file_name().is_some_and(|name| name != "config.txt")
        })
        .expect("Equalizer APO filter body");
    let body = fs::read_to_string(body).unwrap();
    assert!(body.contains("# Raw input copies for MIMO support"));
    assert!(body.contains("# support "));

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
    let reopened = deterministic_service();
    open(&reopened, &path);
    let reopened_results = read_results(&reopened);
    assert_eq!(
        reopened_results.calibration.unwrap().late_energy,
        calibration_summary.late_energy
    );
    let reopened_mimo = reopened_results.mimo.unwrap();
    assert_eq!(reopened_mimo.scorecard, summary.scorecard);
    assert_eq!(reopened_mimo.late_energy, summary.late_energy);
    let cached = export_and_wait(
        &reopened,
        ExportKind::CamillaDsp,
        &folder,
        Arc::new(Events::default()),
    );
    assert_snapshots_equal(
        &live_bytes,
        &snapshot_relative_files(&folder, &cached.files),
    );

    // Preparation policy participates in the calibration identity for both
    // live and reopened results, and consequently invalidates their MIMO.
    for current in [&service, &reopened] {
        for (policy, expected_stale) in [
            (secs_types::calibration::SampleRatePolicy::Resample, true),
            (secs_types::calibration::SampleRatePolicy::Reject, false),
        ] {
            call_ok(
                current,
                Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
                    edit: secs_types::settings_edit::SettingEdit::SampleRatePolicy { policy },
                }),
                Arc::new(NoEvents),
            );
            let result = read_results(current);
            let calibration = result.calibration.unwrap();
            assert_eq!(calibration.stale, expected_stale);
            assert_eq!(
                calibration.stale_reason.as_deref(),
                expected_stale.then_some("settings")
            );
            assert_eq!(result.mimo.unwrap().stale, expected_stale);
            call_ok(
                current,
                Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
                Arc::new(NoEvents),
            );
            let reloaded = Service::new(Deps::default());
            open(&reloaded, &path);
            let result = read_results(&reloaded);
            assert_eq!(result.calibration.unwrap().stale, expected_stale);
            assert_eq!(result.mimo.unwrap().stale, expected_stale);
        }
    }

    // A compute preference change invalidates both live and reopened MIMO,
    // even when both preferences currently resolve to the CPU.
    for current in [&service, &reopened] {
        for (compute, expected_stale) in [
            (secs_types::compute::ComputePreference::Cpu, true),
            (secs_types::compute::ComputePreference::Auto, false),
        ] {
            call_ok(
                current,
                Request::AppSettingsSet(secs_types::ipc::AppSettingsParams {
                    settings: secs_types::ipc::AppSettings {
                        compute,
                        ..Default::default()
                    },
                }),
                Arc::new(NoEvents),
            );
            let Response::Results(result) = call_ok(current, Request::Results, Arc::new(NoEvents))
            else {
                panic!()
            };
            assert_eq!(result.mimo.unwrap().stale, expected_stale);
            let output = export_and_wait(
                current,
                ExportKind::CamillaDsp,
                &folder,
                Arc::new(Events::default()),
            );
            let yaml = output
                .files
                .iter()
                .find(|path| path.ends_with(".yml") || path.ends_with(".yaml"))
                .unwrap();
            assert_eq!(
                fs::read_to_string(yaml).unwrap().contains("mimo_"),
                !expected_stale
            );
            assert!(!folder.join("compute-provenance.json").exists());
        }
    }

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
                    mdat_scope: None,
                    design_name: None,
                    camilla_devices: None,
                    extra_rates: Vec::new(),
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
        MdatImportParams, MdatScanParams, MeasureConfigureParams, MeasurePositionParams,
        PositionClearParams, UndoParams,
    };

    let service = service();
    let not_open = Err((ErrorCode::ProjectNotOpen, None));
    let cases = [
        (Request::MeasureMethods, Ok("measure_methods")),
        (
            Request::MeasureConfigure(MeasureConfigureParams {
                method: "mock".into(),
                ..Default::default()
            }),
            not_open,
        ),
        (
            Request::MeasureConfigure(MeasureConfigureParams::default()),
            Err((ErrorCode::MeasurementInput, Some(("field", "method")))),
        ),
        (
            Request::MeasurePosition(MeasurePositionParams { position: 1 }),
            not_open,
        ),
        (
            Request::PositionClear(PositionClearParams { position: 1 }),
            not_open,
        ),
        (
            Request::Undo(UndoParams {
                token: "undo".into(),
            }),
            not_open,
        ),
        // The headless host reports its structured launch failure.
        (
            Request::RewLaunch,
            Err((ErrorCode::RewFailed, Some(("reason", "cannot_launch")))),
        ),
        (
            Request::MdatScan(MdatScanParams {
                path: "input.mdat".into(),
            }),
            Err((ErrorCode::MeasurementInput, Some(("field", "path")))),
        ),
        (
            Request::MdatImport(MdatImportParams {
                path: "input.mdat".into(),
                assignments: vec![],
                position: 1,
            }),
            not_open,
        ),
    ];
    // Without an open project (and with a file that does not exist) each
    // measurement handler answers exactly as below; none is a stub any more.
    for (request, expected) in cases {
        let method = request.method();
        match (service.call(request, Arc::new(NoEvents)), expected) {
            (Envelope::Ok(response), Ok(name)) => {
                assert_eq!(serde_json::to_value(&response).unwrap()["method"], name)
            }
            (Envelope::Err(error), Err((code, param))) => {
                assert_eq!(error.code, code, "{method}: {error:?}");
                if let Some((key, value)) = param {
                    assert_eq!(
                        error.params.get(key).map(String::as_str),
                        Some(value),
                        "{method}"
                    );
                }
            }
            (envelope, expected) => panic!("{method}: {envelope:?}, expected {expected:?}"),
        }
    }
}

#[derive(Debug)]
struct FakeRidge {
    fail: bool,
    cpu: secs_engine::mimo::CpuRidgeBackend,
}
impl secs_engine::mimo::RidgeBackend for FakeRidge {
    fn factor<'a>(
        &'a self,
        ata: &'a [f64],
        atb: &'a [f64],
        cancel: &secs_compute::CancelToken,
    ) -> Result<Box<dyn secs_engine::mimo::ShiftedSystem + 'a>, secs_engine::mimo::AccelerationError>
    {
        if self.fail {
            Err(secs_engine::mimo::AccelerationError::failed(
                secs_types::compute::ComputeReason::DeviceFailure,
                "simulated GPU failure",
            ))
        } else {
            self.cpu.factor(ata, atb, cancel)
        }
    }
}

#[test]
fn gpu_reports_survive_cache_and_fallback_exports() {
    use secs_types::compute::*;
    use std::sync::atomic::AtomicBool;
    let root = temp_root("gpu-report");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_fir = true;
    settings.mimo_max_supports = 1;
    let fail = Arc::new(AtomicBool::new(false));
    let provider_fail = fail.clone();
    let deps = Deps {
        acceleration: Arc::new(move |preference| {
            if preference == ComputePreference::Cpu {
                return secs_engine::mimo::AccelerationSelection {
                    status: ComputeStatus {
                        preference,
                        reason: ComputeReason::UserCpu,
                        ..Default::default()
                    },
                    backend: None,
                };
            }
            let device = GpuDeviceInfo {
                id: 0,
                name: "Test GPU".into(),
                compute_major: 7,
                compute_minor: 5,
                multiprocessors: 48,
                memory_bytes: 8 << 30,
                integrated: false,
                supported: true,
                recommended: true,
                reason: ComputeReason::Recommended,
            };
            secs_engine::mimo::AccelerationSelection {
                status: ComputeStatus {
                    preference,
                    selected: ComputeBackend::Cuda,
                    reason: ComputeReason::Recommended,
                    gpu_available: true,
                    device: Some(device.clone()),
                    devices: vec![device],
                },
                backend: Some(Arc::new(FakeRidge {
                    fail: provider_fail.load(Ordering::SeqCst),
                    cpu: secs_engine::mimo::CpuRidgeBackend,
                })),
            }
        }),
        ..Deps::default()
    };
    let service = Service::new(deps.clone());
    open(&service, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(job, Duration::from_secs(120)));
    let mut previous_paths = None;
    for fallback in [false, true] {
        fail.store(fallback, Ordering::SeqCst);
        let events = Arc::new(Events::default());
        let Response::Mimo(started) = call_ok(
            &service,
            Request::Mimo(MimoParams {
                settings: Some(settings.clone()),
            }),
            events.clone(),
        ) else {
            panic!()
        };
        let JobResult::Mimo = finished_result(events.wait(started.job, Duration::from_secs(120)))
        else {
            panic!()
        };
        let summary = read_results(&service).mimo.unwrap();
        let paths = cached_paths(&path);
        assert_eq!(summary.compute.requested, ComputePreference::Auto);
        assert_eq!(
            summary.compute.backend,
            if fallback {
                ComputeBackend::Cpu
            } else {
                ComputeBackend::Cuda
            }
        );
        if fallback {
            assert!(summary.compute.fallbacks > 0);
            assert!(summary.compute.cpu_systems > 0);
            assert_eq!(summary.compute.accelerated_systems, 0);
            // TD-54: the backend's own failure text survives beside the reason.
            assert_eq!(
                summary.compute.detail.as_deref(),
                Some("simulated GPU failure")
            );
            assert!(
                summary
                    .notes
                    .iter()
                    .any(|notice| notice.code == "compute_fallback")
            );
            assert_eq!(previous_paths.as_ref(), Some(&paths));
        } else {
            assert!(summary.compute.accelerated_systems > 0);
            assert_eq!(summary.compute.stage, Some(ComputeStage::IdealFirSweep));
            assert_eq!(summary.compute.device.as_deref(), Some("Test GPU"));
            assert_eq!(summary.compute.fallbacks, 0);
            assert_eq!(summary.compute.detail, None);
            previous_paths = Some(paths);
        }
        call_ok(
            &service,
            Request::ProjectSave(secs_types::ipc::ProjectSaveParams::default()),
            Arc::new(NoEvents),
        );
        let reopened = Service::new(deps.clone());
        open(&reopened, &path);
        let Response::Results(cached) = call_ok(&reopened, Request::Results, Arc::new(NoEvents))
        else {
            panic!()
        };
        assert_eq!(cached.mimo.unwrap(), summary);
        for current in [&service, &reopened] {
            let folder = root.join("export");
            let export = export_and_wait(
                current,
                ExportKind::CamillaDsp,
                &folder,
                Arc::new(Events::default()),
            );
            let provenance = folder.join("compute-provenance.json");
            assert!(
                export
                    .files
                    .iter()
                    .any(|path| Path::new(path) == provenance)
            );
            let report: ComputeReport =
                serde_json::from_slice(&fs::read(provenance).unwrap()).unwrap();
            assert_eq!(report, summary.compute);
            for disable_mimo in [true, false] {
                if disable_mimo {
                    let mut disabled = settings.clone();
                    disabled.mimo_on = false;
                    call_ok(
                        current,
                        Request::CalibrationSettingsSet(
                            secs_types::ipc::CalibrationSettingsParams { settings: disabled },
                        ),
                        Arc::new(NoEvents),
                    );
                } else {
                    call_ok(
                        current,
                        Request::AppSettingsSet(secs_types::ipc::AppSettingsParams {
                            settings: secs_types::ipc::AppSettings {
                                compute: ComputePreference::Cpu,
                                ..Default::default()
                            },
                        }),
                        Arc::new(NoEvents),
                    );
                }
                let cpu_export = export_and_wait(
                    current,
                    ExportKind::CamillaDsp,
                    &folder,
                    Arc::new(Events::default()),
                );
                assert!(!folder.join("compute-provenance.json").exists());
                assert!(
                    cpu_export
                        .files
                        .iter()
                        .all(|path| !path.ends_with("compute-provenance.json"))
                );
                // Restore the choice; the previous GPU/fallback run can be exported again.
                call_ok(
                    current,
                    Request::CalibrationSettingsSet(secs_types::ipc::CalibrationSettingsParams {
                        settings: settings.clone(),
                    }),
                    Arc::new(NoEvents),
                );
                call_ok(
                    current,
                    Request::AppSettingsSet(secs_types::ipc::AppSettingsParams {
                        settings: Default::default(),
                    }),
                    Arc::new(NoEvents),
                );
                export_and_wait(
                    current,
                    ExportKind::CamillaDsp,
                    &folder,
                    Arc::new(Events::default()),
                );
                assert!(folder.join("compute-provenance.json").exists());
            }
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[derive(Debug, Default)]
struct PausedProgress {
    events: Events,
    gate: Mutex<(bool, bool)>,
    changed: Condvar,
}
impl EventSink for PausedProgress {
    fn send(&self, event: JobEvent) {
        if matches!(event, JobEvent::Progress { .. }) {
            let mut gate = lock(&self.gate);
            if !gate.0 {
                gate.0 = true;
                self.changed.notify_all();
                while !gate.1 {
                    gate = self.changed.wait(gate).unwrap();
                }
            }
        }
        self.events.send(event);
    }
}
impl PausedProgress {
    fn wait_until_paused(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut gate = lock(&self.gate);
        while !gate.0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "worker did not reach progress gate");
            gate = self.changed.wait_timeout(gate, remaining).unwrap().0;
        }
    }
    fn release(&self) {
        lock(&self.gate).1 = true;
        self.changed.notify_all();
    }
}

#[test]
fn calibration_preserves_settings_edited_while_the_job_runs() {
    let root = temp_root("settings-race");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room_dba");
    let service = service();
    open(&service, &path);
    let events = Arc::new(PausedProgress::default());
    let job = start_calibration(&service, events.clone(), settings.clone());
    events.wait_until_paused();

    call_ok(
        &service,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::RcLo {
                hz: settings.rc_lo + 5.0,
            },
        }),
        Arc::new(NoEvents),
    );
    events.release();
    finished_result(events.events.wait(job, Duration::from_secs(120)));

    let Response::CalibrationSettingsGet(current) = call_ok(
        &service,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(current.rc_lo, settings.rc_lo + 5.0);
    let summary = read_results(&service).calibration.unwrap();
    assert!(summary.stale);
    assert_eq!(summary.stale_reason.as_deref(), Some("settings"));

    fs::remove_dir_all(root).unwrap();
}

fn mimo_notes(files: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|file| file.ends_with(".mdat"))
        .flat_map(|file| {
            secs_mdat::load_mdat_project(&fs::read(file).unwrap(), &secs_jos::Limits::default())
                .unwrap()
                .measurements
        })
        .filter(|measurement| measurement.title.contains("MIMO"))
        .map(|measurement| measurement.notes)
        .collect()
}

#[test]
fn staged_mdat_states_the_mimo_support_cap_like_upstream_live_and_cached() {
    let root = temp_root("mimo-cap-text");
    fs::create_dir_all(&root).unwrap();
    let (source, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 4;
    let mut store = secs_project::MeasurementStore::open(&source).unwrap();
    let mut project_settings = secs_project::calibration_from_project(store.project());
    project_settings.mimo_on = true;
    project_settings.mimo_max_supports = 4;
    secs_project::store_calibration(store.project_mut(), &project_settings);
    store.save().unwrap();
    let running = service();
    open(&running, &source);
    calibrate_and_run_mimo(&running, &settings);

    // `calibration.py:1000-1002`: "(Fsiso 150 Hz · 입력당 서포트 ≤ 4)".
    let live = export_and_wait(
        &running,
        ExportKind::StagedMdat,
        &root.join("live"),
        Arc::new(Events::default()),
    );
    let reopened = service();
    open(&reopened, &source);
    assert!(!read_results(&reopened).mimo.unwrap().stale);
    let cached = export_and_wait(
        &reopened,
        ExportKind::StagedMdat,
        &root.join("cached"),
        Arc::new(Events::default()),
    );
    for (path, files) in [("live", &live.files), ("cached", &cached.files)] {
        let notes = mimo_notes(files);
        assert!(!notes.is_empty(), "{path}: no MIMO measurement");
        for note in notes {
            assert!(
                note.contains(" Hz · 입력당 서포트 ≤ 4)"),
                "{path}: cap text missing: {note}"
            );
            assert!(!note.contains("max"), "{path}: raw cap text: {note}");
        }
    }
    fs::remove_dir_all(root).unwrap();
}

fn spl_has_mimo_curve(service: &Service) -> bool {
    let Response::ResultCurves(curves) = call_ok(
        service,
        Request::ResultCurves(secs_types::ipc::ResultCurvesParams {
            tab: secs_types::ipc::ResultTab::Spl,
            channel: None,
            group: None,
            xo_hz: None,
            corrected: None,
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    curves.curves.iter().any(|curve| curve.kind == "mimo")
}

fn support_counts(service: &Service) -> Vec<Option<usize>> {
    let Response::ResultLines(lines) = call_ok(service, Request::ResultLines, Arc::new(NoEvents))
    else {
        panic!()
    };
    lines
        .channels
        .iter()
        .map(|line| line.mimo_support_count)
        .collect()
}

#[test]
fn result_curves_and_lines_drop_mimo_after_the_compute_preference_changes() {
    let root = temp_root("mimo-compute-curves");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    calibrate_and_run_mimo(&service, &settings);
    assert!(!read_results(&service).mimo.unwrap().stale);
    assert!(spl_has_mimo_curve(&service));
    assert!(support_counts(&service).iter().all(Option::is_some));

    let Response::AppSettingsGet(mut app) =
        call_ok(&service, Request::AppSettingsGet, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert_ne!(app.compute, secs_types::compute::ComputePreference::Cpu);
    app.compute = secs_types::compute::ComputePreference::Cpu;
    call_ok(
        &service,
        Request::AppSettingsSet(secs_types::ipc::AppSettingsParams { settings: app }),
        Arc::new(NoEvents),
    );

    assert!(read_results(&service).mimo.unwrap().stale);
    assert!(!spl_has_mimo_curve(&service), "stale MIMO is still drawn");
    assert!(
        support_counts(&service).iter().all(Option::is_none),
        "stale MIMO support counts are still listed"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reopened_fresh_cache_keeps_mimo_in_mdat() {
    let root = temp_root("fresh-cached-mimo-mdat");
    fs::create_dir_all(&root).unwrap();
    let (source, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let mut store = secs_project::MeasurementStore::open(&source).unwrap();
    let mut project_settings = secs_project::calibration_from_project(store.project());
    project_settings.mimo_on = true;
    project_settings.mimo_max_supports = 1;
    secs_project::store_calibration(store.project_mut(), &project_settings);
    store.save().unwrap();
    let running = deterministic_service();
    open(&running, &source);
    calibrate_and_run_mimo(&running, &settings);

    let live_folder = root.join("live-mdat");
    let live = export_and_wait(
        &running,
        ExportKind::StagedMdat,
        &live_folder,
        Arc::new(Events::default()),
    );
    let live_snapshot = snapshot_relative_files(&live_folder, &live.files);

    let reopened = deterministic_service();
    open(&reopened, &source);
    let cached = read_results(&reopened);
    assert!(cached.calibration.unwrap().cached);
    assert!(!cached.mimo.unwrap().stale);
    let reopened_folder = root.join("reopened-mdat");
    let reopened_export = export_and_wait(
        &reopened,
        ExportKind::StagedMdat,
        &reopened_folder,
        Arc::new(Events::default()),
    );
    let reopened_snapshot = snapshot_relative_files(&reopened_folder, &reopened_export.files);
    assert_eq!(
        live_snapshot
            .iter()
            .map(|(_, bytes)| bytes)
            .collect::<Vec<_>>(),
        reopened_snapshot
            .iter()
            .map(|(_, bytes)| bytes)
            .collect::<Vec<_>>()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reopened_stale_cache_blocks_mdat_but_allows_camilla() {
    let root = temp_root("stale-cached-mdat");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings.clone());
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::RcLo {
                hz: settings.rc_lo + 5.0,
            },
        }),
        Arc::new(NoEvents),
    );
    let summary = read_results(&reopened).calibration.unwrap();
    assert!(summary.cached);
    assert!(summary.stale);

    let Envelope::Err(error) = reopened.call(
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("blocked-mdat").to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("stale cached mdat must be blocked")
    };
    assert_eq!(error.code, ErrorCode::ExportScope);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("stale_cache")
    );

    let camilla = export_and_wait(
        &reopened,
        ExportKind::CamillaDsp,
        &root.join("camilla"),
        Arc::new(Events::default()),
    );
    assert!(camilla.stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_mdat_export_rejects_project_replacement_before_writing() {
    let root = temp_root("replace-cached-export");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    let folder = root.join("export");
    let events = Arc::new(PausedProgress::default());
    let Response::Export(started) = call_ok(
        &reopened,
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    events.wait_until_paused();
    call_ok(
        &reopened,
        Request::ProjectNew(ProjectNewParams {
            path: root.join("replacement.secs").to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    events.release();
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(
        matches!(finished, JobEvent::Finished { status: JobStatus::Failed, error: Some(ref error), .. }
            if error.code == ErrorCode::StaleInputs && error.detail == "project changed while computing")
    );
    assert!(!folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_replacement_rejects_old_compute_commit_and_preserves_completion() {
    for mimo in [false, true] {
        let root = temp_root(if mimo {
            "replace-mimo"
        } else {
            "replace-calibration"
        });
        fs::create_dir_all(&root).unwrap();
        let (source, mut settings) = write_case(&root, "room_dba");
        settings.mimo_on = true;
        settings.mimo_fir = true;
        settings.mimo_max_supports = 1;
        let service = service();
        open(&service, &source);
        if mimo {
            let events = Arc::new(Events::default());
            let id = start_calibration(&service, events.clone(), settings.clone());
            finished_result(events.wait(id, Duration::from_secs(120)));
        }
        let previous_cache =
            fs::read(source.with_extension("irs").join("secs_run_cache.json")).ok();
        let events = Arc::new(PausedProgress::default());
        let request = if mimo {
            Request::Mimo(MimoParams {
                settings: Some(settings),
            })
        } else {
            Request::Calibrate(CalibrateParams {
                settings: Some(settings),
                ..Default::default()
            })
        };
        let response = call_ok(&service, request, events.clone());
        let job = match response {
            Response::Mimo(started) | Response::Calibrate(started) => started.job,
            _ => panic!(),
        };
        events.wait_until_paused();
        let replacement = root.join("replacement.secs");
        let create = Request::ProjectNew(ProjectNewParams {
            path: replacement.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        });
        if mimo {
            // Exercise open as well as new while the old compute is active.
            call_ok(&Service::new(Deps::default()), create, Arc::new(NoEvents));
            open(&service, &replacement);
        } else {
            call_ok(&service, create, Arc::new(NoEvents));
        }
        events.release();
        let finished = events.events.wait(job, Duration::from_secs(120));
        assert!(
            matches!(finished, JobEvent::Finished { status: JobStatus::Failed, result: None, error: Some(ref error), .. } if error.code == ErrorCode::StaleInputs)
        );
        let current = read_results(&service);
        assert!(current.calibration.is_none() && current.mimo.is_none());
        let Response::ProjectInfo(info) =
            call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
        else {
            panic!()
        };
        assert!(!info.dirty);
        assert!(
            !replacement
                .with_extension("irs")
                .join("secs_run_cache.json")
                .exists()
        );
        assert_eq!(
            fs::read(source.with_extension("irs").join("secs_run_cache.json")).ok(),
            previous_cache
        );
        let Response::Jobs(jobs) = call_ok(&service, Request::Jobs, Arc::new(NoEvents)) else {
            panic!()
        };
        assert!(
            jobs.iter()
                .any(|snapshot| snapshot.job == job && snapshot.status == JobStatus::Failed)
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn save_as_into_a_folder_that_does_not_exist_yet_carries_the_measurements() {
    let root = temp_root("save-as-new-folder");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.secs");
    let service = service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: source.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let source_irs = source.with_extension("irs");
    fs::create_dir_all(&source_irs).unwrap();
    fs::write(source_irs.join("marker"), b"source").unwrap();

    let target = root.join("new").join("deeper").join("copy.secs");
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(target.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    );
    assert!(target.is_file());
    assert_eq!(
        fs::read(target.with_extension("irs").join("marker")).unwrap(),
        b"source"
    );
    fs::remove_dir_all(root).unwrap();
}

/// A file swapped for a named pipe: whoever reads it waits until
/// [`Fifo::release`] hands over the original bytes, once per call.
#[cfg(unix)]
struct Fifo {
    path: PathBuf,
    bytes: Vec<u8>,
}

#[cfg(unix)]
impl Fifo {
    fn replace(path: &Path) -> Self {
        let bytes = fs::read(path).unwrap();
        fs::remove_file(path).unwrap();
        Self::create(path, bytes)
    }

    fn create(path: &Path, bytes: Vec<u8>) -> Self {
        let made = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .unwrap();
        assert!(made.success(), "mkfifo failed");
        Self {
            path: path.to_owned(),
            bytes,
        }
    }

    fn release(&self) -> std::thread::JoinHandle<()> {
        let (path, bytes) = (self.path.clone(), self.bytes.clone());
        std::thread::spawn(move || fs::write(path, bytes).unwrap())
    }
}

#[cfg(unix)]
fn call_within(service: &Arc<Service>, request: Request, timeout: Duration) -> Option<Envelope> {
    call_with_events(service, request, Arc::new(NoEvents), timeout)
}

#[cfg(unix)]
fn call_with_events(
    service: &Arc<Service>,
    request: Request,
    events: Arc<dyn EventSink>,
    timeout: Duration,
) -> Option<Envelope> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let service = service.clone();
    std::thread::spawn(move || {
        let _ = sender.send(service.call(request, events));
    });
    receiver.recv_timeout(timeout).ok()
}

#[cfg(unix)]
#[test]
fn calibration_reads_measurements_inside_its_job_outside_the_session_lock() {
    let root = temp_root("load-outside-lock");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let mut wavs: Vec<_> = fs::read_dir(path.with_extension("irs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "wav"))
        .collect();
    wavs.sort();
    let fifo = Fifo::replace(&wavs[wavs.len() / 2]);
    let service = Arc::new(service());
    open(&service, &path);

    let events = Arc::new(Events::default());
    let calibrate = Request::Calibrate(CalibrateParams {
        settings: Some(settings),
        ..Default::default()
    });
    let started =
        match call_with_events(&service, calibrate, events.clone(), Duration::from_secs(10)) {
            Some(Envelope::Ok(response)) => {
                let Response::Calibrate(started) = *response else {
                    panic!()
                };
                started
            }
            Some(Envelope::Err(error)) => panic!("{error:?}"),
            None => {
                fifo.release();
                panic!("calibrate read the measurements before it started a job");
            }
        };
    // The job now waits on the pipe. The session stays free and the compute
    // group is busy, so a second request is refused without reading a WAV.
    assert!(
        matches!(
            call_within(&service, Request::ProjectInfo, Duration::from_secs(5)),
            Some(Envelope::Ok(_))
        ),
        "project_info waited for the measurement load"
    );
    for request in [
        Request::Calibrate(CalibrateParams::default()),
        Request::RecommendXo,
    ] {
        let method = request.method();
        match call_within(&service, request, Duration::from_secs(5)) {
            Some(Envelope::Err(error)) => assert_eq!(error.code, ErrorCode::JobBusy, "{method}"),
            other => {
                fifo.release();
                panic!("{method} must be refused at once: {other:?}");
            }
        }
    }
    let Response::Jobs(jobs) = call_ok(&service, Request::Jobs, Arc::new(NoEvents)) else {
        panic!()
    };
    let job = jobs.iter().find(|job| job.job == started.job).unwrap();
    assert_eq!(job.status, JobStatus::Running);
    assert!(job.cancellable);
    call_ok(
        &service,
        Request::JobCancel(JobCancelParams { job: started.job }),
        Arc::new(NoEvents),
    );
    // Hand the bytes over in case the job already waits on the pipe. A job
    // cancelled before it reaches the pipe never opens it, so the writer is
    // left waiting rather than joined.
    fifo.release();
    let finished = events.wait(started.job, Duration::from_secs(60));
    assert!(
        matches!(
            finished,
            JobEvent::Finished {
                status: JobStatus::Cancelled,
                ..
            }
        ),
        "{finished:?}"
    );
    assert!(read_results(&service).calibration.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn mdat_import_reads_the_file_once_before_taking_the_session_lock() {
    let root = temp_root("import-once");
    fs::create_dir_all(&root).unwrap();
    let service = Arc::new(service());
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: root.join("room.secs").to_string_lossy().into_owned(),
            preset: "7.1.4".into(),
            sub_count: 1,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/goldens/mdat/build_3_real_48000_bytes.bin");
    let Response::MdatScan(scan) = call_ok(
        &service,
        Request::MdatScan(secs_types::ipc::MdatScanParams {
            path: golden.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    let fifo = Fifo::create(&root.join("room.mdat"), fs::read(&golden).unwrap());
    let events = Arc::new(Events::default());
    let Response::MdatImport(started) = call_ok(
        &service,
        Request::MdatImport(secs_types::ipc::MdatImportParams {
            path: fifo.path.to_string_lossy().into_owned(),
            assignments: scan.suggestions,
            position: 1,
        }),
        events.clone(),
    ) else {
        panic!()
    };
    assert!(
        matches!(
            call_within(&service, Request::ProjectInfo, Duration::from_secs(5)),
            Some(Envelope::Ok(_))
        ),
        "project_info waited for the mdat parse"
    );
    let writer = fifo.release();
    let Some(finished) = events.try_wait(started.job, Duration::from_secs(30)) else {
        fifo.release();
        panic!("mdat import read the file a second time");
    };
    finished_result(finished);
    writer.join().unwrap();
    let Response::ProjectInfo(info) = call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert_eq!(info.positions, [1]);
    fs::remove_dir_all(root).unwrap();
}

fn has_cache_warning(notes: &[secs_types::ipc::Notice]) -> bool {
    notes.iter().any(|notice| {
        notice.code == "cache_save_failed"
            && notice
                .params
                .get("error")
                .is_some_and(|error| !error.is_empty())
    })
}

#[test]
fn cache_save_failure_is_a_warning_on_a_successful_calculation() {
    let root = temp_root("cache-save-warning");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    // A non-empty folder where the cache file goes: the final rename fails
    // on every platform and for every user (a read-only folder does not stop root).
    let obstacle = path.with_extension("irs").join("secs_run_cache.json");
    fs::create_dir_all(obstacle.join("in-the-way")).unwrap();
    let service = service();
    open(&service, &path);

    calibrate_and_run_mimo(&service, &settings);
    let results = read_results(&service);
    let calibration = results.calibration.unwrap();
    let mimo = results.mimo.unwrap();
    assert!(!calibration.stale && !mimo.stale);
    assert!(
        has_cache_warning(&calibration.notes),
        "{:?}",
        calibration.notes
    );
    assert!(has_cache_warning(&mimo.notes), "{:?}", mimo.notes);

    // Once a save succeeds the warning goes, and it is never written to the cache.
    fs::remove_dir_all(&obstacle).unwrap();
    let events = Arc::new(Events::default());
    let Response::Mimo(started) = call_ok(
        &service,
        Request::Mimo(MimoParams {
            settings: Some(settings),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    finished_result(events.wait(started.job, Duration::from_secs(120)));
    let results = read_results(&service);
    assert!(!has_cache_warning(&results.calibration.unwrap().notes));
    assert!(!has_cache_warning(&results.mimo.unwrap().notes));
    let cached: secs_types::ipc::CalibrationSummary = cached_field(&path, "calibration");
    assert!(
        cached
            .notes
            .iter()
            .all(|notice| notice.code != "cache_save_failed")
    );
    fs::remove_dir_all(root).unwrap();
}

fn export_mode(settings: &CalibrationSettings) -> String {
    secs_camilla::note::mode_name(settings.mode, settings.ultra_ms)
}
