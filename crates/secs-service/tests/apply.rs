#![forbid(unsafe_code)]

#[path = "../examples/support/case_project.rs"]
mod case_project;

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use secs_service::{ApplyAdapter, Deps, EventSink, LocalTime, NoEvents, Service};
use secs_types::ipc::*;
use tungstenite::Message;

static TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct NativeApply;

impl ApplyAdapter for NativeApply {
    fn camilla_status(&self, address: &str, timeout: Duration) -> Result<CamillaStatus, IpcError> {
        let transport =
            secs_apply::camilla::WsTransport::connect(address, timeout).map_err(camilla_error)?;
        let status = secs_apply::camilla::Camilla::new(transport)
            .status()
            .map_err(camilla_error)?;
        Ok(CamillaStatus {
            version: status.version,
            state: status.state,
            stop_reason: status.stop_reason,
            title: status.title,
            description: status.description,
            config_path: status.config_path,
            capture_rate: status.capture_rate,
            clipped_samples: status.clipped_samples,
            processing_load_pct: status.processing_load_pct,
            playback_peak_db: status.playback_peak_db,
        })
    }

    fn camilla_apply(
        &self,
        address: &str,
        config_path: &Path,
        config_text: &str,
        timeout: Duration,
    ) -> Result<ApplyOutcome, IpcError> {
        let transport =
            secs_apply::camilla::WsTransport::connect(address, timeout).map_err(camilla_error)?;
        match secs_apply::camilla::Camilla::new(transport)
            .apply(&config_path.to_string_lossy(), config_text)
            .map_err(camilla_error)?
        {
            secs_apply::camilla::ApplyOutcome::Applied { .. } => Ok(ApplyOutcome::Applied),
            secs_apply::camilla::ApplyOutcome::RolledBack { error, .. } => {
                Ok(ApplyOutcome::RolledBack {
                    error: IpcError::new(ErrorCode::ApplyFailed, error),
                })
            }
        }
    }

    fn eqapo_include_state(&self, config_text: &str) -> IncludeState {
        match secs_apply::eqapo::include_state(config_text) {
            secs_apply::eqapo::IncludeState::Absent => IncludeState::Absent,
            secs_apply::eqapo::IncludeState::Present => IncludeState::Present,
            secs_apply::eqapo::IncludeState::Damaged => IncludeState::Damaged,
        }
    }

    fn eqapo_with_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        Ok(secs_apply::eqapo::with_include(config_text))
    }

    fn eqapo_without_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        Ok(secs_apply::eqapo::without_include(config_text))
    }
}

fn camilla_error(error: secs_apply::ApplyError) -> IpcError {
    let mut ipc = IpcError::new(ErrorCode::ApplyFailed, error.to_string());
    ipc.retryable = matches!(
        error,
        secs_apply::ApplyError::Connect { .. } | secs_apply::ApplyError::Transport(_)
    );
    ipc
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "secs-apply-service-{name}-{}-{}",
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn ok(service: &Service, request: Request) -> Response {
    match service.call(request, Arc::new(NoEvents)) {
        Envelope::Ok(response) => *response,
        Envelope::Err(error) => panic!("request failed: {error:?}"),
    }
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
    fn wait(&self, job: JobId) -> JobEvent {
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut values = lock(&self.values);
        loop {
            if let Some(event) = values
                .iter()
                .find(|event| matches!(event, JobEvent::Finished { job: id, .. } if *id == job))
            {
                return event.clone();
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "job {job:?} did not finish");
            values = self.changed.wait_timeout(values, remaining).unwrap().0;
        }
    }
}

fn started(service: &Service, request: Request, events: Arc<Events>) -> JobStarted {
    let response = match service.call(request, events) {
        Envelope::Ok(response) => *response,
        Envelope::Err(error) => panic!("job did not start: {error:?}"),
    };
    match response {
        Response::Calibrate(started)
        | Response::ApplyCamilla(started)
        | Response::ApplyEqapo(started) => started,
        other => panic!("unexpected response: {other:?}"),
    }
}

fn succeeded(event: JobEvent) -> JobResult {
    match event {
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            result: Some(result),
            ..
        } => *result,
        other => panic!("job did not succeed: {other:?}"),
    }
}

fn calibrated_service(root: &Path, mut deps: Deps) -> Service {
    deps.app_data_path = root.join("app-data");
    deps.apply = Arc::new(NativeApply);
    deps.clock = Arc::new(|| LocalTime {
        year: 2024,
        month: 1,
        day: 2,
        hour: 3,
        minute: 4,
        second: 5,
    });
    deps.unix_ms = Arc::new(|| 1_704_164_645_000);
    let (project, settings) = case_project::write(root, "room");
    let service = Service::new(deps);
    assert!(matches!(
        ok(
            &service,
            Request::ProjectOpen(ProjectPathParams {
                path: project.to_string_lossy().into_owned(),
            })
        ),
        Response::ProjectOpen(_)
    ));
    let events = Arc::new(Events::default());
    let calibration = started(
        &service,
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            ..Default::default()
        }),
        events.clone(),
    );
    succeeded(events.wait(calibration.job));
    service
}

fn set_apply(service: &Service, camilla: CamillaApply, eqapo: EqApoApply) {
    assert_eq!(
        ok(
            service,
            Request::ApplySettingsSet(ApplySettings {
                camilla: camilla.clone(),
                eqapo: eqapo.clone(),
            })
        ),
        Response::ApplySettingsSet(ApplySettings { camilla, eqapo })
    );
}

fn ws_server(replies: Vec<String>) -> (String, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut requests = Vec::new();
        for reply in replies {
            requests.push(socket.read().unwrap().to_text().unwrap().to_owned());
            socket.send(Message::Text(reply.into())).unwrap();
        }
        requests
    });
    (address, server)
}

fn reply(command: &str, value: serde_json::Value) -> String {
    serde_json::json!({command: {"result": "Ok", "value": value}}).to_string()
}

fn reply_ok(command: &str) -> String {
    serde_json::json!({command: {"result": "Ok"}}).to_string()
}

fn rejected(command: &str, message: &str) -> String {
    serde_json::json!({command: {"result": "Error", "value": message}}).to_string()
}

fn unsupported() -> String {
    serde_json::json!({"Invalid": {"error": "unknown variant"}}).to_string()
}

fn status_replies(title: Option<&str>, description: Option<&str>) -> Vec<String> {
    let optional = |command: &str, value: Option<&str>| {
        value.map_or_else(unsupported, |value| {
            reply(command, serde_json::json!(value))
        })
    };
    vec![
        reply("GetVersion", serde_json::json!("4.1.3")),
        reply("GetState", serde_json::json!("Running")),
        unsupported(),
        optional("GetConfigTitle", title),
        optional("GetConfigDescription", description),
        unsupported(),
        unsupported(),
        unsupported(),
        unsupported(),
        unsupported(),
    ]
}

fn yaml_string(text: &str, key: &str) -> String {
    let prefix = format!("{key}: ");
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("{key} is absent"));
    serde_json::from_str(value).unwrap()
}

fn apply_camilla(service: &Service, events: Arc<Events>) -> JobStarted {
    started(
        service,
        Request::ApplyCamilla(ApplyCamillaParams {
            camilla_devices: None,
        }),
        events,
    )
}

#[test]
fn apply_settings_round_trip_and_legacy_defaults() {
    let root = root("settings");
    let path = root.join("settings.json");
    fs::write(
        &path,
        br#"{"version":1,"settings":{"language":"en","recent_projects":[],"last_window_size":null}}"#,
    )
    .unwrap();
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        app_data_path: root.clone(),
        ..Deps::default()
    });
    let Response::AppSettingsGet(old) = ok(&service, Request::AppSettingsGet) else {
        panic!()
    };
    assert_eq!(old.apply, ApplySettings::default());

    let settings = ApplySettings {
        camilla: CamillaApply {
            level: ApplyLevel::Manage,
            address: "localhost:4321".into(),
        },
        eqapo: EqApoApply {
            level: ApplyLevel::Apply,
        },
    };
    assert_eq!(
        ok(&service, Request::ApplySettingsSet(settings.clone())),
        Response::ApplySettingsSet(settings.clone())
    );
    drop(service);
    let reopened = Service::new(Deps {
        app_settings_path: Some(path),
        app_data_path: root.clone(),
        ..Deps::default()
    });
    let Response::AppSettingsGet(saved) = ok(&reopened, Request::AppSettingsGet) else {
        panic!()
    };
    assert_eq!(saved.apply, settings);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_settings_rejects_addresses_without_a_port() {
    let service = Service::new(Deps::default());
    let mut settings = ApplySettings::default();
    settings.camilla.address = "localhost".into();
    let Envelope::Err(error) =
        service.call(Request::ApplySettingsSet(settings), Arc::new(NoEvents))
    else {
        panic!("invalid address must fail")
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(
        error.params.get("field").map(String::as_str),
        Some("camilla.address")
    );
}

#[test]
fn apply_camilla_covers_success_rejection_rollback_folder_and_design_matches() {
    let root = root("camilla");
    let service = calibrated_service(&root, Deps::default());
    let applied = root.join("app-data/applied/camilladsp");
    fs::create_dir_all(&applied).unwrap();
    fs::write(applied.join("obsolete.txt"), b"old").unwrap();

    let (address, server) = ws_server(vec![
        reply("ValidateConfig", serde_json::json!("validated")),
        reply("GetConfigFilePath", serde_json::json!("old.yml")),
        reply_ok("SetConfigFilePath"),
        reply_ok("Reload"),
    ]);
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Apply,
            address,
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let started = apply_camilla(&service, events.clone());
    let JobResult::Apply(first) = succeeded(events.wait(started.job)) else {
        panic!("apply result is absent")
    };
    assert!(matches!(first.outcome, ApplyOutcome::Applied));
    assert!(!first.stale);
    assert!(!applied.join("obsolete.txt").exists());
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 4);

    let yml = fs::read_dir(applied.join("configs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension().and_then(|value| value.to_str()) == Some("yml")
                && !path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with("_bypass.yml")
        })
        .unwrap();
    let yaml = fs::read_to_string(&yml).unwrap();
    let title = yaml_string(&yaml, "title");
    let description = yaml_string(&yaml, "description");
    assert!(description.contains(&format!("secs-design {}", first.design_id)));

    let (address, server) = ws_server(vec![rejected("ValidateConfig", "bad yaml")]);
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Apply,
            address,
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let rejected_job = apply_camilla(&service, events.clone());
    assert!(matches!(
        events.wait(rejected_job.job),
        JobEvent::Finished {
            status: JobStatus::Failed,
            error: Some(IpcError {
                code: ErrorCode::ApplyFailed,
                ..
            }),
            ..
        }
    ));
    assert_eq!(server.join().unwrap().len(), 1);

    let (address, server) = ws_server(vec![
        reply("ValidateConfig", serde_json::json!("validated")),
        reply("GetConfigFilePath", serde_json::json!("old.yml")),
        reply_ok("SetConfigFilePath"),
        rejected("Reload", "cannot start"),
        reply_ok("SetConfigFilePath"),
        reply_ok("Reload"),
    ]);
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Apply,
            address,
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let rollback_job = apply_camilla(&service, events.clone());
    let JobResult::Apply(rolled_back) = succeeded(events.wait(rollback_job.job)) else {
        panic!("apply result is absent")
    };
    assert!(matches!(
        rolled_back.outcome,
        ApplyOutcome::RolledBack {
            error: IpcError {
                code: ErrorCode::ApplyFailed,
                ..
            }
        }
    ));
    assert_eq!(server.join().unwrap().len(), 6);

    for (status_title, status_description, expected) in [
        (
            Some(title.as_str()),
            Some(description.as_str()),
            DesignMatch::This {
                design_id: first.design_id.clone(),
            },
        ),
        (
            Some("another config"),
            Some(description.as_str()),
            DesignMatch::Other {
                title: "another config".into(),
            },
        ),
        (None, None, DesignMatch::Unknown),
    ] {
        let (address, server) = ws_server(status_replies(status_title, status_description));
        set_apply(
            &service,
            CamillaApply {
                level: ApplyLevel::Apply,
                address,
            },
            EqApoApply::default(),
        );
        let Response::ApplyStatus(status) = ok(&service, Request::ApplyStatus) else {
            panic!()
        };
        assert!(status.camilla.reachable);
        assert_eq!(status.camilla.design, expected);
        assert_eq!(server.join().unwrap().len(), 10);
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_eqapo_installs_reapplies_repairs_once_and_removes_stale_files() {
    let root = root("eqapo-apply");
    let config_root = root.join("EqualizerAPO/config");
    fs::create_dir_all(&config_root).unwrap();
    let original = "Preamp: -3 dB\n";
    fs::write(config_root.join("config.txt"), original).unwrap();
    fs::create_dir_all(config_root.join("SECS2R/old/nested")).unwrap();
    fs::write(config_root.join("SECS2R/stale.txt"), b"stale").unwrap();
    fs::write(config_root.join("SECS2R/old/nested/stale.txt"), b"stale").unwrap();
    let install = EqApoInstall {
        config_path: config_root.to_string_lossy().into_owned(),
        install_path: "EqualizerAPO-XT/current".into(),
        variant: EqApoVariant::Xt,
    };
    let service = calibrated_service(
        &root,
        Deps {
            eqapo_install: Arc::new(move || Some(install.clone())),
            ..Deps::default()
        },
    );
    set_apply(
        &service,
        CamillaApply::default(),
        EqApoApply {
            level: ApplyLevel::Apply,
        },
    );

    let events = Arc::new(Events::default());
    let first_job = started(&service, Request::ApplyEqapo, events.clone());
    let JobResult::Apply(first) = succeeded(events.wait(first_job.job)) else {
        panic!("apply result is absent")
    };
    assert!(matches!(first.outcome, ApplyOutcome::Applied));
    let bundle = config_root.join("SECS2R");
    assert!(bundle.join("SECS.txt").is_file());
    assert!(!bundle.join("config.txt").exists());
    assert!(bundle.join("secs-design.json").is_file());
    assert!(!bundle.join("stale.txt").exists());
    assert!(!bundle.join("old").exists());
    assert_eq!(
        fs::read_to_string(config_root.join("config.txt.secs2r-backup")).unwrap(),
        original
    );
    assert_eq!(
        secs_apply::eqapo::include_state(
            &fs::read_to_string(config_root.join("config.txt")).unwrap()
        ),
        secs_apply::eqapo::IncludeState::Present
    );

    let before = fs::metadata(bundle.join("SECS.txt"))
        .unwrap()
        .modified()
        .unwrap();
    thread::sleep(Duration::from_millis(1100));
    let events = Arc::new(Events::default());
    let second_job = started(&service, Request::ApplyEqapo, events.clone());
    succeeded(events.wait(second_job.job));
    let after = fs::metadata(bundle.join("SECS.txt"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(before, after, "unchanged bundle file was rewritten");

    fs::write(
        config_root.join("config.txt"),
        format!("Preamp: -6 dB\n{}\n", secs_apply::eqapo::MARKER),
    )
    .unwrap();
    let events = Arc::new(Events::default());
    let repair_job = started(&service, Request::ApplyEqapo, events.clone());
    succeeded(events.wait(repair_job.job));
    assert_eq!(
        secs_apply::eqapo::include_state(
            &fs::read_to_string(config_root.join("config.txt")).unwrap()
        ),
        secs_apply::eqapo::IncludeState::Present
    );
    assert_eq!(
        fs::read_to_string(config_root.join("config.txt.secs2r-backup")).unwrap(),
        original,
        "the first backup was replaced"
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn eqapo_status_reads_injected_install_and_applied_manifest() {
    let root = root("status");
    fs::write(
        root.join("config.txt"),
        format!(
            "{}\n{}\n",
            secs_apply::eqapo::MARKER,
            secs_apply::eqapo::INCLUDE
        ),
    )
    .unwrap();
    fs::create_dir_all(root.join("SECS2R")).unwrap();
    fs::write(
        root.join("SECS2R/secs-design.json"),
        br#"{"design_id":"abc"}"#,
    )
    .unwrap();
    let install = EqApoInstall {
        config_path: root.to_string_lossy().into_owned(),
        install_path: "EqualizerAPO-XT/current".into(),
        variant: EqApoVariant::Xt,
    };
    let service = Service::new(Deps {
        eqapo_install: Arc::new(move || Some(install.clone())),
        apply: Arc::new(NativeApply),
        ..Deps::default()
    });
    let Response::ApplyStatus(status) = ok(&service, Request::ApplyStatus) else {
        panic!()
    };
    assert!(status.eqapo.installed);
    assert_eq!(status.eqapo.variant, EqApoVariant::Xt);
    assert_eq!(status.eqapo.include, IncludeState::Present);
    assert_eq!(status.eqapo.applied_design_id.as_deref(), Some("abc"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn revert_eqapo_removes_only_the_managed_pair_and_keeps_bundle() {
    let root = root("revert");
    fs::create_dir_all(root.join("SECS2R")).unwrap();
    fs::write(root.join("SECS2R/keep.txt"), b"keep").unwrap();
    fs::write(
        root.join("config.txt"),
        format!(
            "Preamp: -3 dB\n{}\n{}\n",
            secs_apply::eqapo::MARKER,
            secs_apply::eqapo::INCLUDE
        ),
    )
    .unwrap();
    let install = EqApoInstall {
        config_path: root.to_string_lossy().into_owned(),
        install_path: "EqualizerAPO".into(),
        variant: EqApoVariant::Official,
    };
    let service = Service::new(Deps {
        eqapo_install: Arc::new(move || Some(install.clone())),
        apply: Arc::new(NativeApply),
        ..Deps::default()
    });
    assert_eq!(
        ok(&service, Request::RevertEqapo),
        Response::RevertEqapo(())
    );
    assert_eq!(
        fs::read_to_string(root.join("config.txt")).unwrap(),
        "Preamp: -3 dB\n"
    );
    assert_eq!(fs::read(root.join("SECS2R/keep.txt")).unwrap(), b"keep");
    fs::remove_dir_all(root).unwrap();
}
