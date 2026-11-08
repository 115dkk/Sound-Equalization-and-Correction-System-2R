#![allow(deprecated)]
#![forbid(unsafe_code)]

use secs_service::{Deps, EventSink, MockMeasurer, NoEvents, Service, plan_for, sample};
use secs_types::ipc::*;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Default)]
struct Events {
    values: Mutex<Vec<JobEvent>>,
    changed: Condvar,
}
impl EventSink for Events {
    fn send(&self, event: JobEvent) {
        self.values.lock().unwrap().push(event);
        self.changed.notify_all();
    }
}
impl Events {
    fn wait_until(&self, timeout: Duration, predicate: impl Fn(&[JobEvent]) -> bool) {
        let deadline = Instant::now() + timeout;
        let mut values = self.values.lock().unwrap();
        while !predicate(&values) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "event timeout: {values:?}");
            values = self.changed.wait_timeout(values, remaining).unwrap().0;
        }
    }

    fn wait(&self, job: JobId, timeout: Duration) -> JobEvent {
        let deadline = Instant::now() + timeout;
        let mut values = self.values.lock().unwrap();
        loop {
            if let Some(event) = values
                .iter()
                .find(|e| matches!(e,JobEvent::Finished{job:id,..} if *id==job))
            {
                return event.clone();
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "job timeout: {values:?}");
            values = self.changed.wait_timeout(values, remaining).unwrap().0;
        }
    }
}
fn root(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/measure-ipc-{}-{name}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}
fn ok(service: &Service, request: Request, events: Arc<dyn EventSink>) -> Response {
    match service.call(request, events) {
        Envelope::Ok(r) => *r,
        other => panic!("{other:?}"),
    }
}
fn call(service: &Service, request: Request) -> Response {
    ok(service, request, Arc::new(NoEvents))
}
fn new(service: &Service, path: &std::path::Path) {
    call(
        service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
    );
}
fn configure(service: &Service, method: &str) -> MeasureConfigResult {
    let Response::MeasureConfigure(result) = call(
        service,
        Request::MeasureConfigure(MeasureConfigureParams {
            method: method.into(),
            sweep_length: Some(65536),
            timing: Some("none".into()),
            ..Default::default()
        }),
    ) else {
        panic!()
    };
    result
}
fn set_start_delay(path: &std::path::Path, seconds: u64) {
    let mut project = secs_project::Project::load_text(&fs::read_to_string(path).unwrap()).unwrap();
    project.extra.retain(|(key, _)| key != "start_delay");
    project.extra.push((
        "start_delay".into(),
        secs_project::PyJson::Str(format!("{seconds} seconds")),
    ));
    fs::write(path, project.save_text()).unwrap();
}

fn measure(service: &Service, position: i64, events: Arc<Events>) -> JobId {
    let Response::MeasurePosition(job) = ok(
        service,
        Request::MeasurePosition(MeasurePositionParams { position }),
        events,
    ) else {
        panic!()
    };
    job.job
}
fn succeeded(events: &Events, job: JobId) -> JobResult {
    match events.wait(job, Duration::from_secs(180)) {
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            result: Some(result),
            ..
        } => *result,
        other => panic!("{other:?}"),
    }
}
fn sample_load(service: &Service, positions: Option<u32>, events: Arc<Events>) -> JobResult {
    let Response::SampleLoad(job) = ok(
        service,
        Request::SampleLoad(SampleLoadParams { positions }),
        events.clone(),
    ) else {
        panic!()
    };
    succeeded(&events, job.job)
}

#[test]
fn sample_load_embedded_keeps_only_position_one() {
    let root = root("sample-load-embedded");
    let path = root.join("sample.secs");
    let service = Service::new(Deps::default());
    call(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "ignored".into(),
            sub_count: 0,
            sample: true,
            tutorial: false,
        }),
    );
    let JobResult::Measurement(first) = sample_load(&service, Some(3), Arc::new(Events::default()))
    else {
        panic!()
    };
    assert_eq!(first.positions, [1, 2, 3]);
    let JobResult::Measurement(embedded) = sample_load(&service, None, Arc::new(Events::default()))
    else {
        panic!()
    };
    assert_eq!(embedded.positions, [1]);
    let project = secs_project::MeasurementStore::open(&path).unwrap();
    let record = project.entry_for(1).unwrap();
    assert_eq!(record.ir.len(), 14);
    for entry in sample::index().entries.iter() {
        let (actual, rate) = secs_project::load_ir(&path, &entry.role, 1).unwrap();
        let (expected, expected_rate, expected_t0) = sample::ir(&entry.role, 1).unwrap();
        assert_eq!(rate, expected_rate, "{}", entry.role);
        assert_eq!(actual, expected, "{}", entry.role);
        assert_eq!(
            record.t0.get(&entry.role),
            Some(&expected_t0),
            "{}",
            entry.role
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sample_load_synthetic_structure_and_determinism() {
    let root = root("sample-load-synthetic");
    let first = root.join("first.secs");
    let second = root.join("second.secs");
    let service = Service::new(Deps::default());
    for path in [&first, &second] {
        new(&service, path);
        let error = service.call(
            Request::SampleLoad(SampleLoadParams { positions: Some(1) }),
            Arc::new(NoEvents),
        );
        assert!(matches!(
            error,
            Envelope::Err(IpcError {
                code: ErrorCode::MeasurementInput,
                ..
            })
        ));
        let JobResult::Measurement(info) =
            sample_load(&service, Some(3), Arc::new(Events::default()))
        else {
            panic!()
        };
        assert_eq!(info.positions, [1, 2, 3]);
    }
    for role in ["L", "R"] {
        for position in 1..=3 {
            let a = fs::read(
                first
                    .with_extension("irs")
                    .join(format!("{role}_P{position}.wav")),
            )
            .unwrap();
            let b = fs::read(
                second
                    .with_extension("irs")
                    .join(format!("{role}_P{position}.wav")),
            )
            .unwrap();
            assert_eq!(a, b, "{role} position {position}");
            let (values, rate) = secs_project::read_wav_float32(&a).unwrap();
            assert_eq!(rate, 48_000);
            assert_eq!(values.len(), 48_000);
            let direct = 144 + ((position - 1) as f64 * 0.35e-3 * 48_000.0).round() as usize;
            assert!(values[direct].abs() > 0.8, "{role} position {position}");
        }
    }
    let main = sample::fallback_ir("L");
    let sub = sample::fallback_ir("SW4");
    assert_eq!(main, sample::fallback_ir("L"));
    assert_eq!(sub, sample::fallback_ir("SW4"));
    let high_energy = |values: &[f32]| {
        values
            .windows(2)
            .map(|pair| f64::from(pair[1] - pair[0]).powi(2))
            .sum::<f64>()
    };
    assert!(high_energy(&sub) < high_energy(&main));
    fs::remove_dir_all(root).unwrap();
}

fn calibration(service: &Service) -> CalibrationSummary {
    let events = Arc::new(Events::default());
    let Response::Calibrate(job) = ok(
        service,
        Request::Calibrate(CalibrateParams::default()),
        events.clone(),
    ) else {
        panic!()
    };
    let JobResult::Calibration = succeeded(&events, job.job) else {
        panic!()
    };
    let Response::Results(result) = call(service, Request::Results) else {
        panic!()
    };
    result.calibration.unwrap()
}

#[test]
fn mock_flow_measurement_and_calibration() {
    let root = root("flow");
    let path = root.join("room.secs");
    let service = Service::new(Deps::default());
    new(&service, &path);
    let config = configure(&service, "mock");
    assert_eq!(config.roles, ["L", "R"]);
    let Response::MeasureMethods(methods) = call(&service, Request::MeasureMethods) else {
        panic!()
    };
    assert!(methods.methods.contains(&"mock".into()));
    assert_eq!(methods.mock_positions, [1]);
    assert_eq!(methods.mock_roles.len(), 14);
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    let values = events.values.lock().unwrap();
    assert!(matches!(values[0], JobEvent::Started { .. }));
    let role_progress = values
        .iter()
        .filter(|event| {
            matches!(event, JobEvent::Progress { stage, .. } if stage == "L" || stage == "R")
        })
        .collect::<Vec<_>>();
    for (i, role) in ["L", "R"].iter().enumerate() {
        assert!(
            matches!(role_progress[i],JobEvent::Progress{stage,done,total,..} if stage==role && *done==i as u64+1 && *total==2)
        );
    }
    assert!(path.is_file());
    assert!(path.with_extension("irs").join("L_P1.wav").is_file());
    assert!(!calibration(&service).stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mock_measure_reports_in_role_stages_td171() {
    let root = root("mock-progress");
    let path = root.join("room.secs");
    let service = Service::new(Deps::default());
    new(&service, &path);
    configure(&service, "mock");
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);

    let values = events.values.lock().unwrap();
    let first_role_done = values
        .iter()
        .position(|event| matches!(event, JobEvent::Progress { stage, .. } if stage == "L"))
        .expect("the first role must finish");
    assert!(values[..first_role_done].iter().any(|event| {
        matches!(event, JobEvent::Progress { stage, .. } if stage == "running" || stage == "process")
    }));
    let final_progress = values
        .iter()
        .rev()
        .find_map(|event| match event {
            JobEvent::Progress { done, total, .. } => Some((*done, *total)),
            _ => None,
        })
        .expect("measurement progress is absent");
    assert_eq!(final_progress.0, final_progress.1);
    drop(values);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn measurement_records_report_planned_positions_and_sources() {
    let root = root("records");
    let path = root.join("room.secs");
    let service = Service::new(Deps::default());
    new(&service, &path);
    configure(&service, "mock");
    let Response::MeasurementRecords(empty) = call(&service, Request::MeasurementRecords) else {
        panic!()
    };
    assert_eq!(empty.records.len(), 9);
    assert_eq!(
        empty
            .records
            .iter()
            .map(|record| record.position)
            .collect::<Vec<_>>(),
        (1..=9).collect::<Vec<_>>()
    );
    assert!(empty.records.iter().all(|record| !record.done));
    assert!(empty.records.iter().all(|record| record.roles.is_empty()));
    assert!(empty.records.iter().all(|record| record.sources.is_empty()));
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    let Response::MeasurementRecords(records) = call(&service, Request::MeasurementRecords) else {
        panic!()
    };
    assert_eq!(records.records.len(), 9);
    assert!(records.records[0].done);
    assert_eq!(records.records[0].roles, ["L", "R"]);
    assert_eq!(records.records[0].sources["L"], "measured");
    assert_eq!(records.records[0].sources["R"], "measured");
    assert!(records.records[1..].iter().all(|record| !record.done));
    assert!(
        records.records[1..]
            .iter()
            .all(|record| record.roles.is_empty())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn countdown_reports_seconds_and_cancel_writes_nothing() {
    let root = root("countdown");
    let path = root.join("room.secs");
    let service = Service::new(Deps::default());
    new(&service, &path);
    set_start_delay(&path, 2);
    call(
        &service,
        Request::ProjectOpen(ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }),
    );
    configure(&service, "mock");
    let before = fs::read(&path).unwrap();
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    events.wait_until(Duration::from_millis(1200), |values| {
        values.iter().any(|event| {
            matches!(event, JobEvent::Progress { job: id, stage, done: 1, total: 2 } if *id == job && stage == "countdown")
        })
    });
    call(&service, Request::JobCancel(JobCancelParams { job }));
    assert!(matches!(
        events.wait(job, Duration::from_secs(1)),
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            error: Some(IpcError {
                code: ErrorCode::Cancelled,
                ..
            }),
            ..
        }
    ));
    let values = events.values.lock().unwrap();
    assert!(!values.iter().any(|event| {
        matches!(event, JobEvent::Progress { stage, .. } if stage == "L" || stage == "R")
    }));
    drop(values);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!path.with_extension("irs").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn repetitions_accept_supported_values_and_acoustic_forces_one() {
    let root = root("repetitions");
    let path = root.join("room.secs");
    let service = Service::new(Deps::default());
    new(&service, &path);
    for repetitions in [1, 2, 4, 8] {
        let Response::MeasureConfigure(config) = call(
            &service,
            Request::MeasureConfigure(MeasureConfigureParams {
                method: "rew".into(),
                repetitions: Some(repetitions),
                timing: Some("none".into()),
                ..Default::default()
            }),
        ) else {
            panic!()
        };
        assert_eq!(config.repetitions, repetitions);
        call(
            &service,
            Request::ProjectSave(ProjectSaveParams { path: None }),
        );
        let project =
            secs_project::Project::load_text(&fs::read_to_string(&path).unwrap()).unwrap();
        let secs_project::PyJson::Object(devices) = &project
            .extra
            .iter()
            .find(|(key, _)| key == "devices")
            .unwrap()
            .1
        else {
            panic!()
        };
        assert!(devices.iter().any(|(key, value)| {
            key == "repetitions"
                && matches!(value, secs_project::PyJson::Int(stored) if stored == &repetitions.to_string())
        }));
    }
    let Envelope::Err(error) = service.call(
        Request::MeasureConfigure(MeasureConfigureParams {
            method: "rew".into(),
            repetitions: Some(3),
            ..Default::default()
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(error.code, ErrorCode::MeasurementInput);
    assert_eq!(error.params["field"], "repetitions");
    let Response::MeasureConfigure(config) = call(
        &service,
        Request::MeasureConfigure(MeasureConfigureParams {
            method: "rew".into(),
            repetitions: Some(8),
            timing: Some("acoustic".into()),
            ..Default::default()
        }),
    ) else {
        panic!()
    };
    assert_eq!(config.repetitions, 1);
    call(
        &service,
        Request::ProjectSave(ProjectSaveParams { path: None }),
    );
    let project = secs_project::Project::load_text(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(plan_for(&project, &Default::default()).repetitions, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn position_clear_undo_is_byte_exact_and_single_use() {
    let root = root("undo");
    let path = root.join("room.secs");
    let service = Service::new(Deps::default());
    new(&service, &path);
    configure(&service, "mock");
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    let before = fs::read(&path).unwrap();
    let Response::PositionClear(result) = call(
        &service,
        Request::PositionClear(PositionClearParams { position: 1 }),
    ) else {
        panic!()
    };
    assert!(result.project.positions.is_empty());
    assert!(path.with_extension("irs").join("L_P1.wav").is_file());
    call(
        &service,
        Request::Undo(UndoParams {
            token: result.token.clone(),
        }),
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(matches!(
        service.call(
            Request::Undo(UndoParams {
                token: result.token
            }),
            Arc::new(NoEvents)
        ),
        Envelope::Err(IpcError {
            code: ErrorCode::InvalidRequest,
            ..
        })
    ));
    let Response::PositionClear(result) = call(
        &service,
        Request::PositionClear(PositionClearParams { position: 1 }),
    ) else {
        panic!()
    };
    call(
        &service,
        Request::ProjectOpen(ProjectPathParams {
            path: path.to_string_lossy().into(),
        }),
    );
    assert!(matches!(
        service.call(
            Request::Undo(UndoParams {
                token: result.token
            }),
            Arc::new(NoEvents)
        ),
        Envelope::Err(IpcError {
            code: ErrorCode::InvalidRequest,
            ..
        })
    ));
    fs::remove_dir_all(root).unwrap();
}
fn rooms() -> BTreeMap<i64, secs_audio::SimRoom> {
    let project = secs_project::Project::from_preset("2.0").unwrap();
    let plan = plan_for(&project, &Default::default());
    let (room, _) = sample::room(&plan, 1).unwrap();
    BTreeMap::from([(1, room.clone()), (2, room)])
}
#[test]
fn device_group_exclusion_and_cancel_preserve_project() {
    let root = root("cancel");
    let path = root.join("room.secs");
    let service = Service::new(Deps {
        mock_measurer: {
            let calls = std::sync::atomic::AtomicUsize::new(0);
            Arc::new(move || {
                let delay = if calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0 {
                    Duration::ZERO
                } else {
                    Duration::from_secs(5)
                };
                MockMeasurer::with_rooms(rooms()).with_delay(delay)
            })
        },
        ..Default::default()
    });
    new(&service, &path);
    configure(&service, "mock");
    let first = Arc::new(Events::default());
    let job = measure(&service, 1, first.clone());
    succeeded(&first, job);
    let before = fs::read(&path).unwrap();
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    assert!(matches!(
        service.call(
            Request::MeasurePosition(MeasurePositionParams { position: 1 }),
            events.clone()
        ),
        Envelope::Err(IpcError {
            code: ErrorCode::JobBusy,
            ..
        })
    ));
    let compute_events = Arc::new(Events::default());
    let Response::Calibrate(compute) = ok(
        &service,
        Request::Calibrate(Default::default()),
        compute_events.clone(),
    ) else {
        panic!()
    };
    std::thread::sleep(Duration::from_millis(50));
    let start = Instant::now();
    call(&service, Request::JobCancel(JobCancelParams { job }));
    assert!(matches!(
        events.wait(job, Duration::from_secs(1)),
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(fs::read(&path).unwrap(), before);
    succeeded(&compute_events, compute.job);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn measuring_another_position_marks_calibration_stale() {
    let root = root("stale");
    let path = root.join("room.secs");
    let service = Service::new(Deps {
        mock_measurer: Arc::new(|| MockMeasurer::with_rooms(rooms())),
        ..Default::default()
    });
    new(&service, &path);
    configure(&service, "mock");
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    assert!(!calibration(&service).stale);
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    let Response::Results(results) = call(&service, Request::Results) else {
        panic!()
    };
    assert!(
        results.calibration.unwrap().stale,
        "same-position remeasurement must invalidate the result"
    );
    assert!(!calibration(&service).stale);
    let job = measure(&service, 2, events.clone());
    succeeded(&events, job);
    let Response::Results(results) = call(&service, Request::Results) else {
        panic!()
    };
    assert!(results.calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn mdat_scan_and_import_match_direct_oracle_bytes() {
    let root = root("mdat");
    let path = root.join("import.secs");
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = base.join("oracle/goldens/mdat/build_3_96000_bytes.bin");
    let service = Service::new(Deps::default());
    new(&service, &path);
    let Response::MdatScan(scan) = call(
        &service,
        Request::MdatScan(MdatScanParams {
            path: source.to_string_lossy().into(),
        }),
    ) else {
        panic!()
    };
    assert_eq!(scan.measurements.len(), 3);
    let assignments = scan
        .measurements
        .iter()
        .take(2)
        .zip(["L", "R"])
        .map(|(m, role)| MdatAssignment {
            measurement: m.title.clone(),
            role: role.into(),
            index: Some(m.index),
            position: Some(1),
        })
        .collect::<Vec<_>>();
    let project = secs_project::Project::load_text(&fs::read_to_string(&path).unwrap()).unwrap();
    let mut direct = secs_project::MeasurementStore::new(root.join("direct/import.secs"), project);
    let picks = assignments
        .iter()
        .map(|a| secs_service::mdat_import::Assignment {
            role: Some(a.role.clone()),
            position: a.position,
        })
        .collect::<Vec<_>>();
    secs_service::mdat_import::import(&mut direct, &source, Some(&picks), Some(1)).unwrap();
    let expected = fs::read(root.join("direct/import.secs")).unwrap();
    let events = Arc::new(Events::default());
    let Response::MdatImport(job) = ok(
        &service,
        Request::MdatImport(MdatImportParams {
            path: source.to_string_lossy().into(),
            assignments,
            position: 1,
        }),
        events.clone(),
    ) else {
        panic!()
    };
    succeeded(&events, job.job);
    assert_eq!(fs::read(&path).unwrap(), expected);
    let recorded = secs_project::MeasurementStore::open(&path).unwrap();
    for (role, notes) in [("L", "global SPL"), ("R", "item SPL")] {
        let direct_source = root
            .join("direct/import.irs")
            .join(format!("{role}_P1.src.json"));
        let service_source = path
            .with_extension("irs")
            .join(format!("{role}_P1.src.json"));
        assert!(service_source.is_file());
        assert_eq!(
            fs::read(service_source).unwrap(),
            fs::read(direct_source).unwrap()
        );
        let info = recorded.source_of(role, 1).unwrap();
        assert_eq!(info.notes.as_deref(), Some(notes));
        assert_eq!(info.format.as_deref(), Some("SECS2 measurement"));
    }
    let problems =
        secs_service::mdat_import::problem_notices(&[Some("L".into()), Some("L".into())], &[1, 1]);
    assert_eq!(problems[0].code, "mdat_duplicate_roles");
    assert!(problems[0].params.contains_key("roles"));
    assert_eq!(
        secs_service::mdat_import::problem_notices(&[], &[])[0].code,
        "mdat_nothing_selected"
    );
    fs::remove_dir_all(root).unwrap();
}

#[derive(Clone, Debug)]
struct StableCallbackTransport(secs_rew::RecordedTransport);

impl secs_rew::Transport for StableCallbackTransport {
    fn request(
        &self,
        request: &secs_rew::Request,
    ) -> Result<secs_rew::Response, secs_rew::RewError> {
        let mut request = request.clone();
        if matches!(
            request.path.as_str(),
            "/measure/subscribe" | "/measure/unsubscribe"
        ) && let Some(body) = request.body.as_mut()
        {
            body["url"] = json!("callback");
        }
        self.0.request(&request)
    }
}

struct LateProgressState {
    started: bool,
    release: Option<std::sync::mpsc::SyncSender<()>>,
    done: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
}

#[derive(Clone)]
struct LateProgressTransport {
    recorded: secs_rew::RecordedTransport,
    state: Arc<Mutex<LateProgressState>>,
}

fn read_http_response(stream: &mut std::net::TcpStream) -> Result<(), String> {
    use std::io::Read as _;

    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0_u8; 1];
        stream
            .read_exact(&mut byte)
            .map_err(|error| error.to_string())?;
        header.push(byte[0]);
    }
    let header = String::from_utf8(header).map_err(|error| error.to_string())?;
    let length = header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream
        .read_exact(&mut body)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn delayed_progress(
    url: String,
    headers_sent: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
) -> Result<(), String> {
    use std::io::Write as _;

    let target = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("unexpected callback URL: {url}"))?;
    let (address, path) = target
        .split_once('/')
        .ok_or_else(|| format!("callback URL has no path: {url}"))?;
    let mut stream = std::net::TcpStream::connect(address).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    let payload = serde_json::to_vec(&json!("50% late L")).unwrap();
    write!(
        stream,
        "POST /{path} HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\n\r\n",
        payload.len()
    )
    .map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;
    headers_sent.send(()).map_err(|error| error.to_string())?;
    release.recv().map_err(|error| error.to_string())?;
    stream
        .write_all(&payload)
        .map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;
    read_http_response(&mut stream)?;
    write!(
        stream,
        "GET /{path} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;
    read_http_response(&mut stream)
}

impl secs_rew::Transport for LateProgressTransport {
    fn request(
        &self,
        request: &secs_rew::Request,
    ) -> Result<secs_rew::Response, secs_rew::RewError> {
        let mut request = request.clone();
        if request.path == "/measure/subscribe" {
            let url = request
                .body
                .as_ref()
                .and_then(|body| body["url"].as_str())
                .expect("subscribe URL")
                .to_owned();
            let mut state = self.state.lock().unwrap();
            if !state.started {
                state.started = true;
                let (headers_tx, headers_rx) = std::sync::mpsc::sync_channel(1);
                let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
                let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
                std::thread::spawn(move || {
                    let _ = done_tx.send(delayed_progress(url, headers_tx, release_rx));
                });
                headers_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("late callback did not send headers");
                std::thread::sleep(Duration::from_millis(100));
                state.release = Some(release_tx);
                state.done = Some(done_rx);
            }
            request.body.as_mut().unwrap()["url"] = json!("callback");
        } else if request.path == "/measure/unsubscribe" {
            request.body.as_mut().unwrap()["url"] = json!("callback");
        } else if request.path == "/audio/java/output-channel"
            && request
                .body
                .as_ref()
                .is_some_and(|body| body["channel"] == "R")
        {
            let (release, done) = {
                let mut state = self.state.lock().unwrap();
                (state.release.take(), state.done.take())
            };
            if let (Some(release), Some(done)) = (release, done) {
                release.send(()).unwrap();
                done.recv_timeout(Duration::from_secs(5))
                    .expect("late callback did not finish")
                    .expect("late callback request failed");
            }
        }
        self.recorded.request(&request)
    }
}

fn get(path: &str, body: serde_json::Value, seconds: u64) -> secs_rew::RecordedExchange {
    secs_rew::RecordedExchange::new(
        secs_rew::Request::new("GET", path, None, Duration::from_secs(seconds)),
        body,
    )
}
fn post(path: &str, body: serde_json::Value) -> secs_rew::RecordedExchange {
    secs_rew::RecordedExchange::new(
        secs_rew::Request::new("POST", path, Some(body), secs_rew::READ_TIMEOUT),
        json!({"message":"ok"}),
    )
}
fn rew_script_with_cal(input_cal: &str) -> Vec<secs_rew::RecordedExchange> {
    let mut script = vec![get(
        "/version",
        json!({"message":"5.40 Beta 124 API 0.9.4"}),
        2,
    )];
    for (i, role) in ["L", "R"].iter().enumerate() {
        if i == 0 {
            script.extend([
                get("/audio/driver", json!({"driver":"Java"}), 5),
                get("/audio/java/input-device", json!("Demo USB Mic"), 5),
                get("/audio/java/output-device", json!("Demo Speakers (2ch)"), 5),
                get("/audio/java/input", json!("1: Demo In 1"), 5),
                get(
                    "/audio/java/output-devices",
                    json!(["Demo Speakers (2ch)"]),
                    5,
                ),
                get("/audio/java/output-channels", json!(["L", "R"]), 5),
                get("/audio/java/output-device", json!("Demo Speakers (2ch)"), 5),
                get("/audio/java/input-devices", json!(["Demo USB Mic"]), 5),
                get("/audio/java/inputs", json!(["1: Demo In 1"]), 5),
                get("/audio/java/input-device", json!("Demo USB Mic"), 5),
            ]);
        }
        if i == 0 {
            script.extend([
                post("/application/blocking", json!(true)),
                post("/audio/samplerate", json!({"value":48000.0,"unit":"Hz"})),
                post(
                    "/measure/sweep/configuration",
                    json!({"startFrequency":20,"endFrequency":20000,"length":"64k"}),
                ),
                post("/measure/level", json!({"value":-20.0,"unit":"dBFS"})),
                get("/measure/sweep/repetitions", json!(1), 5),
                post("/measure/timing/reference", json!("None")),
            ]);
        }
        script.push(post(
            "/measure/subscribe",
            json!({"url":"callback","parameters":{}}),
        ));
        script.extend([
            post("/audio/java/output-channel",json!({"channel":role})),
            post("/measure/naming",json!({"title":format!("{role} P1"),"namingOption":"Use as entered","prefixMeasNameWithOutput":false})),
            post("/measure/notes",json!(format!("SECS {role} P1 (none timing)"))),
            get("/measurements",json!({}),10),
        ]);
        let mut start = post("/measure/command", json!({"command":"SPL"}));
        start.request.timeout = secs_rew::MEASURE_TIMEOUT;
        script.push(start);
        script.push(get("/measurements", json!({"1":{"uuid":role}}), 10));
        let mut ir = get(
            &format!("/measurements/{role}/impulse-response"),
            json!({"data":"P4AAAL6AAAA+AAAA","sampleRate":48000.0,"startTime":-0.125}),
            15,
        );
        ir.request.query = vec![("normalised".into(), "false".into())];
        script.push(ir);
        script.extend([
            get("/spl-meter/1/levels", json!({"spl":78.5}), 3),
            get(
                &format!("/measurements/{role}"),
                json!({"signalToNoisedB":12.9}),
                5,
            ),
            get(
                &format!("/measurements/{role}"),
                json!({"splOffsetdB":116.12}),
                5,
            ),
        ]);
        script.push(post("/measure/unsubscribe", json!({"url":"callback"})));
        if i == 0 {
            script.push(get(
                "/audio/input-cal",
                json!({"separateCalFileForEachInput":false,"calDataAllInputs":{"calFilePath":input_cal}}),
                3,
            ));
        }
    }
    script
}

fn rew_script() -> Vec<secs_rew::RecordedExchange> {
    rew_script_with_cal("")
}

#[test]
fn rew_configures_once_per_position_td172() {
    let root = root("rew");
    let path = root.join("room.secs");
    let mut script = rew_script();
    script.push(get(
        "/version",
        json!({"message":"5.40 Beta 124 API 0.9.4"}),
        2,
    ));
    let transport = secs_rew::RecordedTransport::new(script);
    let copy = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(StableCallbackTransport(copy.clone()))),
        ..Default::default()
    });
    new(&service, &path);
    configure(&service, "rew");
    let Response::MeasureMethods(methods) = call(&service, Request::MeasureMethods) else {
        panic!()
    };
    assert!(methods.rew.reachable);
    assert_eq!(methods.rew.pro, None);
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    let Response::MeasureMethods(methods) = call(&service, Request::MeasureMethods) else {
        panic!()
    };
    assert_eq!(methods.rew.pro, Some(true));
    transport.assert_finished();
    let calls = transport.calls();
    assert_eq!(
        calls
            .iter()
            .filter(|request| request.path == "/measure/sweep/configuration")
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|request| request.path == "/audio/java/output-channel")
            .count(),
        2
    );
    let mut store = secs_project::MeasurementStore::open(&path).unwrap();
    for role in ["L", "R"] {
        let (samples, rate) = store.ir_of(role, 1).unwrap();
        assert_eq!(&*samples, &[1.0, -0.25, 0.125]);
        assert_eq!(rate, 48000);
        assert_eq!(store.t0_of(role, 1), -0.125);
    }
    let values = events.values.lock().unwrap();
    let low_snr_roles = values
        .iter()
        .filter_map(|event| match event {
            JobEvent::Log { code, params, .. } if code == "measurement_low_snr" => {
                Some(params["role"].as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(low_snr_roles, ["L", "R"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn late_progress_from_the_previous_role_is_ignored_td172() {
    let root = root("rew-late-progress");
    let path = root.join("room.secs");
    let transport = secs_rew::RecordedTransport::new(rew_script());
    let state = Arc::new(Mutex::new(LateProgressState {
        started: false,
        release: None,
        done: None,
    }));
    let copy = transport.clone();
    let state_copy = state.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || {
            Box::new(LateProgressTransport {
                recorded: copy.clone(),
                state: state_copy.clone(),
            })
        }),
        ..Deps::default()
    });
    new(&service, &path);
    configure(&service, "rew");
    call(&service, Request::MeasureMethods);
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    transport.assert_finished();

    let values = events.values.lock().unwrap();
    let first_role_finished = values
        .iter()
        .position(|event| matches!(event, JobEvent::Progress { stage, .. } if stage == "L"))
        .expect("L role completion");
    assert!(
        values[first_role_finished + 1..]
            .iter()
            .any(|event| matches!(event, JobEvent::Progress { stage, .. } if stage == "R"))
    );
    assert!(
        !values[first_role_finished + 1..]
            .iter()
            .any(|event| matches!(event, JobEvent::Progress { stage, .. } if stage == "running"))
    );
    drop(values);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn subscribe_failure_is_kept_when_the_role_then_fails_td215() {
    let root = root("rew-subscribe-and-sweep-failure");
    let path = root.join("room.secs");
    let mut script = rew_script();
    let subscribe = script
        .iter()
        .position(|exchange| exchange.request.path == "/measure/subscribe")
        .unwrap();
    script[subscribe].response = Ok(secs_rew::Response {
        status: 500,
        body: json!({"message":"subscribe failed"}),
    });
    let sweep = script
        .iter()
        .enumerate()
        .skip(subscribe + 1)
        .find(|(_, exchange)| exchange.request.path == "/measure/command")
        .unwrap()
        .0;
    script[sweep].response = Ok(secs_rew::Response {
        status: 500,
        body: json!({"message":"sweep failed"}),
    });
    script.truncate(sweep + 1);
    let transport = secs_rew::RecordedTransport::new(script);
    let copy = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(StableCallbackTransport(copy.clone()))),
        ..Deps::default()
    });
    new(&service, &path);
    configure(&service, "rew");
    call(&service, Request::MeasureMethods);
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());

    let JobEvent::Finished {
        status: JobStatus::Failed,
        error: Some(error),
        ..
    } = events.wait(job, Duration::from_secs(5))
    else {
        panic!("measurement must fail after the rejected sweep")
    };
    assert!(error.detail.contains("sweep failed"));
    assert!(
        error
            .params
            .get("subscribe_error")
            .is_some_and(|detail| detail.contains("subscribe failed"))
    );
    transport.assert_finished();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn subscribe_failure_is_noticed_td215() {
    let root = root("rew-subscribe-failure");
    let path = root.join("room.secs");
    let mut script = rew_script();
    let subscribe = script
        .iter()
        .position(|exchange| exchange.request.path == "/measure/subscribe")
        .unwrap();
    script[subscribe].response = Ok(secs_rew::Response {
        status: 500,
        body: json!({"message":"subscribe failed"}),
    });
    let unsubscribe = script
        .iter()
        .enumerate()
        .skip(subscribe + 1)
        .find(|(_, exchange)| exchange.request.path == "/measure/unsubscribe")
        .unwrap()
        .0;
    script.remove(unsubscribe);
    let transport = secs_rew::RecordedTransport::new(script);
    let copy = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(StableCallbackTransport(copy.clone()))),
        ..Deps::default()
    });
    new(&service, &path);
    configure(&service, "rew");
    call(&service, Request::MeasureMethods);
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    transport.assert_finished();

    let values = events.values.lock().unwrap();
    assert!(values.iter().any(|event| {
        matches!(event, JobEvent::Log { code, params, .. }
            if code == "measurement_subscribe_failed" && params["role"] == "L")
    }));
    drop(values);
    let mut store = secs_project::MeasurementStore::open(&path).unwrap();
    assert!(store.ir_of("L", 1).is_ok());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sidecar_build_failure_is_noticed_td215() {
    let root = root("rew-sidecar-failure");
    let path = root.join("room.secs");
    let cal = root.join("invalid-sensitivity.cal");
    fs::write(
        &cal,
        b"Sens Factor = 1.2.3
",
    )
    .unwrap();
    let transport = secs_rew::RecordedTransport::new(rew_script_with_cal(&cal.to_string_lossy()));
    let copy = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(StableCallbackTransport(copy.clone()))),
        ..Deps::default()
    });
    new(&service, &path);
    configure(&service, "rew");
    call(&service, Request::MeasureMethods);
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    transport.assert_finished();

    let values = events.values.lock().unwrap();
    assert!(values.iter().any(|event| {
        matches!(event, JobEvent::Log { code, params, .. }
            if code == "measurement_source_missing" && params["role"] == "L")
    }));
    drop(values);
    let mut store = secs_project::MeasurementStore::open(&path).unwrap();
    assert!(store.ir_of("L", 1).is_ok());
    assert_eq!(store.source_of("L", 1), None);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn oversized_input_cal_file_is_refused_with_a_notice_and_read_once_td170() {
    let root = root("rew-cal-limit");
    let path = root.join("room.secs");
    let cal = root.join("oversized.cal");
    fs::File::create(&cal)
        .unwrap()
        .set_len(4 * 1024 * 1024 + 1)
        .unwrap();
    let transport = secs_rew::RecordedTransport::new(rew_script_with_cal(&cal.to_string_lossy()));
    let copy = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(StableCallbackTransport(copy.clone()))),
        ..Deps::default()
    });
    new(&service, &path);
    configure(&service, "rew");
    let Response::MeasureMethods(_) = call(&service, Request::MeasureMethods) else {
        panic!("REW methods must be available")
    };
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    transport.assert_finished();

    let values = events.values.lock().unwrap();
    let notices = values
        .iter()
        .filter_map(|event| match event {
            JobEvent::Log { code, params, .. } if code == "measurement_calibration_too_large" => {
                Some(params)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    assert_eq!(
        notices[0].get("path").map(String::as_str),
        Some(cal.to_string_lossy().as_ref())
    );
    drop(values);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rew_license_observation_tracks_rejection_success_and_launch_reset() {
    let root = root("rew-license");
    let path = root.join("room.secs");
    let mut script = rew_script();
    let denied = script
        .iter()
        .position(|exchange| exchange.request.path == "/measure/sweep/configuration")
        .unwrap();
    script.truncate(denied + 1);
    script[denied].response = Ok(secs_rew::Response {
        status: 401,
        body: json!({"message":"A Pro upgrade license is required for this action"}),
    });
    script.extend([
        get("/version", json!({"message":"5.40 Beta 124 API 0.9.4"}), 2),
        get("/version", json!({"message":"5.40 Beta 124 API 0.9.4"}), 2),
    ]);
    let transport = secs_rew::RecordedTransport::new(script);
    let copy = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(StableCallbackTransport(copy.clone()))),
        ..Default::default()
    });
    new(&service, &path);
    configure(&service, "rew");

    let Response::MeasureMethods(methods) = call(&service, Request::MeasureMethods) else {
        panic!()
    };
    assert_eq!(methods.rew.pro, None);
    assert!(methods.methods.iter().any(|method| method == "rew"));

    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    assert!(matches!(
        events.wait(job, Duration::from_secs(1)),
        JobEvent::Finished {
            status: JobStatus::Failed,
            error: Some(IpcError {
                code: ErrorCode::RewProRequired,
                ..
            }),
            ..
        }
    ));

    let Response::MeasureMethods(methods) = call(&service, Request::MeasureMethods) else {
        panic!()
    };
    assert_eq!(methods.rew.pro, Some(false));
    assert!(!methods.methods.iter().any(|method| method == "rew"));

    let Response::RewLaunch(status) = call(&service, Request::RewLaunch) else {
        panic!()
    };
    assert_eq!(status.pro, None);
    transport.assert_finished();
    fs::remove_dir_all(root).unwrap();
}

#[derive(Debug)]
struct CancelAfterRole {
    service: Arc<Service>,
    events: Arc<Events>,
    path: PathBuf,
}
impl EventSink for CancelAfterRole {
    fn send(&self, event: JobEvent) {
        if let JobEvent::Progress { job, done: 1, .. } = &event {
            let store = secs_project::MeasurementStore::open(&self.path).unwrap();
            assert_eq!(store.records()[0].ir.len(), 1);
            assert!(!store.records()[0].done);
            call(
                &self.service,
                Request::JobCancel(JobCancelParams { job: *job }),
            );
        }
        self.events.send(event);
    }
}
#[test]
fn rew_cancel_keeps_already_recorded_role() {
    let root = root("rew-cancel");
    let path = root.join("room.secs");
    let mut script = rew_script();
    let second = script
        .iter()
        .enumerate()
        .filter(|(_, exchange)| exchange.request.path == "/measure/subscribe")
        .nth(1)
        .unwrap()
        .0;
    script.truncate(second);
    let transport = secs_rew::RecordedTransport::new(script);
    let copy = transport.clone();
    let service = Arc::new(Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(StableCallbackTransport(copy.clone()))),
        ..Default::default()
    }));
    new(&service, &path);
    configure(&service, "rew");
    call(&service, Request::MeasureMethods);
    let events = Arc::new(Events::default());
    let sink = Arc::new(CancelAfterRole {
        service: service.clone(),
        events: events.clone(),
        path: path.clone(),
    });
    let Response::MeasurePosition(job) = ok(
        &service,
        Request::MeasurePosition(MeasurePositionParams { position: 1 }),
        sink,
    ) else {
        panic!()
    };
    assert!(matches!(
        events.wait(job.job, Duration::from_secs(1)),
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
    transport.assert_finished();
    let store = secs_project::MeasurementStore::open(&path).unwrap();
    assert_eq!(store.records()[0].ir.len(), 1);
    assert!(store.records()[0].ir.contains_key("L"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn undo_retains_only_last_sixteen_tokens_and_original_order() {
    let root = root("undo-cap");
    let path = root.join("room.secs");
    let mut project = secs_project::Project::from_preset("2.0").unwrap();
    project.measurements = secs_project::PyJson::Array(
        (1..=17)
            .map(|pos| {
                secs_project::PyJson::Object(vec![(
                    "pos".into(),
                    secs_project::PyJson::Int(pos.to_string()),
                )])
            })
            .collect(),
    );
    fs::write(&path, project.save_text()).unwrap();
    let service = Service::new(Deps::default());
    call(
        &service,
        Request::ProjectOpen(ProjectPathParams {
            path: path.to_string_lossy().into(),
        }),
    );
    let mut tokens = vec![];
    for position in (1..=17).rev() {
        let Response::PositionClear(undo) = call(
            &service,
            Request::PositionClear(PositionClearParams { position }),
        ) else {
            panic!()
        };
        tokens.push(undo.token);
    }
    assert!(matches!(
        service.call(
            Request::Undo(UndoParams {
                token: tokens[0].clone()
            }),
            Arc::new(NoEvents)
        ),
        Envelope::Err(IpcError {
            code: ErrorCode::InvalidRequest,
            ..
        })
    ));
    for token in tokens.into_iter().skip(1).rev() {
        call(&service, Request::Undo(UndoParams { token }));
    }
    let loaded = secs_project::Project::load_text(&fs::read_to_string(&path).unwrap()).unwrap();
    let secs_project::PyJson::Array(entries) = loaded.measurements else {
        panic!()
    };
    assert_eq!(entries.len(), 16);
    assert_eq!(
        entries,
        (1..=16)
            .map(|pos| secs_project::PyJson::Object(vec![(
                "pos".into(),
                secs_project::PyJson::Int(pos.to_string())
            )]))
            .collect::<Vec<_>>()
    );
    fs::remove_dir_all(root).unwrap();
}

#[derive(Debug)]
struct LaunchHost(Result<(), secs_service::RewLaunchError>);

impl secs_service::Host for LaunchHost {
    fn launch_rew(&self, _executable: Option<&Path>) -> Result<(), secs_service::RewLaunchError> {
        self.0.clone()
    }
}

#[derive(Debug, Default)]
struct RecordingLaunchHost(Mutex<Vec<Option<PathBuf>>>);

impl secs_service::Host for RecordingLaunchHost {
    fn launch_rew(&self, executable: Option<&Path>) -> Result<(), secs_service::RewLaunchError> {
        self.0
            .lock()
            .unwrap()
            .push(executable.map(Path::to_path_buf));
        Ok(())
    }
}

/// TD-221: a project may name the REW executable. The launch hands that path
/// to the host instead of the fixed install paths, an empty value clears it,
/// and a relative path is refused before it is stored.
#[test]
fn rew_launch_tries_the_configured_executable_first_td221() {
    let root = root("rew-executable");
    let host = Arc::new(RecordingLaunchHost::default());
    let service = Service::new(Deps {
        host: host.clone(),
        ..Default::default()
    });
    new(&service, &root.join("room.secs"));
    call(&service, Request::RewLaunch);

    let exe = root.join("tools").join("roomeqwizard.exe");
    let exe_text = exe.to_string_lossy().into_owned();
    let Response::MeasureConfigure(result) = call(
        &service,
        Request::MeasureConfigure(MeasureConfigureParams {
            method: "rew".into(),
            rew_executable: Some(exe_text.clone()),
            ..Default::default()
        }),
    ) else {
        panic!()
    };
    assert_eq!(result.rew_executable.as_deref(), Some(exe_text.as_str()));
    call(&service, Request::RewLaunch);

    let Envelope::Err(error) = service.call(
        Request::MeasureConfigure(MeasureConfigureParams {
            method: "rew".into(),
            rew_executable: Some("tools/roomeqwizard.exe".into()),
            ..Default::default()
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(error.code, ErrorCode::MeasurementInput);
    assert_eq!(error.params["field"], "rew_executable");

    let Response::MeasureConfigure(result) = call(
        &service,
        Request::MeasureConfigure(MeasureConfigureParams {
            method: "rew".into(),
            rew_executable: Some(String::new()),
            ..Default::default()
        }),
    ) else {
        panic!()
    };
    assert_eq!(result.rew_executable, None);
    call(&service, Request::RewLaunch);

    assert_eq!(*host.0.lock().unwrap(), vec![None, Some(exe), None]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rew_launch_classifies_reasons_and_returns_after_spawn() {
    let cases = [
        (
            secs_service::RewLaunchError::ExeNotFound("missing".into()),
            "exe_not_found",
        ),
        (
            secs_service::RewLaunchError::CannotLaunch("blocked".into()),
            "cannot_launch",
        ),
        (
            secs_service::RewLaunchError::LaunchFailed("failed".into()),
            "launch_failed",
        ),
    ];
    for (failure, reason) in cases {
        let service = Service::new(Deps {
            host: Arc::new(LaunchHost(Err(failure))),
            ..Default::default()
        });
        let Envelope::Err(error) = service.call(Request::RewLaunch, Arc::new(NoEvents)) else {
            panic!()
        };
        assert_eq!(error.code, ErrorCode::RewFailed);
        assert_eq!(error.params["reason"], reason);
    }
    let service = Service::new(Deps {
        host: Arc::new(LaunchHost(Ok(()))),
        ..Default::default()
    });
    let started = Instant::now();
    let Response::RewLaunch(status) = call(&service, Request::RewLaunch) else {
        panic!()
    };
    assert!(started.elapsed() < Duration::from_millis(100));
    assert_eq!(
        status,
        RewStatusResult {
            running: true,
            reachable: false,
            pro: None,
        }
    );
}

#[test]
fn unparsable_mic_calibration_setting_names_itself_td268() {
    let root = root("bad-mic-calibration-setting");
    let path = root.join("room.secs");
    let mut project = secs_project::Project::from_preset("2.0").unwrap();
    project.extra.push((
        "devices".into(),
        secs_project::PyJson::Object(vec![(
            "mic_calibration".into(),
            secs_project::PyJson::Str("not a calibration".into()),
        )]),
    ));
    fs::write(&path, project.save_text()).unwrap();
    let service = Service::new(Deps::default());
    call(
        &service,
        Request::ProjectOpen(ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }),
    );

    let Envelope::Err(error) = service.call(Request::MeasureConfigGet, Arc::new(NoEvents)) else {
        panic!("invalid microphone calibration must fail measure_config_get")
    };
    assert_eq!(error.code, ErrorCode::MeasurementInput);
    assert_eq!(
        error.params.get("field").map(String::as_str),
        Some("mic_calibration")
    );

    let Envelope::Err(error) = service.call(Request::MicCalibrationGet, Arc::new(NoEvents)) else {
        panic!("mic_calibration_get must retain its typed parse error")
    };
    assert_eq!(error.code, ErrorCode::MeasurementInput);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("mic_calibration")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn measure_config_get_reads_stored_settings_without_dirtying_the_project() {
    let root = root("config-get");
    let path = root.join("room.secs");
    let service = Service::new(Deps::default());
    assert!(matches!(
        service.call(Request::MeasureConfigGet, Arc::new(NoEvents)),
        Envelope::Err(IpcError {
            code: ErrorCode::ProjectNotOpen,
            ..
        })
    ));
    new(&service, &path);
    let Response::MeasureConfigGet(initial) = call(&service, Request::MeasureConfigGet) else {
        panic!()
    };
    assert_eq!(initial.method, "none");
    let configured = configure(&service, "mock");
    call(
        &service,
        Request::ProjectSave(ProjectSaveParams { path: None }),
    );
    let Response::MeasureConfigGet(stored) = call(&service, Request::MeasureConfigGet) else {
        panic!()
    };
    assert_eq!(stored, configured);
    let Response::ProjectInfo(info) = call(&service, Request::ProjectInfo) else {
        panic!()
    };
    assert!(!info.dirty);

    let mut old = secs_project::Project::load_text(&fs::read_to_string(&path).unwrap()).unwrap();
    old.extra.retain(|(key, _)| key != "devices");
    fs::write(&path, old.save_text()).unwrap();
    call(
        &service,
        Request::ProjectOpen(ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }),
    );
    let Response::MeasureConfigGet(compatibility) = call(&service, Request::MeasureConfigGet)
    else {
        panic!()
    };
    assert_eq!(compatibility.method, "native");
    fs::remove_dir_all(root).unwrap();
}

/// Holds REW's answer to one request until the test releases it.
#[derive(Debug, Default)]
struct Gate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}
impl Gate {
    fn arrive_and_wait(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut state = self.state.lock().unwrap();
        state.0 = true;
        self.changed.notify_all();
        while !state.1 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            state = self.changed.wait_timeout(state, remaining).unwrap().0;
        }
    }
    fn wait_arrived(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().unwrap();
        while !state.0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            state = self.changed.wait_timeout(state, remaining).unwrap().0;
        }
        true
    }
    fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}

#[derive(Debug)]
struct ConfigureRewTransport {
    calls: Arc<Mutex<Vec<secs_rew::Request>>>,
    gate: Arc<Gate>,
}
impl secs_rew::Transport for ConfigureRewTransport {
    fn request(
        &self,
        request: &secs_rew::Request,
    ) -> Result<secs_rew::Response, secs_rew::RewError> {
        self.calls.lock().unwrap().push(request.clone());
        if request.path == "/audio/java/output-devices" {
            self.gate.arrive_and_wait();
        }
        let body = match request.path.as_str() {
            "/audio/driver" => json!({"driver":"Java"}),
            "/audio/java/output-devices" => json!(["Selected Speakers"]),
            "/audio/java/output-channels" => json!(["L", "R"]),
            "/audio/java/output-device" => json!("Old Speakers"),
            _ => json!({"message":"ok"}),
        };
        Ok(secs_rew::Response::ok(body))
    }
}

#[test]
fn measure_configure_opens_selected_rew_device_after_project_update() {
    let root = root("configure-rew-device");
    let path = root.join("room.secs");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let gate = Arc::new(Gate::default());
    let service = Arc::new(Service::new(Deps {
        rew_transport: {
            let calls = calls.clone();
            let gate = gate.clone();
            Arc::new(move || {
                Box::new(ConfigureRewTransport {
                    calls: calls.clone(),
                    gate: gate.clone(),
                })
            })
        },
        ..Deps::default()
    }));
    new(&service, &path);
    let configuring = {
        let service = service.clone();
        std::thread::spawn(move || {
            call(
                &service,
                Request::MeasureConfigure(MeasureConfigureParams {
                    method: "rew".into(),
                    output_device: Some("Selected Speakers".into()),
                    ..Default::default()
                }),
            )
        })
    };
    assert!(
        gate.wait_arrived(Duration::from_secs(30)),
        "REW was not asked for its devices"
    );
    // REW is mid-request: another request must finish without waiting for it.
    let (sender, receiver) = std::sync::mpsc::channel();
    {
        let service = service.clone();
        std::thread::spawn(move || {
            let _ = sender.send(service.call(Request::ProjectInfo, Arc::new(NoEvents)));
        });
    }
    let info = receiver.recv_timeout(Duration::from_secs(5));
    gate.release();
    let Response::MeasureConfigure(result) = configuring.join().unwrap() else {
        panic!()
    };
    let Ok(Envelope::Ok(info)) = info else {
        panic!("project_info waited for the REW device request: {info:?}");
    };
    let Response::ProjectInfo(info) = *info else {
        panic!()
    };
    assert!(info.dirty, "the project is updated before REW is called");
    assert_eq!(result.output_device, "Selected Speakers");
    let calls = calls.lock().unwrap();
    assert!(calls.iter().any(|request| {
        request.method == "POST"
            && request.path == "/audio/java/output-device"
            && request.body == Some(json!({"device":"Selected Speakers"}))
            && request.timeout == Duration::from_secs(30)
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn configure_validates_fields_and_host_stub_reports_its_error() {
    let root = root("config");
    let path = root.join("room.secs");
    let service = Service::new(Deps::default());
    new(&service, &path);
    let envelope = service.call_value(
        json!({"method":"measure_configure","params":{"method":"mock"}}),
        Arc::new(NoEvents),
    );
    assert!(matches!(envelope, Envelope::Ok(_)));
    for (field, value) in [
        ("sweep_length", json!(2)),
        ("input_channel", json!(0)),
        ("share_mode", json!("bad")),
        ("timing", json!("bad")),
    ] {
        let mut params = json!({"method":"native"});
        params[field] = value;
        let Envelope::Err(error) = service.call_value(
            json!({"method":"measure_configure","params":params}),
            Arc::new(NoEvents),
        ) else {
            panic!()
        };
        assert_eq!(error.code, ErrorCode::MeasurementInput);
        assert_eq!(error.params["field"], field);
    }
    let Envelope::Err(error) = service.call(Request::RewLaunch, Arc::new(NoEvents)) else {
        panic!()
    };
    assert_eq!(error.code, ErrorCode::RewFailed);
    assert_eq!(error.params["reason"], "cannot_launch");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sample_load_positions_stop_at_the_largest_layout() {
    let root = root("sample-load-bound");
    let path = root.join("bound.secs");
    let service = Service::new(Deps::default());
    new(&service, &path);
    for positions in [18, u32::MAX] {
        let started = std::time::Instant::now();
        let error = service.call(
            Request::SampleLoad(SampleLoadParams {
                positions: Some(positions),
            }),
            Arc::new(NoEvents),
        );
        let Envelope::Err(error) = error else {
            panic!("{positions} positions must be refused");
        };
        assert_eq!(error.code, ErrorCode::MeasurementInput, "{positions}");
        assert_eq!(
            error.params.get("field").map(String::as_str),
            Some("positions")
        );
        assert!(started.elapsed() < Duration::from_secs(1), "{positions}");
    }
    let JobResult::Measurement(info) = sample_load(&service, Some(17), Arc::new(Events::default()))
    else {
        panic!("17 positions must load");
    };
    assert_eq!(info.positions, (1..=17).collect::<Vec<i64>>());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rew_driver_and_devices_stay_while_a_measurement_runs() {
    let root = root("rew-busy");
    let path = root.join("room.secs");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let gate = Arc::new(Gate::default());
    gate.release();
    let service = Service::new(Deps {
        rew_transport: {
            let calls = calls.clone();
            Arc::new(move || {
                Box::new(ConfigureRewTransport {
                    calls: calls.clone(),
                    gate: gate.clone(),
                })
            })
        },
        ..Deps::default()
    });
    new(&service, &path);
    set_start_delay(&path, 60);
    call(
        &service,
        Request::ProjectOpen(ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }),
    );
    configure(&service, "mock");
    let set_driver = || {
        Request::RewDriverSet(RewDriverSetParams {
            driver: "Java".into(),
        })
    };
    let switch_device = || {
        Request::MeasureConfigure(MeasureConfigureParams {
            method: "rew".into(),
            output_device: Some("Selected Speakers".into()),
            ..Default::default()
        })
    };

    // A measurement waits in its start countdown.
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    for request in [set_driver(), switch_device()] {
        let method = request.method();
        let Envelope::Err(error) = service.call(request, Arc::new(NoEvents)) else {
            panic!("{method} must wait for the measurement");
        };
        assert_eq!(error.code, ErrorCode::JobBusy, "{method}");
        assert_eq!(
            error.params.get("kind").map(String::as_str),
            Some("Measure"),
            "{method}"
        );
    }
    assert!(calls.lock().unwrap().is_empty(), "REW was called");
    let Response::MeasureConfigGet(unchanged) = call(&service, Request::MeasureConfigGet) else {
        panic!()
    };
    assert_eq!(unchanged.method, "mock");
    call(&service, Request::JobCancel(JobCancelParams { job }));
    events.wait(job, Duration::from_secs(5));

    // The level monitor does not hold REW back: the screen restarts it.
    let monitor_events = Arc::new(Events::default());
    let Response::LevelMonitorStart(monitor) = ok(
        &service,
        Request::LevelMonitorStart(LevelMonitorStartParams::default()),
        monitor_events.clone(),
    ) else {
        panic!()
    };
    assert!(matches!(
        call(&service, set_driver()),
        Response::RewDriverSet(_)
    ));
    let Response::MeasureConfigure(switched) = call(&service, switch_device()) else {
        panic!()
    };
    assert_eq!(switched.output_device, "Selected Speakers");
    call(&service, Request::LevelMonitorStop);
    monitor_events.wait(monitor.job, Duration::from_secs(5));
    fs::remove_dir_all(root).unwrap();
}
