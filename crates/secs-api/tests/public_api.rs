#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "engine")]
#[path = "../../secs-service/examples/support/case_project.rs"]
mod case_project;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "secs-api-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(feature = "engine")]
#[test]
fn engine_bootstrap_and_invalid_json() {
    use sound_equalization_and_correction_system_rust_api::{
        Engine, ErrorCode, Request, Response, Secs, types,
    };

    let engine = Engine::new();
    let Response::Bootstrap(info) = engine.call(Request::Bootstrap).unwrap() else {
        panic!("bootstrap has one response variant")
    };
    assert_eq!(info.version, env!("CARGO_PKG_VERSION"));

    let types::ipc::Envelope::Err(error) =
        engine.call_json(serde_json::json!({"method": "not_a_method"}))
    else {
        panic!("an unknown method must be an error envelope")
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
}

#[cfg(feature = "engine")]
#[test]
fn engine_calibrate_passes_the_mimo_flag() {
    use sound_equalization_and_correction_system_rust_api::types::compute::ComputePreference;
    use sound_equalization_and_correction_system_rust_api::types::ipc::CalibrateParams;
    use sound_equalization_and_correction_system_rust_api::{
        EngineBuilder, Error, ErrorCode, Request, Secs,
    };

    let engine = EngineBuilder::default()
        .compute_preference(ComputePreference::Gpu)
        .build();
    let error = engine
        .call(Request::Calibrate(CalibrateParams {
            mimo: true,
            ..Default::default()
        }))
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Api(error) if error.code == ErrorCode::ComputeUnavailable
    ));
}

#[cfg(feature = "engine")]
#[test]
fn engine_oracle_flow_and_ordered_events() {
    use std::fs;
    use std::time::Duration;

    use sound_equalization_and_correction_system_rust_api::types::ipc::{
        CalibrateParams, ExportKind, ExportParams, JobEvent, JobId, JobStatus, ProjectPathParams,
    };
    use sound_equalization_and_correction_system_rust_api::{Engine, Request, Response, Secs};

    let root = temp_root("engine-oracle");
    fs::create_dir_all(&root).unwrap();
    let (project, settings) = case_project::write(&root, "dolby");
    let engine = Engine::new();
    assert!(matches!(
        engine
            .call(Request::ProjectOpen(ProjectPathParams {
                path: project.to_string_lossy().into_owned(),
            }))
            .unwrap(),
        Response::ProjectOpen(_)
    ));

    let events = engine.subscribe().unwrap();
    let Response::Calibrate(started) = engine
        .call(Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            ..Default::default()
        }))
        .unwrap()
    else {
        panic!("calibrate has one response variant")
    };
    let finished = engine
        .wait_job(started.job, Duration::from_secs(120))
        .unwrap();
    assert!(matches!(
        finished,
        JobEvent::Finished {
            job,
            status: JobStatus::Succeeded,
            ..
        } if job == started.job
    ));

    let observed: Vec<_> = events
        .receiver
        .try_iter()
        .filter_map(|event| match event {
            sound_equalization_and_correction_system_rust_api::Event::Job(event) => Some(event),
            _ => None,
        })
        .filter(|event| event_job(event) == started.job)
        .collect();
    assert!(matches!(
        observed.first(),
        Some(JobEvent::Started { job, .. }) if *job == started.job
    ));
    assert!(matches!(
        observed.last(),
        Some(JobEvent::Finished {
            job,
            status: JobStatus::Succeeded,
            ..
        }) if *job == started.job
    ));
    assert_eq!(
        observed
            .iter()
            .filter(|event| matches!(event, JobEvent::Started { .. }))
            .count(),
        1
    );
    assert_eq!(
        observed
            .iter()
            .filter(|event| matches!(event, JobEvent::Finished { .. }))
            .count(),
        1
    );

    let Response::Results(results) = engine.call(Request::Results).unwrap() else {
        panic!("results has one response variant")
    };
    assert!(results.calibration.is_some());

    let export_root = root.join("camilladsp");
    let Response::Export(export) = engine
        .call(Request::Export(ExportParams {
            kind: ExportKind::CamillaDsp,
            folder: export_root.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }))
        .unwrap()
    else {
        panic!("export has one response variant")
    };
    assert!(matches!(
        engine
            .wait_job(export.job, Duration::from_secs(120))
            .unwrap(),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    assert!(export_root.exists());
    fs::remove_dir_all(root).unwrap();

    fn event_job(event: &JobEvent) -> JobId {
        match event {
            JobEvent::Started { job, .. }
            | JobEvent::Progress { job, .. }
            | JobEvent::Log { job, .. }
            | JobEvent::Finished { job, .. }
            | JobEvent::Meter { job, .. }
            | JobEvent::MeasureStatus { job, .. } => *job,
        }
    }
}

#[cfg(feature = "client")]
#[test]
fn client_status_call_authentication_and_changed_event() {
    use std::fs;
    use std::sync::Arc;

    use secs_local_api::{EventHub, ServerConfig};
    use secs_service::{Deps, Service};
    use sound_equalization_and_correction_system_rust_api::types::ipc::ProjectNewParams;
    use sound_equalization_and_correction_system_rust_api::types::local_api::API_VERSION;
    use sound_equalization_and_correction_system_rust_api::{
        Client, Error, Request, Response, Secs,
    };

    const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let root = temp_root("client");
    fs::create_dir_all(&root).unwrap();
    let token_path = root.join("local-api-token");
    fs::write(&token_path, TOKEN).unwrap();

    let hub = EventHub::default();
    let service = Arc::new(Service::new(Deps {
        app_version: env!("CARGO_PKG_VERSION").into(),
        observer: Arc::new(hub.clone()),
        ..Deps::default()
    }));
    let server = secs_local_api::start(ServerConfig {
        port: 0,
        token: TOKEN.into(),
        service: service.clone(),
        hub,
        on_change: Arc::new(|_| {}),
    })
    .unwrap();
    let client = Client::from_token_file(server.port(), &token_path).unwrap();

    let status = client.status().unwrap();
    assert_eq!(status.api, API_VERSION);
    assert_eq!(status.app_version, env!("CARGO_PKG_VERSION"));

    let response = client.call(Request::Bootstrap).unwrap();
    #[cfg(feature = "engine")]
    let direct = {
        use sound_equalization_and_correction_system_rust_api::Engine;
        Engine::new().call(Request::Bootstrap).unwrap()
    };
    #[cfg(not(feature = "engine"))]
    let direct = {
        let sound_equalization_and_correction_system_rust_api::types::ipc::Envelope::Ok(response) =
            service.call(Request::Bootstrap, Arc::new(secs_service::NoEvents))
        else {
            panic!("direct bootstrap must succeed")
        };
        *response
    };
    assert_eq!(response, direct);
    assert!(matches!(response, Response::Bootstrap(_)));

    let wrong = Client::new(server.port(), "wrong");
    assert!(matches!(
        wrong.status(),
        Err(Error::Transport(message)) if message.contains("401")
    ));

    let events = client.subscribe().unwrap();
    let project = root.join("client.secs");
    client
        .call(Request::ProjectNew(ProjectNewParams {
            path: project.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: true,
            tutorial: false,
        }))
        .unwrap();
    assert_eq!(
        events.receiver.recv().unwrap(),
        sound_equalization_and_correction_system_rust_api::Event::Changed {
            method: "project_new".into()
        }
    );

    // A reader blocked on a quiet stream returns once another thread closes it.
    let closer = events.closer.clone();
    let (finished, waiting) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let _ = events.receiver.recv();
        let _ = finished.send(());
    });
    std::thread::sleep(std::time::Duration::from_millis(200));
    closer.close();
    waiting
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("a closed event stream must stop blocking its reader");
    reader.join().unwrap();

    // A server that accepts the connection but never answers the upgrade.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let silent_port = silent.local_addr().unwrap().port();
    let started = std::time::Instant::now();
    assert!(matches!(
        Client::new(silent_port, TOKEN).subscribe(),
        Err(Error::Transport(_))
    ));
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
    drop(silent);

    server.stop();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn wire_error_has_one_shape_for_every_error_kind_td160() {
    use sound_equalization_and_correction_system_rust_api::{
        Error, ErrorCode, IpcError, WireError,
    };
    let mut api = IpcError::new(
        ErrorCode::ProjectNotOpen,
        "no project
open",
    )
    .with_param("field", "x");
    api.retryable = true;
    for (error, expected) in [
        (
            Error::Api(api),
            serde_json::json!({"code":"project_not_open","message":"project_not_open: no project open","detail":"no project
open","params":{"field":"x"},"retryable":true}),
        ),
        (
            Error::Transport("offline".into()),
            serde_json::json!({"code":"transport_error","message":"transport_error: offline","detail":"offline","params":{},"retryable":false}),
        ),
        (
            Error::Protocol("bad frame".into()),
            serde_json::json!({"code":"protocol_error","message":"protocol_error: bad frame","detail":"bad frame","params":{},"retryable":false}),
        ),
    ] {
        assert_eq!(
            serde_json::to_value(WireError::from(&error)).unwrap(),
            expected
        );
        assert_eq!(error.to_string(), expected["message"].as_str().unwrap());
    }
}

#[cfg(feature = "engine")]
#[test]
fn dropping_the_engine_stops_its_jobs_within_three_seconds_td206() {
    use secs_service::EventSink;
    use sound_equalization_and_correction_system_rust_api::types::ipc::{
        CalibrateParams, JobStatus, ProjectPathParams,
    };
    use sound_equalization_and_correction_system_rust_api::{
        EngineBuilder, JobEvent, Request, Secs,
    };
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    #[derive(Debug, Default)]
    struct SlowProgress {
        active: AtomicBool,
        progressed: AtomicBool,
        cancelled: AtomicBool,
    }
    impl EventSink for SlowProgress {
        fn send(&self, event: JobEvent) {
            match event {
                JobEvent::Started { .. } => self.active.store(true, Ordering::Release),
                JobEvent::Progress { .. } => {
                    self.progressed.store(true, Ordering::Release);
                    // Keep the real calibration busy long enough to observe missing cancellation.
                    std::thread::sleep(Duration::from_millis(100));
                }
                JobEvent::Finished { status, .. } => {
                    self.cancelled
                        .store(status == JobStatus::Cancelled, Ordering::Release);
                    self.active.store(false, Ordering::Release);
                }
                _ => {}
            }
        }
    }

    let root = temp_root("drop-cancels");
    std::fs::create_dir_all(&root).unwrap();
    let (project, settings) = case_project::write(&root, "dolby");
    let observer = Arc::new(SlowProgress::default());
    let engine = EngineBuilder::default().observer(observer.clone()).build();
    engine
        .call(Request::ProjectOpen(ProjectPathParams {
            path: project.to_string_lossy().into_owned(),
        }))
        .unwrap();
    engine
        .call(Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            ..Default::default()
        }))
        .unwrap();
    let ready = Instant::now();
    while !observer.progressed.load(Ordering::Acquire) && ready.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        observer.progressed.load(Ordering::Acquire),
        "job must report progress before drop"
    );
    let started = Instant::now();
    drop(engine);
    while observer.active.load(Ordering::Acquire) && started.elapsed() < Duration::from_millis(3500)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !observer.active.load(Ordering::Acquire),
        "job remained active 3.5 seconds after drop"
    );
    assert!(started.elapsed() < Duration::from_millis(3500));
    assert!(
        observer.cancelled.load(Ordering::Acquire),
        "drop must cancel rather than let the job finish normally"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn error_display_prints_snake_case_codes() {
    use sound_equalization_and_correction_system_rust_api::{Error, ErrorCode, IpcError, JobId};

    let api = Error::Api(IpcError::new(ErrorCode::ProjectNotOpen, "no project"));
    assert_eq!(api.to_string(), "project_not_open: no project");
    assert_eq!(
        Error::Timeout { job: JobId(7) }.to_string(),
        "timeout: job 7"
    );
    assert_eq!(
        Error::JobNotFound { job: JobId(7) }.to_string(),
        "job_not_found: job 7"
    );
    assert_eq!(
        serde_json::to_value(
            sound_equalization_and_correction_system_rust_api::Event::Lagged { missed: 5 }
        )
        .unwrap(),
        serde_json::json!({"kind":"lagged","event":{"missed":5}})
    );
    assert_eq!(
        serde_json::to_value(sound_equalization_and_correction_system_rust_api::Event::Job(
            sound_equalization_and_correction_system_rust_api::JobEvent::Started {
                job: JobId(7),
                kind: sound_equalization_and_correction_system_rust_api::types::ipc::JobKind::Export,
            },
        ))
        .unwrap(),
        serde_json::json!({"kind":"job","event":{"event":"started","job":7,"kind":"export"}})
    );
}

#[cfg(all(feature = "engine", feature = "client"))]
fn local_client(
    name: &str,
) -> (
    sound_equalization_and_correction_system_rust_api::Client,
    secs_local_api::ServerHandle,
    secs_local_api::EventHub,
    PathBuf,
) {
    use secs_local_api::{EventHub, ServerConfig};
    use secs_service::{Deps, Service};
    use sound_equalization_and_correction_system_rust_api::Client;
    use std::sync::Arc;

    const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let root = temp_root(name);
    std::fs::create_dir_all(&root).unwrap();
    let hub = EventHub::default();
    let service = Arc::new(Service::new(Deps {
        observer: Arc::new(hub.clone()),
        app_data_path: root.clone(),
        ..Deps::default()
    }));
    let server = secs_local_api::start(ServerConfig {
        port: 0,
        token: TOKEN.into(),
        service,
        hub: hub.clone(),
        on_change: Arc::new(|_| {}),
    })
    .unwrap();
    (Client::new(server.port(), TOKEN), server, hub, root)
}

#[cfg(feature = "engine")]
fn start_sample_job(
    api: &dyn sound_equalization_and_correction_system_rust_api::Secs,
    root: &std::path::Path,
) -> sound_equalization_and_correction_system_rust_api::JobId {
    use sound_equalization_and_correction_system_rust_api::types::ipc::{
        ProjectNewParams, SampleLoadParams,
    };
    use sound_equalization_and_correction_system_rust_api::{Request, Response};

    let project = root.join("events.secs");
    api.call(Request::ProjectNew(ProjectNewParams {
        path: project.to_string_lossy().into_owned(),
        preset: "2.0".into(),
        sub_count: 0,
        sample: true,
        tutorial: false,
    }))
    .unwrap();
    let Response::SampleLoad(started) = api
        .call(Request::SampleLoad(SampleLoadParams { positions: Some(2) }))
        .unwrap()
    else {
        panic!("sample_load has one response variant")
    };
    started.job
}

#[cfg(all(feature = "engine", feature = "client"))]
fn assert_job_event_order(
    api: &dyn sound_equalization_and_correction_system_rust_api::Secs,
    root: &std::path::Path,
) {
    use sound_equalization_and_correction_system_rust_api::types::ipc::JobStatus;
    use sound_equalization_and_correction_system_rust_api::{Event, JobEvent};
    use std::time::Duration;

    let subscription = api.subscribe().unwrap();
    let job = start_sample_job(api, root);
    let terminal = api.wait_job(job, Duration::from_secs(120)).unwrap();
    assert!(matches!(
        terminal,
        JobEvent::Finished { job: found, status: JobStatus::Succeeded, .. } if found == job
    ));
    let mut observed = Vec::new();
    while let Ok(event) = subscription.receiver.recv_timeout(Duration::from_secs(5)) {
        if let Event::Job(event) = event {
            let finished = matches!(event, JobEvent::Finished { job: found, .. } if found == job);
            if matches!(
                event,
                JobEvent::Started { job: found, .. }
                    | JobEvent::Progress { job: found, .. }
                    | JobEvent::Log { job: found, .. }
                    | JobEvent::Finished { job: found, .. }
                    | JobEvent::Meter { job: found, .. }
                    | JobEvent::MeasureStatus { job: found, .. }
                    if found == job
            ) {
                observed.push(event);
            }
            if finished {
                break;
            }
        }
    }
    assert!(matches!(observed.first(), Some(JobEvent::Started { .. })));
    assert!(matches!(observed.last(), Some(JobEvent::Finished { .. })));
}

#[cfg(all(feature = "engine", feature = "client"))]
#[test]
fn subscribe_delivers_job_events_in_order_on_both_adapters() {
    use sound_equalization_and_correction_system_rust_api::EngineBuilder;

    let engine_root = temp_root("ordered-engine");
    std::fs::create_dir_all(&engine_root).unwrap();
    let engine = EngineBuilder::default().build();
    assert_job_event_order(&engine, &engine_root);

    let (client, server, _hub, client_root) = local_client("ordered-client");
    assert_job_event_order(&client, &client_root);
    server.stop();
    let _ = std::fs::remove_dir_all(engine_root);
    let _ = std::fs::remove_dir_all(client_root);
}

#[cfg(all(feature = "engine", feature = "client"))]
#[test]
fn slow_engine_subscriber_counts_dropped_events_and_slow_client_receives_lagged() {
    use secs_service::EventSink;
    use sound_equalization_and_correction_system_rust_api::types::ipc::{
        JobEvent, JobId, JobKind, JobStatus,
    };
    use sound_equalization_and_correction_system_rust_api::{Engine, Event, Secs};
    use std::time::Duration;

    // The engine's direct observer test fills its 1024-slot queue with Started events.
    // Keep the public count visible here as part of the same contract.
    let engine = Engine::new();
    let engine_subscription = Secs::subscribe(&engine).unwrap();
    assert_eq!(engine_subscription.dropped.get(), 0);

    let (client, server, hub, root) = local_client("lagged-client");
    let subscription = client.subscribe().unwrap();
    for id in 0..1500 {
        hub.send(JobEvent::Started {
            job: JobId(id),
            kind: JobKind::Export,
        });
        if subscription.dropped.get() > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(subscription.dropped.get() >= 1, "client queue did not fill");
    hub.send(JobEvent::Finished {
        job: JobId(1023),
        status: JobStatus::Succeeded,
        result: None,
        error: None,
    });
    std::thread::sleep(Duration::from_millis(100));
    let large_stage = "x".repeat(128 * 1024);
    for _ in 0..1024 {
        hub.send(JobEvent::Progress {
            job: JobId(1),
            stage: large_stage.clone(),
            done: 0,
            total: 1,
        });
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut lagged = None;
    while std::time::Instant::now() < deadline {
        match subscription
            .receiver
            .recv_timeout(Duration::from_millis(500))
        {
            Ok(Event::Lagged { missed }) => {
                lagged = Some(missed);
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert!(lagged.is_some_and(|missed| missed >= 5));
    server.stop();
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(all(feature = "engine", feature = "client"))]
fn assert_wait_errors(
    api: &dyn sound_equalization_and_correction_system_rust_api::Secs,
    root: &std::path::Path,
) {
    use sound_equalization_and_correction_system_rust_api::types::ipc::{
        CalibrateParams, JobCancelParams,
    };
    use sound_equalization_and_correction_system_rust_api::{
        Error, JobEvent, JobId, Request, Response,
    };
    use std::time::Duration;

    let finished = start_sample_job(api, root);
    assert!(matches!(
        api.wait_job(finished, Duration::from_secs(120)).unwrap(),
        JobEvent::Finished { job, .. } if job == finished
    ));
    assert!(matches!(
        api.wait_job(JobId(u64::MAX), Duration::from_millis(100)),
        Err(Error::JobNotFound { job }) if job == JobId(u64::MAX)
    ));
    let Response::Calibrate(started) = api
        .call(Request::Calibrate(CalibrateParams::default()))
        .unwrap()
    else {
        panic!("calibrate has one response variant")
    };
    assert!(matches!(
        api.wait_job(started.job, Duration::from_millis(100)),
        Err(Error::Timeout { job }) if job == started.job
    ));
    let _ = api.call(Request::JobCancel(JobCancelParams { job: started.job }));
}

#[cfg(all(feature = "engine", feature = "client"))]
#[test]
fn wait_job_answers_finished_unknown_and_timeout_the_same_way() {
    use sound_equalization_and_correction_system_rust_api::Engine;

    let engine_root = temp_root("wait-engine");
    std::fs::create_dir_all(&engine_root).unwrap();
    let engine = Engine::new();
    assert_wait_errors(&engine, &engine_root);

    let (client, server, _hub, client_root) = local_client("wait-client");
    assert_wait_errors(&client, &client_root);
    server.stop();
    let _ = std::fs::remove_dir_all(engine_root);
    let _ = std::fs::remove_dir_all(client_root);
}

#[cfg(all(feature = "engine", feature = "client"))]
fn assert_closer(api: &dyn sound_equalization_and_correction_system_rust_api::Secs) {
    use std::time::{Duration, Instant};

    let subscription = api.subscribe().unwrap();
    let closer = subscription.closer.clone();
    let reader = std::thread::spawn(move || subscription.receiver.recv());
    std::thread::sleep(Duration::from_millis(20));
    let started = Instant::now();
    closer.close();
    let result = reader.join().unwrap();
    assert!(matches!(
        result,
        Err(_) | Ok(sound_equalization_and_correction_system_rust_api::Event::Closed { .. })
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[cfg(all(feature = "engine", feature = "client"))]
#[test]
fn closer_ends_a_blocked_recv_within_one_second() {
    use sound_equalization_and_correction_system_rust_api::Engine;
    let engine = Engine::new();
    assert_closer(&engine);
    let (client, server, _hub, root) = local_client("closer-client");
    assert_closer(&client);
    server.stop();
    let _ = std::fs::remove_dir_all(root);
}
