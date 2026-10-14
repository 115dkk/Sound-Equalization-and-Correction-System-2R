#![forbid(unsafe_code)]

use secs_service::{Deps, EventSink, NoEvents, Service, measure};
use secs_types::ipc::*;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
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
fn calibration(service: &Service) -> CalibrationSummary {
    let events = Arc::new(Events::default());
    let Response::Calibrate(job) = ok(
        service,
        Request::Calibrate(CalibrateParams::default()),
        events.clone(),
    ) else {
        panic!()
    };
    let JobResult::Calibration(result) = succeeded(&events, job.job) else {
        panic!()
    };
    result.calibration.unwrap()
}

#[test]
fn mock_flow_level_apply_and_calibration() {
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
    let Response::LevelCheck(job) = ok(
        &service,
        Request::LevelCheck(LevelCheckParams { role: "L".into() }),
        events.clone(),
    ) else {
        panic!()
    };
    let JobResult::Level(level) = succeeded(&events, job.job) else {
        panic!()
    };
    assert!(level.rms_dbfs.is_finite());
    assert!(level.peak_dbfs.is_finite());
    assert_eq!(level.spl_db, None);
    let Response::LevelApply(applied) = call(
        &service,
        Request::LevelApply(LevelApplyParams {
            level: level.clone(),
        }),
    ) else {
        panic!()
    };
    assert!((applied.sweep_level_dbfs.unwrap() - level.recommended_sweep_dbfs).abs() < 1e-10);
    assert_eq!(
        call(&service, Request::LevelApply(LevelApplyParams { level })),
        Response::LevelApply(applied)
    );
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    let values = events.values.lock().unwrap();
    assert_eq!(values.len(), 4, "{values:?}");
    assert!(matches!(values[0], JobEvent::Started { .. }));
    for (i, role) in ["L", "R"].iter().enumerate() {
        assert!(
            matches!(&values[i+1],JobEvent::Progress{stage,done,total,..} if stage==role && *done==i as u64+1 && *total==2)
        );
    }
    assert!(path.is_file());
    assert!(path.with_extension("irs").join("L_P1.wav").is_file());
    assert!(!calibration(&service).stale);
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
    let plan = measure::plan_for(&project, &Default::default());
    let (room, _) = measure::sample::room(&plan, 1).unwrap();
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
                measure::MockMeasurer::with_rooms(rooms()).with_delay(delay)
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
        mock_measurer: Arc::new(|| measure::MockMeasurer::with_rooms(rooms())),
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
fn rew_script() -> Vec<secs_rew::RecordedExchange> {
    let mut script = vec![get(
        "/version",
        json!({"message":"5.40 Beta 124 API 0.9.4"}),
        2,
    )];
    for (i, role) in ["L", "R"].iter().enumerate() {
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
        if i == 0 {
            script.extend([
                get("/audio/driver", json!({"driver":"Java"}), 5),
                get("/audio/java/outputs", json!(["L", "R"]), 5),
            ]);
        }
        script.extend([
            post("/audio/java/output",json!({"output":role})),
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
    }
    script
}
#[test]
fn rew_recorded_transport_records_each_role_and_warning() {
    let root = root("rew");
    let path = root.join("room.secs");
    let transport = secs_rew::RecordedTransport::new(rew_script());
    let copy = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(copy.clone())),
        ..Default::default()
    });
    new(&service, &path);
    configure(&service, "rew");
    let Response::MeasureMethods(methods) = call(&service, Request::MeasureMethods) else {
        panic!()
    };
    assert!(methods.rew.reachable && methods.rew.pro);
    let events = Arc::new(Events::default());
    let job = measure(&service, 1, events.clone());
    succeeded(&events, job);
    transport.assert_finished();
    let mut store = secs_project::MeasurementStore::open(&path).unwrap();
    for role in ["L", "R"] {
        assert_eq!(
            store.ir_of(role, 1).unwrap(),
            (vec![1.0, -0.25, 0.125], 48000)
        );
        assert_eq!(store.t0_of(role, 1), -0.125);
    }
    let values = events.values.lock().unwrap();
    assert_eq!(values.len(), 6);
    assert!(
        matches!(&values[2],JobEvent::Log{code,params,..} if code=="measurement_low_snr" && params["role"]=="L")
    );
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
        .filter(|(_, e)| e.request.path == "/application/blocking")
        .nth(1)
        .unwrap()
        .0;
    script.truncate(second);
    let transport = secs_rew::RecordedTransport::new(script);
    let copy = transport.clone();
    let service = Arc::new(Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(copy.clone())),
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
    assert!(matches!(
        service.call(Request::RewLaunch, Arc::new(NoEvents)),
        Envelope::Err(IpcError {
            code: ErrorCode::NotImplemented,
            ..
        })
    ));
    fs::remove_dir_all(root).unwrap();
}
