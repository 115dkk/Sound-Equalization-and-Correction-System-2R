#![forbid(unsafe_code)]

#[path = "../examples/support/case_project.rs"]
mod case_project;

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use secs_service::{
    ApplyAdapter, Deps, EventSink, LocalTime, ManagedHostAdapter, NoEvents, Service,
};
use secs_types::ipc::*;
use sha2::{Digest, Sha256};
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
        self.camilla_apply_report(address, config_path, config_text, timeout)
            .map(|(outcome, _previous_path)| outcome)
    }

    fn camilla_apply_report(
        &self,
        address: &str,
        config_path: &Path,
        config_text: &str,
        timeout: Duration,
    ) -> Result<(ApplyOutcome, Option<String>), IpcError> {
        let transport =
            secs_apply::camilla::WsTransport::connect(address, timeout).map_err(camilla_error)?;
        match secs_apply::camilla::Camilla::new(transport)
            .apply(&config_path.to_string_lossy(), config_text)
            .map_err(camilla_error)?
        {
            secs_apply::camilla::ApplyOutcome::Applied { previous_path } => {
                Ok((ApplyOutcome::Applied, previous_path))
            }
            secs_apply::camilla::ApplyOutcome::RolledBack {
                error,
                previous_path,
            } => Ok((
                ApplyOutcome::RolledBack {
                    error: IpcError::new(ErrorCode::ApplyFailed, error),
                },
                previous_path,
            )),
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

/// Only the Windows-only manage tests below use this adapter.
#[cfg(target_os = "windows")]
#[derive(Debug)]
struct FixedApply {
    result: Result<ApplyOutcome, IpcError>,
}

#[cfg(target_os = "windows")]
impl ApplyAdapter for FixedApply {
    fn camilla_status(
        &self,
        _address: &str,
        _timeout: Duration,
    ) -> Result<CamillaStatus, IpcError> {
        Err(IpcError::new(ErrorCode::NotImplemented, "unused status"))
    }

    fn camilla_apply(
        &self,
        _address: &str,
        _config_path: &Path,
        _config_text: &str,
        _timeout: Duration,
    ) -> Result<ApplyOutcome, IpcError> {
        self.result.clone()
    }

    fn eqapo_include_state(&self, config_text: &str) -> IncludeState {
        NativeApply.eqapo_include_state(config_text)
    }

    fn eqapo_with_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        NativeApply.eqapo_with_include(config_text)
    }

    fn eqapo_without_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        NativeApply.eqapo_without_include(config_text)
    }
}

#[derive(Debug)]
struct FailingCamillaMarker {
    fail: Arc<AtomicBool>,
}

impl ApplyAdapter for FailingCamillaMarker {
    fn camilla_status(&self, address: &str, timeout: Duration) -> Result<CamillaStatus, IpcError> {
        NativeApply.camilla_status(address, timeout)
    }

    fn camilla_apply(
        &self,
        address: &str,
        config_path: &Path,
        config_text: &str,
        timeout: Duration,
    ) -> Result<ApplyOutcome, IpcError> {
        NativeApply.camilla_apply(address, config_path, config_text, timeout)
    }

    fn camilla_apply_report(
        &self,
        address: &str,
        config_path: &Path,
        config_text: &str,
        timeout: Duration,
    ) -> Result<(ApplyOutcome, Option<String>), IpcError> {
        NativeApply.camilla_apply_report(address, config_path, config_text, timeout)
    }

    fn before_camilla_marker_write(&self, _path: &Path) -> Result<(), IpcError> {
        if self.fail.load(Ordering::SeqCst) {
            Err(IpcError::new(
                ErrorCode::ApplyFailed,
                "injected CamillaDSP marker write failure",
            ))
        } else {
            Ok(())
        }
    }

    fn eqapo_include_state(&self, config_text: &str) -> IncludeState {
        NativeApply.eqapo_include_state(config_text)
    }

    fn eqapo_with_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        NativeApply.eqapo_with_include(config_text)
    }

    fn eqapo_without_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        NativeApply.eqapo_without_include(config_text)
    }
}

#[derive(Debug)]
struct FailingEqapoSwitch {
    fail: Arc<AtomicBool>,
    break_restore: Option<PathBuf>,
}

impl ApplyAdapter for FailingEqapoSwitch {
    fn camilla_status(
        &self,
        _address: &str,
        _timeout: Duration,
    ) -> Result<CamillaStatus, IpcError> {
        Err(IpcError::new(ErrorCode::NotImplemented, "unused status"))
    }

    fn camilla_apply(
        &self,
        _address: &str,
        _config_path: &Path,
        _config_text: &str,
        _timeout: Duration,
    ) -> Result<ApplyOutcome, IpcError> {
        Err(IpcError::new(ErrorCode::NotImplemented, "unused apply"))
    }

    fn eqapo_include_state(&self, config_text: &str) -> IncludeState {
        NativeApply.eqapo_include_state(config_text)
    }

    fn eqapo_with_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        if self.fail.load(Ordering::SeqCst) {
            if let Some(entry) = &self.break_restore {
                fs::remove_file(entry).unwrap();
                fs::create_dir(entry).unwrap();
                fs::write(entry.join("held"), b"held").unwrap();
            }
            Err(IpcError::new(
                ErrorCode::ApplyFailed,
                "injected entry switch failure",
            ))
        } else {
            NativeApply.eqapo_with_include(config_text)
        }
    }

    fn eqapo_without_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        NativeApply.eqapo_without_include(config_text)
    }
}

#[derive(Debug)]
struct FailingGenerationInstall {
    fail: Arc<AtomicBool>,
}

impl ApplyAdapter for FailingGenerationInstall {
    fn camilla_status(
        &self,
        _address: &str,
        _timeout: Duration,
    ) -> Result<CamillaStatus, IpcError> {
        Err(IpcError::new(ErrorCode::NotImplemented, "unused status"))
    }

    fn camilla_apply(
        &self,
        _address: &str,
        _config_path: &Path,
        _config_text: &str,
        _timeout: Duration,
    ) -> Result<ApplyOutcome, IpcError> {
        Err(IpcError::new(ErrorCode::NotImplemented, "unused apply"))
    }

    fn before_generation_install(&self, _root: &Path, _stage: &Path) -> Result<(), IpcError> {
        if self.fail.load(Ordering::SeqCst) {
            Err(IpcError::new(
                ErrorCode::ApplyFailed,
                "injected generation install failure",
            ))
        } else {
            Ok(())
        }
    }

    fn eqapo_include_state(&self, config_text: &str) -> IncludeState {
        NativeApply.eqapo_include_state(config_text)
    }

    fn eqapo_with_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        NativeApply.eqapo_with_include(config_text)
    }

    fn eqapo_without_include(&self, config_text: &str) -> Result<Option<String>, IpcError> {
        NativeApply.eqapo_without_include(config_text)
    }
}

#[derive(Debug)]
struct RecordingManagedHost {
    state: ServiceState,
    status: HostStatus,
    requests: Arc<Mutex<Vec<HostRequest>>>,
}

impl ManagedHostAdapter for RecordingManagedHost {
    fn service_state(&self) -> ServiceState {
        self.state
    }

    fn call(&self, request: &HostRequest) -> Result<HostResponse, IpcError> {
        lock(&self.requests).push(request.clone());
        match request {
            HostRequest::Status => Ok(HostResponse::Status(Box::new(self.status.clone()))),
            HostRequest::SetManaged(_)
            | HostRequest::StopManaging
            | HostRequest::UpdateSelf { .. } => Ok(HostResponse::Ok),
            HostRequest::Log { .. } => Ok(HostResponse::Log(Vec::new())),
        }
    }

    fn install(&self) -> Result<(), IpcError> {
        Ok(())
    }

    fn uninstall(&self) -> Result<(), IpcError> {
        Ok(())
    }
}

#[derive(Debug)]
struct RejectingManagedHost;

impl ManagedHostAdapter for RejectingManagedHost {
    fn service_state(&self) -> ServiceState {
        ServiceState::Running
    }

    fn call(&self, request: &HostRequest) -> Result<HostResponse, IpcError> {
        match request {
            HostRequest::SetManaged(_) => Err(IpcError::new(
                ErrorCode::ApplyFailed,
                "injected SetManaged failure",
            )),
            HostRequest::Status => Ok(HostResponse::Status(Box::new(host_status()))),
            HostRequest::StopManaging | HostRequest::UpdateSelf { .. } => Ok(HostResponse::Ok),
            HostRequest::Log { .. } => Ok(HostResponse::Log(Vec::new())),
        }
    }

    fn install(&self) -> Result<(), IpcError> {
        Ok(())
    }

    fn uninstall(&self) -> Result<(), IpcError> {
        Ok(())
    }
}

fn host_status() -> HostStatus {
    HostStatus {
        version: "test".into(),
        managed: HostManaged::default(),
        camilla: ManagedProcess::default(),
        eqapo_include: IncludeState::Absent,
        last_repair: None,
    }
}

fn camilla_error(error: secs_apply::ApplyError) -> IpcError {
    let connection_refused = matches!(
        &error,
        secs_apply::ApplyError::Connect {
            connection_refused: true,
            ..
        }
    );
    let mut ipc = IpcError::new(ErrorCode::ApplyFailed, error.to_string());
    ipc.retryable = matches!(
        error,
        secs_apply::ApplyError::Connect { .. } | secs_apply::ApplyError::Transport(_)
    );
    if connection_refused {
        ipc.params
            .insert("reason".to_owned(), "connection_refused".to_owned());
    }
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

fn snapshot_files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, at: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out.sort_by(|left, right| left.0.cmp(&right.0));
    out
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

fn calibrated_service(root: &Path, deps: Deps) -> Service {
    calibrated_service_with_apply(root, deps, Arc::new(NativeApply))
}

fn calibrated_service_with_apply(
    root: &Path,
    mut deps: Deps,
    apply: Arc<dyn ApplyAdapter>,
) -> Service {
    deps.app_data_path = root.join("app-data");
    deps.apply = apply;
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
    apply_camilla_with(service, None, events)
}

fn apply_camilla_with(
    service: &Service,
    camilla_devices: Option<CamillaDevices>,
    events: Arc<Events>,
) -> JobStarted {
    started(
        service,
        Request::ApplyCamilla(ApplyCamillaParams { camilla_devices }),
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
            exe_path: Some("camilladsp.exe".into()),
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
fn managed_host_status_and_missing_camilla_executable_are_typed() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let expected = host_status();
    let service = Service::new(Deps {
        managed_host: Arc::new(RecordingManagedHost {
            state: ServiceState::Running,
            status: expected.clone(),
            requests: requests.clone(),
        }),
        ..Deps::default()
    });
    let Response::ApplyStatus(status) = ok(&service, Request::ApplyStatus) else {
        panic!()
    };
    assert_eq!(status.host, ServiceState::Running);
    assert_eq!(status.host_status, Some(Box::new(expected)));
    assert_eq!(
        ok(&service, Request::HostInstall),
        Response::HostInstall(())
    );
    assert_eq!(
        ok(&service, Request::HostUninstall),
        Response::HostUninstall(())
    );
    assert_eq!(
        ok(&service, Request::HostStopManaging),
        Response::HostStopManaging(())
    );
    assert_eq!(
        *lock(&requests),
        vec![HostRequest::Status, HostRequest::StopManaging]
    );

    let mut settings = ApplySettings::default();
    settings.camilla.level = ApplyLevel::Manage;
    ok(&service, Request::ApplySettingsSet(settings));
    let Envelope::Err(error) = service.call(
        Request::ApplyCamilla(ApplyCamillaParams::default()),
        Arc::new(NoEvents),
    ) else {
        panic!("missing managed executable must fail before starting a job")
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(
        error.params.get("field").map(String::as_str),
        Some("apply.camilla.exe_path")
    );
}

#[cfg(target_os = "windows")]
#[test]
fn manage_camilla_sends_the_applied_target_to_the_host() {
    let root = root("managed-camilla");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let service = calibrated_service(
        &root,
        Deps {
            managed_host: Arc::new(RecordingManagedHost {
                state: ServiceState::Running,
                status: host_status(),
                requests: requests.clone(),
            }),
            ..Deps::default()
        },
    );
    let (address, server) = ws_server(vec![
        reply("ValidateConfig", serde_json::json!("validated")),
        reply("GetConfigFilePath", serde_json::json!("old.yml")),
        reply_ok("SetConfigFilePath"),
        reply_ok("Reload"),
    ]);
    let expected_port = address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .unwrap();
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Manage,
            address,
            exe_path: Some(r"C:\CamillaDSP\camilladsp.exe".into()),
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let job = apply_camilla(&service, events.clone());
    succeeded(events.wait(job.job));
    server.join().unwrap();
    let calls = lock(&requests);
    assert!(matches!(
        calls.as_slice(),
        [HostRequest::SetManaged(HostManaged {
            camilla: Some(ManagedCamilla { port, .. }),
            eqapo: None,
            ..
        })] if *port == expected_port
    ));
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "windows")]
#[test]
fn failed_set_managed_keeps_the_generation_camilla_already_accepted() {
    let root = root("managed-camilla-host-failure");
    let service = calibrated_service(
        &root,
        Deps {
            managed_host: Arc::new(RejectingManagedHost),
            ..Deps::default()
        },
    );
    let applied = root.join("app-data/applied/camilladsp");
    let (address, server) = ws_server(vec![
        reply("ValidateConfig", serde_json::json!("validated")),
        reply("GetConfigFilePath", serde_json::json!("before.yml")),
        reply_ok("SetConfigFilePath"),
        reply_ok("Reload"),
    ]);
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Apply,
            address,
            exe_path: None,
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let first_job = apply_camilla(&service, events.clone());
    let JobResult::Apply(first) = succeeded(events.wait(first_job.job)) else {
        panic!("apply result is absent")
    };
    server.join().unwrap();

    let (address, server) = ws_server(vec![
        reply("ValidateConfig", serde_json::json!("validated")),
        reply("GetConfigFilePath", serde_json::json!("before.yml")),
        reply_ok("SetConfigFilePath"),
        reply_ok("Reload"),
    ]);
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Manage,
            address,
            exe_path: Some(r"C:\CamillaDSP\camilladsp.exe".into()),
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let failed_job = apply_camilla(&service, events.clone());
    let JobEvent::Finished {
        status: JobStatus::Failed,
        error: Some(error),
        ..
    } = events.wait(failed_job.job)
    else {
        panic!("host handoff must fail")
    };
    assert_eq!(error.detail, "injected SetManaged failure");
    server.join().unwrap();

    let generations = fs::read_dir(&applied)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            path.is_dir().then_some(path)
        })
        .collect::<Vec<_>>();
    assert_eq!(generations.len(), 2);
    assert!(Path::new(&first.folder).is_dir());
    assert!(
        generations
            .iter()
            .all(|generation| generation.join("secs-design.json").is_file())
    );
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "windows")]
#[test]
fn managed_camilla_hands_over_only_when_nothing_is_listening() {
    let root = root("managed-handover");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let host = Arc::new(RecordingManagedHost {
        state: ServiceState::Running,
        status: host_status(),
        requests: requests.clone(),
    });
    let refused = IpcError::new(ErrorCode::ApplyFailed, "connection refused")
        .with_param("reason", "connection_refused");
    let service = calibrated_service_with_apply(
        &root,
        Deps {
            managed_host: host.clone(),
            ..Deps::default()
        },
        Arc::new(FixedApply {
            result: Err(refused),
        }),
    );
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Manage,
            address: "127.0.0.1:12345".into(),
            exe_path: Some(r"C:\CamillaDSP\camilladsp.exe".into()),
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let started = apply_camilla(&service, events.clone());
    let JobResult::Apply(result) = succeeded(events.wait(started.job)) else {
        panic!("managed apply result is absent")
    };
    assert_eq!(result.outcome, ApplyOutcome::HandedOver);
    assert!(matches!(
        lock(&requests).as_slice(),
        [HostRequest::SetManaged(HostManaged {
            camilla: Some(_),
            ..
        })]
    ));

    lock(&requests).clear();
    let rejected = calibrated_service_with_apply(
        &root,
        Deps {
            managed_host: host,
            ..Deps::default()
        },
        Arc::new(FixedApply {
            result: Err(IpcError::new(
                ErrorCode::ApplyFailed,
                "listening CamillaDSP rejected the design",
            )),
        }),
    );
    set_apply(
        &rejected,
        CamillaApply {
            level: ApplyLevel::Manage,
            address: "127.0.0.1:12345".into(),
            exe_path: Some(r"C:\CamillaDSP\camilladsp.exe".into()),
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let started = apply_camilla(&rejected, events.clone());
    assert!(matches!(
        events.wait(started.job),
        JobEvent::Finished {
            status: JobStatus::Failed,
            ..
        }
    ));
    assert!(lock(&requests).is_empty());
    fs::remove_dir_all(root).unwrap();
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
            exe_path: None,
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
    assert_eq!(
        PathBuf::from(&first.folder),
        applied.join(format!("{}-1", &first.design_id[..16]))
    );
    assert!(applied.join("obsolete.txt").exists());
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 4);

    let yml = fs::read_dir(PathBuf::from(&first.folder).join("configs"))
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
            exe_path: None,
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
            exe_path: None,
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
                exe_path: None,
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
fn rejected_camilla_generation_keeps_previous_bytes_and_restores_its_path() {
    let root = root("camilla-generation-rollback");
    let service = calibrated_service(&root, Deps::default());
    let applied = root.join("app-data/applied/camilladsp");

    let (address, server) = ws_server(vec![
        reply("ValidateConfig", serde_json::json!("validated")),
        reply("GetConfigFilePath", serde_json::json!("before.yml")),
        reply_ok("SetConfigFilePath"),
        reply_ok("Reload"),
    ]);
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Apply,
            address,
            exe_path: None,
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let first_job = apply_camilla(&service, events.clone());
    let JobResult::Apply(first) = succeeded(events.wait(first_job.job)) else {
        panic!("apply result is absent")
    };
    server.join().unwrap();
    let first_folder = PathBuf::from(&first.folder);
    let first_config = fs::read_dir(first_folder.join("configs"))
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
    let first_bytes = fs::read(&first_config).unwrap();

    let devices = CamillaDevices {
        capture: CamillaCaptureDevice::Asio {
            device: "capture-device".into(),
        },
        playback: CamillaPlaybackDevice::Asio {
            device: "playback-device".into(),
        },
    };
    let previous_path = first_config.to_string_lossy().into_owned();
    let (address, server) = ws_server(vec![
        reply("ValidateConfig", serde_json::json!("validated")),
        reply("GetConfigFilePath", serde_json::json!(previous_path)),
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
            exe_path: None,
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let failed_job = apply_camilla_with(&service, Some(devices.clone()), events.clone());
    let JobResult::Apply(rolled_back) = succeeded(events.wait(failed_job.job)) else {
        panic!("apply result is absent")
    };
    assert!(matches!(
        rolled_back.outcome,
        ApplyOutcome::RolledBack { .. }
    ));
    let requests = server.join().unwrap();
    assert!(requests[4].contains(&first_config.to_string_lossy().replace('\\', "\\\\")));
    assert_eq!(fs::read(&first_config).unwrap(), first_bytes);
    assert!(first_folder.is_dir());
    assert_eq!(fs::read_dir(&applied).unwrap().count(), 1);

    let (address, server) = ws_server(vec![
        reply("ValidateConfig", serde_json::json!("validated")),
        reply(
            "GetConfigFilePath",
            serde_json::json!(first_config.to_string_lossy()),
        ),
        reply_ok("SetConfigFilePath"),
        reply_ok("Reload"),
    ]);
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Apply,
            address,
            exe_path: None,
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let success_job = apply_camilla_with(&service, Some(devices), events.clone());
    let JobResult::Apply(second) = succeeded(events.wait(success_job.job)) else {
        panic!("apply result is absent")
    };
    server.join().unwrap();
    assert_ne!(second.folder, first.folder);
    assert!(first_folder.exists());
    assert!(Path::new(&second.folder).is_dir());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_camilla_rollback_keeps_both_possible_active_generations() {
    let root = root("camilla-rollback-failed");
    let service = calibrated_service(&root, Deps::default());
    let applied = root.join("app-data/applied/camilladsp");
    let (address, server) = ws_server(vec![
        reply("ValidateConfig", serde_json::json!("validated")),
        reply("GetConfigFilePath", serde_json::json!("before.yml")),
        reply_ok("SetConfigFilePath"),
        reply_ok("Reload"),
    ]);
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Apply,
            address,
            exe_path: None,
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let first_job = apply_camilla(&service, events.clone());
    let JobResult::Apply(first) = succeeded(events.wait(first_job.job)) else {
        panic!("apply result is absent")
    };
    server.join().unwrap();
    let first_folder = PathBuf::from(first.folder);
    let first_config = fs::read_dir(first_folder.join("configs"))
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

    let (address, server) = ws_server(vec![
        reply("ValidateConfig", serde_json::json!("validated")),
        reply(
            "GetConfigFilePath",
            serde_json::json!(first_config.to_string_lossy()),
        ),
        reply_ok("SetConfigFilePath"),
        rejected("Reload", "cannot start"),
        rejected("SetConfigFilePath", "cannot restore"),
    ]);
    set_apply(
        &service,
        CamillaApply {
            level: ApplyLevel::Apply,
            address,
            exe_path: None,
        },
        EqApoApply::default(),
    );
    let events = Arc::new(Events::default());
    let failed_job = apply_camilla(&service, events.clone());
    let JobEvent::Finished {
        status: JobStatus::Failed,
        error: Some(error),
        ..
    } = events.wait(failed_job.job)
    else {
        panic!("apply must fail")
    };
    assert!(error.detail.contains("rollback failed"));
    server.join().unwrap();
    let generations = fs::read_dir(&applied)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            path.is_dir().then_some(path)
        })
        .collect::<Vec<_>>();
    assert_eq!(generations.len(), 2);
    assert!(first_folder.exists());
    assert!(
        generations
            .iter()
            .all(|generation| generation.join("secs-design.json").is_file())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn camilla_pruning_uses_the_reported_previous_path_after_a_marker_write_fails() {
    let root = root("camilla-stale-marker");
    let fail_marker = Arc::new(AtomicBool::new(false));
    let service = calibrated_service_with_apply(
        &root,
        Deps::default(),
        Arc::new(FailingCamillaMarker {
            fail: fail_marker.clone(),
        }),
    );
    let marker = root.join("app-data/applied/camilladsp-current.txt");
    let apply_once = |previous_path: &str| {
        let (address, server) = ws_server(vec![
            reply("ValidateConfig", serde_json::json!("validated")),
            reply("GetConfigFilePath", serde_json::json!(previous_path)),
            reply_ok("SetConfigFilePath"),
            reply_ok("Reload"),
        ]);
        set_apply(
            &service,
            CamillaApply {
                level: ApplyLevel::Apply,
                address,
                exe_path: None,
            },
            EqApoApply::default(),
        );
        let events = Arc::new(Events::default());
        let job = apply_camilla(&service, events.clone());
        let JobResult::Apply(result) = succeeded(events.wait(job.job)) else {
            panic!("apply result is absent")
        };
        server.join().unwrap();
        result
    };
    let main_config = |folder: &str| {
        fs::read_dir(Path::new(folder).join("configs"))
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
            .unwrap()
    };

    let first = apply_once("before.yml");
    let first_config = main_config(&first.folder);
    let first_name = Path::new(&first.folder).file_name().unwrap().to_owned();
    assert_eq!(fs::read(&marker).unwrap(), first_name.as_encoded_bytes());

    fail_marker.store(true, Ordering::SeqCst);
    let second = apply_once(&first_config.to_string_lossy());
    let second_config = main_config(&second.folder);
    assert_eq!(fs::read(&marker).unwrap(), first_name.as_encoded_bytes());

    fail_marker.store(false, Ordering::SeqCst);
    let third = apply_once(&second_config.to_string_lossy());
    assert!(Path::new(&second.folder).exists());
    assert!(!Path::new(&first.folder).exists());
    assert!(Path::new(&third.folder).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_camilla_gives_each_design_its_own_folder_so_reload_reads_its_firs() {
    // CamillaDSP's Reload rebuilds only filters whose definition changed, and a
    // Conv filter's definition is its absolute file name. A new design must
    // therefore name its FIRs in another folder, or the old ones keep playing.
    let root = root("camilla-designs");
    let service = calibrated_service(&root, Deps::default());
    let applied = root.join("app-data/applied/camilladsp");
    // Older releases applied every design into this folder itself.
    fs::create_dir_all(applied.join("configs")).unwrap();
    fs::write(applied.join("configs/secs_linear.yml"), b"old").unwrap();
    let apply_once = |service: &Service, previous: &Path| {
        let (address, server) = ws_server(vec![
            reply("ValidateConfig", serde_json::json!("validated")),
            reply(
                "GetConfigFilePath",
                serde_json::json!(previous.to_string_lossy()),
            ),
            reply_ok("SetConfigFilePath"),
            reply_ok("Reload"),
        ]);
        set_apply(
            service,
            CamillaApply {
                level: ApplyLevel::Apply,
                address,
                exe_path: None,
            },
            EqApoApply::default(),
        );
        let events = Arc::new(Events::default());
        let started = apply_camilla(service, events.clone());
        let JobResult::Apply(result) = succeeded(events.wait(started.job)) else {
            panic!("apply result is absent")
        };
        assert!(matches!(result.outcome, ApplyOutcome::Applied));
        (result, server.join().unwrap())
    };
    let main_config = |folder: &str| {
        fs::read_dir(Path::new(folder).join("configs"))
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
            .unwrap()
    };

    let old_config = applied.join("configs/secs_linear.yml");
    let (first, requests) = apply_once(&service, &old_config);
    assert_eq!(
        PathBuf::from(&first.folder),
        applied.join(format!("{}-1", &first.design_id[..16]))
    );
    assert!(requests[2].contains("SetConfigFilePath"));
    assert!(requests[2].contains(&first.design_id[..16]));
    assert!(applied.join("configs").exists());
    let first_config = main_config(&first.folder);
    let (again, _) = apply_once(&service, &first_config);
    assert_ne!(again.folder, first.folder);
    assert!(Path::new(&first.folder).exists());
    assert!(!applied.join("configs").exists());

    // A setting that changes only the FIRs: a new design and a new folder.
    ok(
        &service,
        Request::CalibrationSettingsEdit(CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::HfPhase { on: true },
        }),
    );
    let events = Arc::new(Events::default());
    let calibration = started(
        &service,
        Request::Calibrate(CalibrateParams::default()),
        events.clone(),
    );
    succeeded(events.wait(calibration.job));
    let again_config = main_config(&again.folder);
    let (second, requests) = apply_once(&service, &again_config);
    assert_ne!(second.design_id, first.design_id);
    assert_ne!(second.folder, first.folder);
    assert!(requests[2].contains(&second.design_id[..16]));
    assert!(PathBuf::from(&second.folder).join("coeffs").is_dir());
    assert!(!Path::new(&first.folder).exists());
    assert!(Path::new(&again.folder).exists());
    let mut left: Vec<_> = fs::read_dir(&applied)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    left.sort();
    let mut expected = vec![PathBuf::from(&again.folder), PathBuf::from(&second.folder)];
    expected.sort();
    assert_eq!(left, expected);

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
    let first_name = format!("{}-1", &first.design_id[..16]);
    let first_generation = bundle.join(&first_name);
    assert_eq!(
        fs::read_to_string(bundle.join("SECS.txt")).unwrap(),
        format!("Include: {first_name}\\config.txt\r\n")
    );
    assert!(first_generation.join("config.txt").is_file());
    assert!(first_generation.join("secs-design.json").is_file());
    assert!(bundle.join("stale.txt").exists());
    assert!(bundle.join("old").exists());
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

    let events = Arc::new(Events::default());
    let second_job = started(&service, Request::ApplyEqapo, events.clone());
    let JobResult::Apply(second) = succeeded(events.wait(second_job.job)) else {
        panic!("apply result is absent")
    };
    let second_name = format!("{}-2", &second.design_id[..16]);
    assert_eq!(
        fs::read_to_string(bundle.join("SECS.txt")).unwrap(),
        format!("Include: {second_name}\\config.txt\r\n")
    );
    assert!(first_generation.exists());
    assert!(bundle.join(&second_name).join("config.txt").is_file());
    assert!(!bundle.join("stale.txt").exists());
    assert!(!bundle.join("old").exists());

    fs::write(
        config_root.join("config.txt"),
        format!("Preamp: -6 dB\n{}\n", secs_apply::eqapo::MARKER),
    )
    .unwrap();
    let events = Arc::new(Events::default());
    let repair_job = started(&service, Request::ApplyEqapo, events.clone());
    let JobResult::Apply(third) = succeeded(events.wait(repair_job.job)) else {
        panic!("apply result is absent")
    };
    assert!(!first_generation.exists());
    assert!(bundle.join(&second_name).exists());
    assert!(Path::new(&third.folder).exists());
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
fn applied_eqapo_manifest_names_every_installed_file_with_size_and_hash() {
    let root = root("eqapo-manifest");
    let config_root = root.join("EqualizerAPO/config");
    fs::create_dir_all(&config_root).unwrap();
    fs::write(config_root.join("config.txt"), "Preamp: -3 dB\n").unwrap();
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
    let job = started(&service, Request::ApplyEqapo, events.clone());
    let JobResult::Apply(result) = succeeded(events.wait(job.job)) else {
        panic!("apply result is absent")
    };
    let generation = PathBuf::from(result.folder);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(generation.join("secs-design.json")).unwrap()).unwrap();
    let files = manifest["files"].as_array().unwrap();
    assert!(!files.is_empty());
    for record in files {
        let path = generation.join(record["path"].as_str().unwrap());
        let bytes = fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(record["bytes"].as_u64().unwrap(), bytes.len() as u64);
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(record["sha256"].as_str().unwrap(), digest);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_eqapo_prunes_a_link_without_touching_its_target() {
    let root = root("eqapo-linked-stale");
    let config_root = root.join("EqualizerAPO/config");
    let outside = root.join("outside");
    fs::create_dir_all(config_root.join("SECS2R")).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(config_root.join("config.txt"), "Preamp: -3 dB\n").unwrap();
    fs::write(outside.join("sentinel.txt"), b"outside").unwrap();
    let link = config_root.join("SECS2R/linked");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    #[cfg(windows)]
    if let Err(error) = std::os::windows::fs::symlink_dir(&outside, &link) {
        eprintln!("skipping Windows link half: {error}");
        fs::remove_dir_all(root).unwrap();
        return;
    }

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
    let job = started(&service, Request::ApplyEqapo, events.clone());
    succeeded(events.wait(job.job));

    assert_eq!(fs::read(outside.join("sentinel.txt")).unwrap(), b"outside");
    assert!(!link.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_eqapo_include_repair_keeps_every_generation_the_entry_has_referenced() {
    let root = root("eqapo-switch-failure");
    let config_root = root.join("EqualizerAPO/config");
    fs::create_dir_all(&config_root).unwrap();
    fs::write(config_root.join("config.txt"), "Preamp: -3 dB\n").unwrap();
    let install = EqApoInstall {
        config_path: config_root.to_string_lossy().into_owned(),
        install_path: "EqualizerAPO-XT/current".into(),
        variant: EqApoVariant::Xt,
    };
    let fail = Arc::new(AtomicBool::new(false));
    let service = calibrated_service_with_apply(
        &root,
        Deps {
            eqapo_install: Arc::new(move || Some(install.clone())),
            ..Deps::default()
        },
        Arc::new(FailingEqapoSwitch {
            fail: fail.clone(),
            break_restore: None,
        }),
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
    let applied = config_root.join("SECS2R");
    let entry_before = fs::read(applied.join("SECS.txt")).unwrap();
    let previous = applied.join(format!("{}-1", &first.design_id[..16]));
    assert!(previous.join("config.txt").is_file());

    fail.store(true, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let failed_job = started(&service, Request::ApplyEqapo, events.clone());
    assert!(matches!(
        events.wait(failed_job.job),
        JobEvent::Finished {
            status: JobStatus::Failed,
            error: Some(IpcError {
                code: ErrorCode::ApplyFailed,
                ..
            }),
            ..
        }
    ));
    assert_eq!(fs::read(applied.join("SECS.txt")).unwrap(), entry_before);
    assert!(previous.join("config.txt").is_file());
    let mut generations: Vec<_> = fs::read_dir(&applied)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    generations.sort();
    assert_eq!(
        generations,
        [
            previous,
            applied.join(format!("{}-2", &first.design_id[..16])),
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_eqapo_generation_install_preserves_every_previous_byte() {
    let root = root("eqapo-generation-install-failure");
    let config_root = root.join("EqualizerAPO/config");
    fs::create_dir_all(&config_root).unwrap();
    fs::write(config_root.join("config.txt"), "Preamp: -3 dB\n").unwrap();
    let install = EqApoInstall {
        config_path: config_root.to_string_lossy().into_owned(),
        install_path: "EqualizerAPO-XT/current".into(),
        variant: EqApoVariant::Xt,
    };
    let fail = Arc::new(AtomicBool::new(false));
    let service = calibrated_service_with_apply(
        &root,
        Deps {
            eqapo_install: Arc::new(move || Some(install.clone())),
            ..Deps::default()
        },
        Arc::new(FailingGenerationInstall { fail: fail.clone() }),
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
    succeeded(events.wait(first_job.job));
    let applied = config_root.join("SECS2R");
    let before = snapshot_files(&applied);

    fail.store(true, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let failed_job = started(&service, Request::ApplyEqapo, events.clone());
    assert!(matches!(
        events.wait(failed_job.job),
        JobEvent::Finished {
            status: JobStatus::Failed,
            error: Some(IpcError {
                code: ErrorCode::ApplyFailed,
                ..
            }),
            ..
        }
    ));
    assert_eq!(snapshot_files(&applied), before);
    let generations = fs::read_dir(&applied)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            path.is_dir().then_some(path)
        })
        .collect::<Vec<_>>();
    assert_eq!(generations.len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_eqapo_restore_keeps_the_generation_the_entry_may_reference() {
    let root = root("eqapo-restore-failure");
    let config_root = root.join("EqualizerAPO/config");
    fs::create_dir_all(&config_root).unwrap();
    fs::write(config_root.join("config.txt"), "Preamp: -3 dB\n").unwrap();
    let install = EqApoInstall {
        config_path: config_root.to_string_lossy().into_owned(),
        install_path: "EqualizerAPO-XT/current".into(),
        variant: EqApoVariant::Xt,
    };
    let fail = Arc::new(AtomicBool::new(false));
    let entry = config_root.join("SECS2R/SECS.txt");
    let service = calibrated_service_with_apply(
        &root,
        Deps {
            eqapo_install: Arc::new(move || Some(install.clone())),
            ..Deps::default()
        },
        Arc::new(FailingEqapoSwitch {
            fail: fail.clone(),
            break_restore: Some(entry.clone()),
        }),
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
    succeeded(events.wait(first_job.job));

    fail.store(true, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let failed_job = started(&service, Request::ApplyEqapo, events.clone());
    let JobEvent::Finished {
        status: JobStatus::Failed,
        error: Some(error),
        ..
    } = events.wait(failed_job.job)
    else {
        panic!("apply must fail")
    };
    assert!(error.detail.contains("rollback failed"));
    let generations = fs::read_dir(config_root.join("SECS2R"))
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            path.is_dir()
                .then_some(path)
                .filter(|path| path.file_name().unwrap() != "SECS.txt")
        })
        .collect::<Vec<_>>();
    assert_eq!(generations.len(), 2);
    assert!(
        generations
            .iter()
            .all(|generation| generation.join("config.txt").is_file())
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
    fs::create_dir_all(root.join("SECS2R/abc-1")).unwrap();
    fs::write(
        root.join("SECS2R/SECS.txt"),
        "Include: abc-1\\config.txt\r\n",
    )
    .unwrap();
    fs::write(
        root.join("SECS2R/abc-1/secs-design.json"),
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

    fs::remove_file(root.join("SECS2R/SECS.txt")).unwrap();
    fs::write(
        root.join("SECS2R/secs-design.json"),
        br#"{"design_id":"flat"}"#,
    )
    .unwrap();
    let Response::ApplyStatus(status) = ok(&service, Request::ApplyStatus) else {
        panic!()
    };
    assert_eq!(status.eqapo.applied_design_id.as_deref(), Some("flat"));
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
