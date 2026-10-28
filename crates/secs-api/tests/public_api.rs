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

    let events = engine.subscribe();
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
        .try_iter()
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
    use sound_equalization_and_correction_system_rust_api::types::local_api::{
        API_VERSION, ServerEvent,
    };
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

    let mut events = client.events().unwrap();
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
        events.next().unwrap().unwrap(),
        ServerEvent::Changed {
            method: "project_new".into()
        }
    );

    server.stop();
    fs::remove_dir_all(root).unwrap();
}
