#![forbid(unsafe_code)]

#[path = "../examples/support/case_project.rs"]
mod case_project;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use secs_service::{Deps, EventSink, LocalTime, LogLevel, NoEvents, Service, SettingsEffect};
use secs_types::ipc::{Envelope, ErrorCode, JobEvent, ProjectNewParams, Request};
use sha2::{Digest, Sha256};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
use secs_project::{MeasurementResult, MeasurementStore, Project};
use secs_types::calibration::CalibrationSettings;
use secs_types::ipc::{
    AppSettings, AppSettingsParams, CalibrateParams, DspSetParams, ExportKind, ExportParams,
    JobCancelParams, JobId, JobKind, JobResult, JobStatus, MdatScope, MimoParams, OutputRateChoice,
    Response, TargetPresetParams,
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
    hold_progress: Mutex<bool>,
    progress_released: Condvar,
}

impl EventSink for Events {
    fn send(&self, event: JobEvent) {
        let progress = matches!(event, JobEvent::Progress { .. });
        lock(&self.values).push(event);
        self.changed.notify_all();
        if progress {
            let mut held = lock(&self.hold_progress);
            while *held {
                held = self.progress_released.wait(held).unwrap();
            }
        }
    }
}

impl Events {
    fn holding_progress() -> Self {
        Self {
            hold_progress: Mutex::new(true),
            ..Self::default()
        }
    }

    fn wait_started(&self, kind: JobKind, timeout: Duration) -> JobId {
        let deadline = Instant::now() + timeout;
        let mut values = lock(&self.values);
        loop {
            if let Some(JobEvent::Started { job, .. }) = values
                .iter()
                .find(|event| matches!(event, JobEvent::Started { kind: event_kind, .. } if *event_kind == kind))
            {
                return *job;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "{kind:?} job did not start");
            values = self.changed.wait_timeout(values, remaining).unwrap().0;
        }
    }

    fn release_progress(&self) {
        *lock(&self.hold_progress) = false;
        self.progress_released.notify_all();
    }

    fn started_jobs(&self, kind: JobKind) -> Vec<JobId> {
        lock(&self.values)
            .iter()
            .filter_map(|event| match event {
                JobEvent::Started {
                    job,
                    kind: event_kind,
                } if *event_kind == kind => Some(*job),
                _ => None,
            })
            .collect()
    }

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
fn status_project_and_results_come_from_the_same_open_project() {
    let root = temp_root("status-session-snapshot");
    fs::create_dir_all(&root).unwrap();
    let (without_subs, settings) = case_project::write(&root.join("without-subs.secs"), "room");
    let mut store = MeasurementStore::open(&without_subs).unwrap();
    store
        .project_mut()
        .config
        .speakers
        .retain(|speaker| speaker.size != "sub");
    store.project_mut().config.renumber();
    store.save().unwrap();
    let (with_subs, with_subs_settings) = case_project::write(&root.join("with-subs.secs"), "room");

    for (path, settings) in [(&without_subs, settings), (&with_subs, with_subs_settings)] {
        let service = service();
        open(&service, path);
        let events = Arc::new(Events::default());
        let job = start_calibration(&service, events.clone(), settings);
        finished_result(events.wait(job, Duration::from_secs(120)));
    }

    let service = Arc::new(service());
    open(&service, &without_subs);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let opener_service = service.clone();
    let opener_barrier = barrier.clone();
    let first = without_subs.clone();
    let second = with_subs.clone();
    let opener = std::thread::spawn(move || {
        opener_barrier.wait();
        for index in 0..50 {
            open(
                &opener_service,
                if index % 2 == 0 { &second } else { &first },
            );
        }
    });
    barrier.wait();
    for _ in 0..50 {
        let status = service.status();
        assert_eq!(status.project.is_some(), status.results.is_some());
        if let (Some(project), Some(results)) = (status.project, status.results)
            && let Some(calibration) = results.calibration
        {
            let expected_subs = if project.path == without_subs.to_string_lossy() {
                0
            } else {
                2
            };
            assert_eq!(
                calibration.sub_roles.len(),
                expected_subs,
                "{}",
                project.path
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    opener.join().unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_failed_project_save_after_a_recorded_measurement_leaves_the_project_dirty() {
    let root = temp_root("failed-record-save");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("room.secs");
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
    call_ok(
        &service,
        Request::MeasureConfigure(secs_types::ipc::MeasureConfigureParams {
            method: "mock".into(),
            ..Default::default()
        }),
        Arc::new(NoEvents),
    );
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );
    let Response::ProjectInfo(before) = call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert!(!before.dirty);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();

    let events = Arc::new(Events::default());
    let Response::MeasurePosition(started) = call_ok(
        &service,
        Request::MeasurePosition(secs_types::ipc::MeasurePositionParams { position: 1 }),
        events.clone(),
    ) else {
        panic!("measurement must start")
    };
    let JobEvent::Finished {
        status: JobStatus::Failed,
        error: Some(_),
        ..
    } = events.wait(started.job, Duration::from_secs(30))
    else {
        panic!("record must fail when the project destination is a directory")
    };
    let Response::ProjectInfo(info) = call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert_eq!(info.positions, [1]);
    assert!(info.dirty);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn opening_a_missing_project_names_the_path_td208() {
    let root = temp_root("missing-project-open");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("missing.secs");
    let service = service();

    let Envelope::Err(error) = service.call(
        Request::ProjectOpen(secs_types::ipc::ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("opening a missing project must fail")
    };
    assert_eq!(error.code, ErrorCode::ProjectIo);
    assert_eq!(
        error.params.get("path").map(String::as_str),
        Some(path.to_string_lossy().as_ref())
    );
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("not_found")
    );
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
struct ReadDenyAcl {
    path: PathBuf,
    user: String,
}

#[cfg(windows)]
impl Drop for ReadDenyAcl {
    fn drop(&mut self) {
        let _ = std::process::Command::new("icacls")
            .arg(&self.path)
            .arg("/remove:d")
            .arg(&self.user)
            .output();
        let _ = std::process::Command::new("icacls")
            .arg(&self.path)
            .arg("/inheritance:e")
            .output();
    }
}

#[test]
#[cfg(windows)]
fn opening_an_unreadable_project_is_a_permission_error_td208() {
    let root = temp_root("unreadable-project-open");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("unreadable.secs");
    fs::write(&path, Project::from_preset("2.0").unwrap().save_text()).unwrap();
    let user = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap(),
        std::env::var("USERNAME").unwrap()
    );
    let denied = std::process::Command::new("icacls")
        .arg(&path)
        .arg("/inheritance:r")
        .arg("/deny")
        .arg(format!("{user}:(R)"))
        .output()
        .unwrap();
    assert!(
        denied.status.success(),
        "icacls failed: {}",
        String::from_utf8_lossy(&denied.stderr)
    );
    let acl = ReadDenyAcl {
        path: path.clone(),
        user,
    };
    let service = service();

    let Envelope::Err(error) = service.call(
        Request::ProjectOpen(secs_types::ipc::ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("opening an unreadable project must fail")
    };
    assert_eq!(error.code, ErrorCode::PermissionDenied);
    assert_eq!(
        error.params.get("path").map(String::as_str),
        Some(path.to_string_lossy().as_ref())
    );
    drop(acl);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_open_surfaces_sidecar_warnings_td183() {
    let root = temp_root("sidecar-warning");
    let path = root.join("warning.secs");
    let irs = path.with_extension("irs");
    fs::create_dir_all(&irs).unwrap();
    let wav = irs.join("L_P1.wav");
    fs::write(&wav, []).unwrap();
    let sidecar = secs_project::source_path(&wav);
    fs::write(&sidecar, b"{not json").unwrap();
    let mut project = Project::from_preset("2.0").unwrap();
    project.measurements = secs_project::PyJson::Array(vec![secs_project::PyJson::Object(vec![
        ("pos".into(), secs_project::PyJson::Int("1".into())),
        (
            "ir".into(),
            secs_project::PyJson::Object(vec![(
                "L".into(),
                secs_project::PyJson::Str("warning.irs/L_P1.wav".into()),
            )]),
        ),
    ])]);
    fs::write(&path, project.save_text()).unwrap();
    let service = service();

    let Response::ProjectOpen(info) = call_ok(
        &service,
        Request::ProjectOpen(secs_types::ipc::ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!();
    };

    assert!(info.migration_warnings.iter().any(|warning| {
        warning.code == "measurement.source.skipped" && warning.entry == sidecar.to_string_lossy()
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_sidecar_curve_is_excluded_and_noticed_td262() {
    let root = temp_root("invalid-sidecar-curve");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let store = MeasurementStore::open(&path).unwrap();
    let first = store.records().into_iter().next().unwrap();
    let position = first.pos;
    let role = first.ir.keys().next().unwrap().clone();
    let wav = path.parent().unwrap().join(&first.ir[&role]);
    secs_project::save_source(
        &wav,
        Some(&secs_mdat::SourceInfo {
            cal: vec![(
                "meterCal".into(),
                secs_mdat::CalInfo {
                    inverse_c_selected: false,
                    source_name: "unsorted.cal".into(),
                    source_path: "unsorted.cal".into(),
                    freq: Some(vec![1_000.0, 100.0]),
                    gain: Some(vec![0.0, 1.0]),
                    phase: None,
                    boxed: Vec::new(),
                },
            )],
            ..Default::default()
        }),
    )
    .unwrap();
    let service = deterministic_service();
    open(&service, &path);

    let Response::MicCalibrationGet(status) =
        call_ok(&service, Request::MicCalibrationGet, Arc::new(NoEvents))
    else {
        panic!();
    };
    assert_eq!(status.attached, 0);

    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    let results = read_results(&service);
    let notices = &results.calibration.unwrap().notices;
    assert!(notices.iter().any(|notice| {
        notice.code == "calibration_curve_invalid"
            && notice.params.get("role") == Some(&role)
            && notice.params.get("position") == Some(&position.to_string())
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reopening_after_calibrating_with_an_invalid_curve_keeps_results_fresh_td262() {
    let root = temp_root("invalid-sidecar-reopen");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let store = MeasurementStore::open(&path).unwrap();
    let first = store.records().into_iter().next().unwrap();
    let position = first.pos;
    let role = first.ir.keys().next().unwrap().clone();
    let wav = path.parent().unwrap().join(&first.ir[&role]);
    secs_project::save_source(
        &wav,
        Some(&secs_mdat::SourceInfo {
            cal: vec![(
                "meterCal".into(),
                secs_mdat::CalInfo {
                    inverse_c_selected: true,
                    source_name: "unsorted.cal".into(),
                    source_path: "unsorted.cal".into(),
                    freq: Some(vec![1_000.0, 100.0]),
                    gain: Some(vec![0.0, 1.0]),
                    phase: None,
                    boxed: Vec::new(),
                },
            )],
            ..Default::default()
        }),
    )
    .unwrap();
    let service = deterministic_service();
    open(&service, &path);

    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    let live = read_results(&service).calibration.unwrap();
    assert!(!live.stale);
    assert_eq!(
        live.notices
            .iter()
            .filter(|notice| {
                notice.code == "calibration_curve_invalid"
                    && notice.params.get("role") == Some(&role)
                    && notice.params.get("position") == Some(&position.to_string())
            })
            .count(),
        1
    );

    let save_events = Arc::new(Events::default());
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        save_events.clone(),
    );
    let save_job = save_events.wait_started(JobKind::Export, Duration::from_secs(2));
    assert!(matches!(
        save_events.wait(save_job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    drop(service);

    let reopened = deterministic_service();
    open(&reopened, &path);
    let cached = read_results(&reopened).calibration.unwrap();
    assert!(
        !cached.stale,
        "invalid calibration changed the reopened signature"
    );
    assert_eq!(
        cached
            .notices
            .iter()
            .filter(|notice| notice.code == "calibration_curve_invalid")
            .count(),
        1
    );
    let Response::MicCalibrationGet(status) =
        call_ok(&reopened, Request::MicCalibrationGet, Arc::new(NoEvents))
    else {
        panic!();
    };
    assert_eq!(status.attached, 0);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn damaged_settings_file_is_reported_and_not_overwritten_until_set_td169() {
    let root = temp_root("damaged-settings");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let damaged = b"{not json";
    fs::write(&path, damaged).unwrap();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = lines.clone();
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        log: Arc::new(move |level, scope, line| {
            lock(&sink).push((level, scope.to_owned(), line.to_owned()));
        }),
        ..Deps::default()
    });

    assert!(matches!(
        call_ok(&service, Request::AppSettingsGet, Arc::new(NoEvents)),
        Response::AppSettingsGet(settings) if settings == AppSettings::default()
    ));
    assert_eq!(fs::read(&path).unwrap(), damaged);
    let lines = lock(&lines);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].0, LogLevel::Warn);
    assert_eq!(lines[0].1, "settings");
    assert!(lines[0].2.contains(&path.to_string_lossy().into_owned()));
    drop(lines);

    let replacement = AppSettings {
        language: "en".into(),
        ..AppSettings::default()
    };
    assert!(matches!(
        call_ok(
            &service,
            Request::AppSettingsSet(AppSettingsParams {
                settings: replacement,
            }),
            Arc::new(NoEvents),
        ),
        Response::AppSettingsSet(_)
    ));
    assert_ne!(fs::read(&path).unwrap(), damaged);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn newer_settings_file_is_never_overwritten_td169() {
    let root = temp_root("newer-settings");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let newer_settings = AppSettings {
        language: "newer".into(),
        ..AppSettings::default()
    };
    let original = serde_json::to_vec(&serde_json::json!({
        "version": 2,
        "settings": newer_settings,
    }))
    .unwrap();
    fs::write(&path, &original).unwrap();
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });

    let Envelope::Err(error) = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings::default(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("a newer settings file must not be overwritten")
    };
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("app_settings_newer")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn newer_settings_file_written_while_running_is_never_overwritten_td169() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct CountingEffect(Arc<AtomicUsize>);

    impl SettingsEffect for CountingEffect {
        fn changed(&self, _previous: &AppSettings, _next: &AppSettings) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let root = temp_root("newer-settings-while-running");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    let calls = Arc::new(AtomicUsize::new(0));
    service.register_settings_effect(Arc::new(CountingEffect(calls.clone())));
    let original = serde_json::to_vec(&serde_json::json!({
        "version": 2,
        "settings": AppSettings {
            language: "newer".into(),
            ..AppSettings::default()
        },
    }))
    .unwrap();
    fs::write(&path, &original).unwrap();

    let Envelope::Err(error) = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings {
                language: "replacement".into(),
                ..AppSettings::default()
            },
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("a settings file that became newer while running must not be overwritten")
    };
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("app_settings_newer")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unreadable_settings_file_is_never_overwritten_td169() {
    // A valid file over the 1 MiB read bound: this app cannot tell which
    // version wrote it, so an explicit set must not replace it (audit #186 A-14).
    let root = temp_root("unreadable-settings");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let mut original = serde_json::to_vec(&serde_json::json!({
        "version": 2,
        "settings": AppSettings::default(),
        "padding": "x".repeat(1024 * 1024 + 16),
    }))
    .unwrap();
    original.push(b'\n');
    fs::write(&path, &original).unwrap();
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });

    let Envelope::Err(error) = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings::default(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("a settings file this app cannot read must not be overwritten")
    };
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("settings_unreadable")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_save_returns_before_the_side_mdat_exists_and_a_job_writes_it_td149() {
    let root = temp_root("side-mdat-job");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let side_mdat = path.with_extension("mdat");
    let service = deterministic_service();
    open(&service, &path);
    let events = Arc::new(Events::holding_progress());

    let response = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    assert!(
        matches!(response, Envelope::Ok(response) if matches!(*response, Response::ProjectSave(_)))
    );
    assert!(!side_mdat.exists());
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    events.release_progress();
    assert!(matches!(
        events.wait(job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            error: None,
            ..
        }
    ));
    assert!(side_mdat.is_file());
    secs_mdat::load_mdat_project(&fs::read(&side_mdat).unwrap(), &secs_jos::Limits::default())
        .unwrap();

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_side_mdat_job_reads_sources_from_the_snapshot_not_the_disk() {
    let root = temp_root("side-mdat-source-snapshot");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let store = secs_project::MeasurementStore::open(&path).unwrap();
    let raw = store
        .records()
        .into_iter()
        .flat_map(|record| record.ir.into_values())
        .find(|raw| !raw.is_empty())
        .unwrap();
    let wav = if Path::new(&raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        path.parent().unwrap().join(raw)
    };
    secs_project::save_source(
        &wav,
        Some(&secs_mdat::SourceInfo {
            strings: vec![("outputName".into(), "snapshot source".into())],
            ..Default::default()
        }),
    )
    .unwrap();

    let service = deterministic_service();
    open(&service, &path);
    secs_project::save_source(
        &wav,
        Some(&secs_mdat::SourceInfo {
            strings: vec![("outputName".into(), "disk source".into())],
            ..Default::default()
        }),
    )
    .unwrap();
    let events = Arc::new(Events::default());
    let response = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    assert!(matches!(response, Envelope::Ok(_)));
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    assert!(matches!(
        events.wait(job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    let loaded = secs_mdat::load_mdat_project(
        &fs::read(path.with_extension("mdat")).unwrap(),
        &secs_jos::Limits::default(),
    )
    .unwrap();
    assert!(loaded.measurements.iter().any(|measurement| {
        measurement
            .source_info
            .strings
            .iter()
            .any(|(name, value)| name == "outputName" && value == "snapshot source")
    }));
    assert!(loaded.measurements.iter().all(|measurement| {
        measurement
            .source_info
            .strings
            .iter()
            .all(|(name, value)| name != "outputName" || value != "disk source")
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn side_mdat_failure_is_a_job_error_not_a_silent_save_td149() {
    let root = temp_root("side-mdat-job-error");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let side_mdat = path.with_extension("mdat");
    fs::create_dir(&side_mdat).unwrap();
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());

    let response = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    assert!(
        matches!(response, Envelope::Ok(response) if matches!(*response, Response::ProjectSave(_)))
    );
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    let JobEvent::Finished {
        status: JobStatus::Failed,
        error: Some(error),
        ..
    } = events.wait(job, Duration::from_secs(30))
    else {
        panic!("side mdat failure must finish the export job with an error")
    };
    assert!(
        error
            .detail
            .contains(&side_mdat.to_string_lossy().into_owned())
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn side_mdat_job_reports_sidecar_warnings_td183() {
    let root = temp_root("side-mdat-sidecar-warning");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let store = MeasurementStore::open(&path).unwrap();
    let record = store.records().into_iter().next().unwrap();
    let role = record.ir.keys().next().unwrap();
    let wav = path.parent().unwrap().join(&record.ir[role]);
    let sidecar = secs_project::source_path(&wav);
    secs_project::save_source(&wav, Some(&secs_mdat::SourceInfo::default())).unwrap();
    fs::remove_file(&sidecar).unwrap();
    fs::create_dir(&sidecar).unwrap();
    let service = deterministic_service();
    open(&service, &path);
    let events = Arc::new(Events::default());

    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    assert!(matches!(
        events.wait(job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    let expected_entry = sidecar
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned();
    let values = lock(&events.values);
    assert!(
        values.iter().any(|event| {
            matches!(event, JobEvent::Log { code, params, .. }
                if code == "measurement.source.skipped"
                    && params.get("entry") == Some(&expected_entry))
        }),
        "events: {values:?}; sidecar: {}",
        sidecar.display()
    );
    drop(values);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn successive_saves_to_two_paths_write_both_side_mdats_td149() {
    let root = temp_root("side-mdat-two-paths");
    fs::create_dir_all(&root).unwrap();
    let (source, _) = write_case(&root, "room");
    let first = root.join("first.secs");
    let second = root.join("second.secs");
    let first_mdat = first.with_extension("mdat");
    let second_mdat = second.with_extension("mdat");
    let (blocked_tx, blocked_rx) = std::sync::mpsc::sync_channel(1);
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let uuid_calls = Arc::new(AtomicU64::new(0));
    let worker_gate = gate.clone();
    let worker_calls = uuid_calls.clone();
    let service = Service::new(Deps {
        uuid: Arc::new(Mutex::new(Box::new(move || {
            let call = worker_calls.fetch_add(1, Ordering::Relaxed);
            if call == 0 {
                blocked_tx.send(()).unwrap();
                let mut released = lock(&worker_gate.0);
                while !*released {
                    released = worker_gate.1.wait(released).unwrap();
                }
            }
            Ok(0x1234_5678_9abc_def0 + u128::from(call))
        }))),
        unix_ms: Arc::new(|| FIXED_MS),
        ..Deps::default()
    });
    open(&service, &source);
    let events = Arc::new(Events::default());

    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(first.to_string_lossy().into_owned()),
        }),
        events.clone(),
    );
    blocked_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(second.to_string_lossy().into_owned()),
        }),
        events.clone(),
    );
    *lock(&gate.0) = true;
    gate.1.notify_all();
    for job in events.started_jobs(JobKind::Export) {
        assert!(matches!(
            events.wait(job, Duration::from_secs(30)),
            JobEvent::Finished {
                status: JobStatus::Succeeded,
                error: None,
                ..
            }
        ));
    }
    assert!(events.try_wait(job, Duration::from_secs(1)).is_some());
    assert!(first_mdat.is_file(), "first side mdat was not written");
    assert!(second_mdat.is_file(), "second side mdat was not written");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn saves_while_a_side_mdat_job_runs_coalesce_into_one_job_td149() {
    let root = temp_root("side-mdat-coalesce");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let side_mdat = path.with_extension("mdat");
    let mut store = MeasurementStore::open(&path).unwrap();
    let record = store.records().into_iter().next().unwrap();
    let role = record.ir.keys().next().unwrap().clone();
    let raw = PathBuf::from(record.ir[&role].clone());
    let wav = if raw.is_absolute() {
        raw
    } else {
        path.parent().unwrap().join(raw)
    };
    let (mut samples, sample_rate) = store.ir_of(&role, record.pos).unwrap();
    let changed_sample = if samples[0] == 0.25 { 0.5 } else { 0.25 };

    let (blocked_tx, blocked_rx) = std::sync::mpsc::sync_channel(1);
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let uuid_calls = Arc::new(AtomicU64::new(0));
    let worker_gate = gate.clone();
    let worker_calls = uuid_calls.clone();
    let service = Service::new(Deps {
        uuid: Arc::new(Mutex::new(Box::new(move || {
            let call = worker_calls.fetch_add(1, Ordering::Relaxed);
            if call == 0 {
                blocked_tx.send(()).unwrap();
                let mut released = lock(&worker_gate.0);
                while !*released {
                    released = worker_gate.1.wait(released).unwrap();
                }
            }
            Ok(0x1234_5678_9abc_def0 + u128::from(call))
        }))),
        unix_ms: Arc::new(|| FIXED_MS),
        ..Deps::default()
    });
    open(&service, &path);
    let events = Arc::new(Events::default());

    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    blocked_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    samples[0] = changed_sample;
    fs::write(&wav, secs_project::write_wav_float32(&samples, sample_rate)).unwrap();
    for _ in 0..2 {
        call_ok(
            &service,
            Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
            events.clone(),
        );
    }
    let Response::Jobs(running) = call_ok(&service, Request::Jobs, Arc::new(NoEvents)) else {
        panic!("jobs request must return snapshots")
    };
    let running_side_mdats = running
        .iter()
        .filter(|snapshot| snapshot.kind == JobKind::Export && !snapshot.status.is_terminal())
        .count();
    let started = events.started_jobs(JobKind::Export);
    *lock(&gate.0) = true;
    gate.1.notify_all();
    for job in &started {
        events.wait(*job, Duration::from_secs(30));
    }

    assert_eq!(
        started.len(),
        1,
        "side mdat saves started more than one job"
    );
    assert_eq!(
        running_side_mdats, 1,
        "more than one side mdat job was running"
    );
    let loaded =
        secs_mdat::load_mdat_project(&fs::read(&side_mdat).unwrap(), &secs_jos::Limits::default())
            .unwrap();
    let measurement = loaded
        .measurements
        .iter()
        .find(|measurement| measurement.title == format!("{role}_P{}", record.pos))
        .unwrap();
    let (saved, _) = loaded.ir_provider.get_ir(&measurement.uuid).unwrap();
    assert!((saved[0] - f64::from(changed_sample)).abs() < 1e-6);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn shutdown_waits_for_a_running_side_mdat_job_td149() {
    let root = temp_root("side-mdat-shutdown");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let side_mdat = path.with_extension("mdat");
    let (blocked_tx, blocked_rx) = std::sync::mpsc::sync_channel(1);
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let worker_gate = gate.clone();
    let first = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let worker_first = first.clone();
    let service = Arc::new(Service::new(Deps {
        uuid: Arc::new(Mutex::new(Box::new(move || {
            if worker_first.swap(false, Ordering::Relaxed) {
                blocked_tx.send(()).unwrap();
                let mut released = lock(&worker_gate.0);
                while !*released {
                    released = worker_gate.1.wait(released).unwrap();
                }
            }
            Ok(0x1234_5678_9abc_def0)
        }))),
        unix_ms: Arc::new(|| FIXED_MS),
        ..Deps::default()
    }));
    open(&service, &path);
    let events = Arc::new(Events::default());
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    blocked_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));

    let shutdown_service = service.clone();
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::sync_channel(1);
    let shutdown = std::thread::spawn(move || {
        shutdown_tx
            .send(shutdown_service.shutdown(Duration::from_secs(5)))
            .unwrap();
    });
    let returned_early = shutdown_rx.recv_timeout(Duration::from_millis(200)).ok();
    *lock(&gate.0) = true;
    gate.1.notify_all();
    let completed = returned_early.unwrap_or_else(|| {
        shutdown_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("shutdown did not finish after the side mdat job")
    });
    shutdown.join().unwrap();

    assert!(
        returned_early.is_none(),
        "shutdown returned before the side mdat job"
    );
    assert!(completed);
    assert!(matches!(
        events.wait(job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            error: None,
            ..
        }
    ));
    assert!(side_mdat.is_file());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn side_mdat_failure_reaches_the_log_sink_td151() {
    let root = temp_root("side-mdat-log");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let side_mdat = path.with_extension("mdat");
    fs::create_dir(&side_mdat).unwrap();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink_lines = lines.clone();
    let service = Service::new(Deps {
        log: Arc::new(move |level, scope, line| {
            lock(&sink_lines).push((level, scope.to_owned(), line.to_owned()));
        }),
        ..Deps::default()
    });
    open(&service, &path);

    let events = Arc::new(Events::default());
    let response = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    assert!(
        matches!(response, Envelope::Ok(response) if matches!(*response, Response::ProjectSave(_)))
    );
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    assert!(matches!(
        events.wait(job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Failed,
            error: Some(_),
            ..
        }
    ));
    assert!(lock(&lines).is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn low_protect_edits_persist_and_mark_results_stale() {
    let root = temp_root("low-protect");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let initial = service();
    open(&initial, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&initial, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert!(!read_results(&initial).calibration.unwrap().stale);
    call_ok(
        &initial,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::LowProtect { on: true },
        }),
        Arc::new(NoEvents),
    );
    assert!(read_results(&initial).calibration.unwrap().stale);
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
        panic!()
    };
    assert!(settings.low_protect);
    assert!(read_results(&reopened).calibration.unwrap().stale);
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::LowProtect { on: false },
        }),
        Arc::new(NoEvents),
    );
    assert!(!read_results(&reopened).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dip_limit_edits_persist_and_mark_results_stale() {
    let root = temp_root("dip-limit");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let initial = service();
    open(&initial, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&initial, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert!(!read_results(&initial).calibration.unwrap().stale);
    call_ok(
        &initial,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::DipLimit { on: true },
        }),
        Arc::new(NoEvents),
    );
    assert!(read_results(&initial).calibration.unwrap().stale);
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
        panic!()
    };
    assert!(settings.dip_limit);
    assert!(read_results(&reopened).calibration.unwrap().stale);
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::DipLimit { on: false },
        }),
        Arc::new(NoEvents),
    );
    assert!(!read_results(&reopened).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lf_align_and_mode_decay_edits_persist_and_mark_results_stale() {
    use secs_types::settings_edit::SettingEdit;

    let root = temp_root("lf-align-mode-decay");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let initial = service();
    open(&initial, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&initial, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert!(!read_results(&initial).calibration.unwrap().stale);

    for edit in [
        SettingEdit::LfAlign { on: true },
        SettingEdit::ModeDecay { on: true },
        SettingEdit::HfPhase { on: true },
    ] {
        call_ok(
            &initial,
            Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
                edit,
            }),
            Arc::new(NoEvents),
        );
    }
    let summary = read_results(&initial).calibration.unwrap();
    assert!(summary.stale);
    assert_eq!(summary.stale_reason.as_deref(), Some("settings"));
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
        panic!()
    };
    assert!(settings.lf_align);
    assert!(settings.mode_decay);
    assert!(settings.hf_phase);
    assert!(read_results(&reopened).calibration.unwrap().stale);

    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::LfAlign { on: false },
        }),
        Arc::new(NoEvents),
    );
    assert!(read_results(&reopened).calibration.unwrap().stale);
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::ModeDecay { on: false },
        }),
        Arc::new(NoEvents),
    );
    assert!(read_results(&reopened).calibration.unwrap().stale);
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::HfPhase { on: false },
        }),
        Arc::new(NoEvents),
    );
    assert!(!read_results(&reopened).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn input_hp_edits_validate_persist_and_mark_results_stale() {
    use secs_types::settings_edit::SettingEdit;

    let root = temp_root("input-hp");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let initial = service();
    open(&initial, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&initial, events.clone(), settings.clone());
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert!(!read_results(&initial).calibration.unwrap().stale);

    let Response::CalibrationSettingsEdit(enabled) = call_ok(
        &initial,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::InputHp { hz: Some(27.0) },
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(enabled.input_hp_hz, Some(27.0));
    assert_ne!(enabled.engine_key(), settings.engine_key());
    let summary = read_results(&initial).calibration.unwrap();
    assert!(summary.stale);
    assert_eq!(summary.stale_reason.as_deref(), Some("settings"));
    call_ok(
        &initial,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );

    let reopened = service();
    open(&reopened, &path);
    let Response::CalibrationSettingsGet(stored) = call_ok(
        &reopened,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(stored.input_hp_hz, Some(27.0));
    assert!(read_results(&reopened).calibration.unwrap().stale);

    let Response::CalibrationSettingsEdit(disabled) = call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::InputHp { hz: None },
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(disabled.input_hp_hz, None);
    assert_eq!(disabled.engine_key(), settings.engine_key());
    assert!(!read_results(&reopened).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rc_band_edits_store_sorted_clear_change_keys_and_mark_results_stale() {
    use secs_types::{calibration::RcBand, settings_edit::SettingEdit};

    let root = temp_root("rc-bands-edit");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(job, Duration::from_secs(120)));
    let baseline_key = settings.engine_key();

    let Response::CalibrationSettingsEdit(edited) = call_ok(
        &service,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::RcBands {
                bands: vec![
                    RcBand {
                        lo_hz: 15_000.0,
                        hi_hz: 20_000.0,
                    },
                    RcBand {
                        lo_hz: 1_000.0,
                        hi_hz: 2_000.0,
                    },
                ],
            },
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(
        edited
            .rc_bands
            .iter()
            .map(|band| band.lo_hz)
            .collect::<Vec<_>>(),
        [1_000.0, 15_000.0]
    );
    assert_ne!(edited.engine_key(), baseline_key);
    let summary = read_results(&service).calibration.unwrap();
    assert!(summary.stale);
    assert_eq!(summary.stale_reason.as_deref(), Some("settings"));

    let Response::CalibrationSettingsEdit(cleared) = call_ok(
        &service,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::RcBands { bands: Vec::new() },
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert!(cleared.rc_bands.is_empty());
    assert_eq!(cleared.engine_key(), baseline_key);
    assert!(!read_results(&service).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
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
fn tutorial_folder_lives_under_app_data_and_refuses_an_existing_path_td210() {
    let root = temp_root("tutorial-app-data");
    let app_data = root.join("app-data");
    fs::create_dir_all(&root).unwrap();
    let service = Service::new(Deps {
        app_data_path: app_data.clone(),
        ..Deps::default()
    });
    let create = |name: &str| {
        call_ok(
            &service,
            Request::ProjectNew(ProjectNewParams {
                path: root.join(name).to_string_lossy().into_owned(),
                preset: "2.0".into(),
                sub_count: 0,
                sample: false,
                tutorial: true,
            }),
            Arc::new(NoEvents),
        );
        let mut folders = fs::read_dir(app_data.join("tutorial"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        folders.sort();
        assert_eq!(folders.len(), 1);
        folders.remove(0)
    };

    let first = create("first.secs");
    assert!(first.starts_with(&app_data));
    assert_ne!(first.parent(), Some(std::env::temp_dir().as_path()));
    let second = create("second.secs");
    assert!(second.starts_with(&app_data));
    assert_ne!(second, first);
    assert!(!first.exists());

    drop(service);
    assert!(!second.exists());
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
    start_calibration_in_mode(service, events, settings, false)
}

fn start_calibration_in_mode(
    service: &Service,
    events: Arc<dyn EventSink>,
    settings: CalibrationSettings,
    background: bool,
) -> JobId {
    let Response::Calibrate(started) = call_ok(
        service,
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            background,
            ..Default::default()
        }),
        events,
    ) else {
        panic!("calibration must start");
    };
    started.job
}

#[test]
fn background_and_foreground_calibration_summaries_are_bit_identical() {
    let root = temp_root("background-determinism");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let service = deterministic_service();
    open(&service, &path);

    let foreground_events = Arc::new(Events::default());
    let foreground_job =
        start_calibration_in_mode(&service, foreground_events.clone(), settings.clone(), false);
    finished_result(foreground_events.wait(foreground_job, Duration::from_secs(120)));
    let foreground = read_results(&service);

    let background_events = Arc::new(Events::default());
    let background_job =
        start_calibration_in_mode(&service, background_events.clone(), settings, true);
    finished_result(background_events.wait(background_job, Duration::from_secs(120)));
    let background = read_results(&service);

    assert_eq!(
        serde_json::to_vec(&background).unwrap(),
        serde_json::to_vec(&foreground).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
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

#[derive(Debug)]
struct CancelFirstExportProgress {
    service: Arc<Service>,
    events: Arc<Events>,
    requested: std::sync::atomic::AtomicBool,
}

impl EventSink for CancelFirstExportProgress {
    fn send(&self, event: JobEvent) {
        self.events.send(event.clone());
        if let JobEvent::Progress { job, .. } = event
            && !self.requested.swap(true, Ordering::SeqCst)
        {
            assert!(matches!(
                self.service.call(
                    Request::JobCancel(JobCancelParams { job }),
                    Arc::new(NoEvents),
                ),
                Envelope::Ok(_)
            ));
        }
    }
}

#[test]
fn camilla_export_cancelled_before_commit_writes_nothing_td196() {
    let root = temp_root("cancel-camilla-prepare");
    fs::create_dir_all(&root).unwrap();
    let destination = root.join("export");
    fs::create_dir(&destination).unwrap();
    let (path, settings) = write_case(&root, "room");
    let service = Arc::new(deterministic_service());
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
        panic!();
    };
    let mut fields = camilla.fields;
    fields.output_rate = OutputRateChoice::Fixed(96_000);
    call_ok(
        &service,
        Request::DspSet(DspSetParams {
            preset: None,
            fields: Some(fields),
        }),
        Arc::new(NoEvents),
    );
    let events = Arc::new(Events::default());
    let sink = Arc::new(CancelFirstExportProgress {
        service: service.clone(),
        events: events.clone(),
        requested: std::sync::atomic::AtomicBool::new(false),
    });
    let Response::Export(started) = call_ok(
        &service,
        Request::Export(ExportParams {
            kind: ExportKind::CamillaDsp,
            folder: destination.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        sink.clone(),
    ) else {
        panic!();
    };

    assert!(matches!(
        events.wait(started.job, Duration::from_secs(180)),
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
    assert!(sink.requested.load(Ordering::SeqCst));
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
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
fn bootstrap_carries_the_settings_notice() {
    let root = temp_root("bootstrap-settings-notice");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    fs::write(&path, b"{not json").unwrap();
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    let Response::Bootstrap(info) = call_ok(&service, Request::Bootstrap, Arc::new(NoEvents))
    else {
        panic!("bootstrap response must match the request")
    };
    let notice = info.settings_notice.expect("settings notice is absent");
    assert_eq!(notice.code, "app_settings_unreadable");
    assert_eq!(
        notice.params.get("path").map(String::as_str),
        Some(path.to_string_lossy().as_ref())
    );
    fs::remove_dir_all(root).unwrap();
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
    assert_eq!(info.log_dir, None);

    let root = temp_root("bootstrap-log-dir");
    let settings_path = root.join("settings.json");
    let settings_only = Service::new(Deps {
        app_settings_path: Some(settings_path.clone()),
        ..Deps::default()
    });
    let Response::Bootstrap(info) = call_ok(&settings_only, Request::Bootstrap, Arc::new(NoEvents))
    else {
        panic!("bootstrap response must match the request");
    };
    assert_eq!(info.log_dir, None);

    let log_dir = root.join("actual-logs");
    let service = Service::new(Deps {
        app_settings_path: Some(settings_path),
        log_dir: Some(log_dir.clone()),
        ..Deps::default()
    });
    let Response::Bootstrap(info) = call_ok(&service, Request::Bootstrap, Arc::new(NoEvents))
    else {
        panic!("bootstrap response must match the request");
    };
    assert_eq!(info.log_dir, Some(log_dir.to_string_lossy().into_owned()));
}

#[test]
fn recommend_xo_accepts_absent_null_and_background_params() {
    let service = service();
    for request in [
        serde_json::json!({"method": "recommend_xo"}),
        serde_json::json!({"method": "recommend_xo", "params": null}),
        serde_json::json!({"method": "recommend_xo", "params": {"background": true}}),
    ] {
        let Envelope::Err(error) = service.call_value(request, Arc::new(NoEvents)) else {
            panic!("recommend_xo without a project must fail");
        };
        assert_eq!(error.code, ErrorCode::ProjectNotOpen);
    }
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
        .notices
        .iter()
        .find(|notice| notice.code == "measurement_resampled")
        .expect("converted measurements must be visible in the calculation notes");
    assert_eq!(notice.params["count"], "1");
    assert_eq!(notice.params["target_hz"], "48000");
    let labels: serde_json::Value = cached_field(&path, "labels");
    assert_eq!(labels["sample_rate_policy"], "resample");
    let xo_events = Arc::new(Events::default());
    let Response::RecommendXo(xo) =
        call_ok(&service, Request::RecommendXo(None), xo_events.clone())
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
    let main_position = direct.design.main_position();
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
    let position = direct.design.main_position();
    let role = direct.design.main_roles().next().unwrap().to_owned();
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
fn relative_export_folder_is_rejected_td211() {
    let root = temp_root("relative-export");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("room.secs");
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
    let relative = format!(
        "relative-export-{}-{}",
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    );
    let relative_path = PathBuf::from(&relative);
    let _ = fs::remove_dir_all(&relative_path);
    let events = Arc::new(Events::default());

    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::CamillaDsp,
            folder: relative.clone(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!("a relative export folder must fail before a job starts")
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("relative_folder")
    );
    assert_eq!(
        error.params.get("field").map(String::as_str),
        Some("folder")
    );
    assert!(lock(&events.values).is_empty());
    assert!(!relative_path.exists());
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
            .notices
            .iter()
            .all(|notice| notice.code != "measurement_resampled")
    );
    assert_eq!(results.calibration.as_ref().unwrap().main_roles.len(), 11);
    let direct = secs_engine_testkit::production_case("dolby", 1);
    assert_eq!(cached_chain(&path), *direct.design.signal_chain());
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
        call_ok(&service, Request::RecommendXo(None), xo_events.clone())
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
        inverted: &[],
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
                        direct.design.latency_ms(),
                        direct.design.rc_hi,
                        None,
                    ),
                    latency_ms: direct.design.latency_ms(),
                };
                let project_name = {
                    let store = secs_project::MeasurementStore::open(&path).unwrap();
                    (!store.project().name.trim().is_empty())
                        .then(|| store.project().name.trim().to_owned())
                };
                let design_id = secs_graph::design::design_id(
                    direct.design.signal_chain(),
                    None,
                    secs_graph::LfeChoice::DOLBY,
                )
                .unwrap();
                let direct_files = secs_camilla::export_with_options(
                    direct.design.signal_chain(),
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
                    chain: direct.design.signal_chain(),
                    mimo: None,
                    mode: &export_mode(&settings),
                    note: &secs_camilla::note::eqapo_note(
                        &export_mode(&settings),
                        direct.design.latency_ms(),
                        direct.design.rc_hi,
                        None,
                    ),
                    label: &label,
                    latency_ms: direct.design.latency_ms(),
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
    let secs_types::notes::Note::RcLimit {
        hz: rc_hi_hz,
        auto: rc_auto,
        schroeder_hz,
        ..
    } = summary
        .notes
        .iter()
        .find(|note| matches!(note, secs_types::notes::Note::RcLimit { .. }))
        .unwrap()
    else {
        unreachable!("matched the RC limit variant")
    };
    let rc_pieces = secs_export::rc_pieces_from_values(
        *rc_hi_hz,
        *rc_auto,
        *schroeder_hz,
        settings.room_volume_m3,
        false,
    );
    let label = settings.label();
    let direct_run = secs_filters::FilterRun {
        chain: &chain,
        mimo: None,
        mode: "mixed",
        note: &secs_camilla::note::eqapo_note("mixed", summary.latency_ms, *rc_hi_hz, None),
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

    // An explicit folder is the named bundle's base: neither the project's
    // export folder nor the host's home directory (not implemented here) is read.
    let explicit = root.join("explicit");
    let events = Arc::new(Events::default());
    let Response::Export(started) = call_ok(
        &service,
        Request::Export(ExportParams {
            kind: ExportKind::CamillaDsp,
            folder: explicit.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: Some("Cinema".into()),
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!("a named export into an explicit folder must start");
    };
    let JobResult::Export(result) =
        finished_result(events.wait(started.job, Duration::from_secs(120)))
    else {
        panic!("a named export into an explicit folder must finish");
    };
    assert_eq!(Path::new(&result.folder).parent(), Some(explicit.as_path()));
    assert!(explicit.join("designs.json").is_file());

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
    let reverse_service = service();
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

    let reverse_root = root.join("case.irs");
    fs::create_dir(&reverse_root).unwrap();
    let reverse_source = reverse_root.join("room.secs");
    call_ok(
        &reverse_service,
        Request::ProjectNew(ProjectNewParams {
            path: reverse_source.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let reverse_source_irs = reverse_source.with_extension("irs");
    fs::create_dir_all(&reverse_source_irs).unwrap();
    fs::write(reverse_root.join("keep.txt"), b"keep").unwrap();
    fs::write(reverse_source_irs.join("marker"), b"reverse").unwrap();
    let reverse_destination = root.join("case.secs");
    let Envelope::Err(error) = reverse_service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(reverse_destination.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("save as whose measurement folder contains the source must fail");
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert!(reverse_source.is_file());
    assert_eq!(fs::read(reverse_root.join("keep.txt")).unwrap(), b"keep");
    assert_eq!(
        fs::read(reverse_source_irs.join("marker")).unwrap(),
        b"reverse"
    );
    assert!(!reverse_destination.exists());

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
fn extension_only_save_as_without_a_source_measurement_folder_succeeds_td04() {
    let root = temp_root("save-as-extension-only");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("room.secs");
    let destination = root.join("room.backup");
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
    assert!(!source.with_extension("irs").exists());

    let Response::ProjectSave(info) = call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(destination.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!();
    };
    assert_eq!(info.path, destination.to_string_lossy());
    assert!(source.is_file());
    assert!(destination.is_file());

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
fn polarity_setting_marks_results_stale_and_flips_the_engine_input() {
    let root = temp_root("speaker-polarity-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let service = deterministic_service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(calibration, Duration::from_secs(120)));

    let Response::ConfigSetSpeaker(config) = call_ok(
        &service,
        Request::ConfigSetSpeaker(secs_types::ipc::ConfigSetSpeakerParams {
            index: 0,
            role: None,
            size: None,
            band_index: Some(0),
            out: None,
            pol: Some("-".into()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("speaker polarity must change")
    };
    assert_eq!(config.speakers[0].bands[0].pol, "-");
    let stale = read_results(&service).calibration.unwrap();
    assert!(stale.stale);
    assert_eq!(stale.stale_reason.as_deref(), Some("config"));

    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(calibration, Duration::from_secs(120)));
    assert!(!read_results(&service).calibration.unwrap().stale);
    let export = export_and_wait(
        &service,
        ExportKind::EqualizerApo,
        &root.join("export"),
        Arc::new(Events::default()),
    );
    let body = export
        .files
        .iter()
        .map(fs::read_to_string)
        .filter_map(Result::ok)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(body.contains("Copy: VML=-1.0*VML"));
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
fn calibrate_with_mimo_matches_separate_runs_and_orders_progress() {
    let root = temp_root("calibrate-with-mimo");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = deterministic_service();
    open(&service, &path);

    let combined_events = Arc::new(Events::default());
    let Response::Calibrate(started) = call_ok(
        &service,
        Request::Calibrate(CalibrateParams {
            settings: Some(settings.clone()),
            mimo: true,
            ..Default::default()
        }),
        combined_events.clone(),
    ) else {
        panic!("combined calibration must start")
    };
    assert_eq!(
        finished_result(combined_events.wait(started.job, Duration::from_secs(120))),
        JobResult::Calibration
    );
    let stages: Vec<_> = lock(&combined_events.values)
        .iter()
        .filter_map(|event| match event {
            JobEvent::Progress { stage, .. } => Some(stage.clone()),
            _ => None,
        })
        .collect();
    let calibration_stage = stages
        .iter()
        .position(|stage| *stage == "engine.design")
        .expect("calibration progress");
    let mimo_stage = stages
        .iter()
        .position(|stage| *stage == "engine.mimo")
        .expect("MIMO progress");
    assert!(calibration_stage < mimo_stage);
    let combined = read_results(&service);
    assert!(!combined.calibration.as_ref().unwrap().stale);
    assert!(!combined.mimo.as_ref().unwrap().stale);

    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings.clone());
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));
    let mimo_events = Arc::new(Events::default());
    let Response::Mimo(mimo) = call_ok(
        &service,
        Request::Mimo(MimoParams {
            settings: Some(settings),
        }),
        mimo_events.clone(),
    ) else {
        panic!("separate MIMO must start")
    };
    finished_result(mimo_events.wait(mimo.job, Duration::from_secs(120)));
    let separate = read_results(&service);
    assert_eq!(combined.calibration, separate.calibration);
    assert_eq!(combined.mimo, separate.mimo);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn calibrate_with_mimo_refuses_disabled_and_too_few_positions_before_starting() {
    let root = temp_root("calibrate-mimo-preconditions");
    fs::create_dir_all(&root).unwrap();

    let (disabled_path, disabled_settings) = write_case(&root, "room_dba");
    let disabled = service();
    open(&disabled, &disabled_path);
    let Envelope::Err(error) = disabled.call(
        Request::Calibrate(CalibrateParams {
            settings: Some(disabled_settings),
            mimo: true,
            ..Default::default()
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("disabled MIMO must be refused")
    };
    assert_eq!(error.code, ErrorCode::EngineInput);
    assert!(error.params.is_empty());
    assert_eq!(error.detail, "invalid input: MIMO is disabled");
    let Response::Jobs(jobs) = call_ok(&disabled, Request::Jobs, Arc::new(NoEvents)) else {
        panic!()
    };
    assert!(jobs.is_empty());

    let few_root = root.join("few");
    fs::create_dir_all(&few_root).unwrap();
    let (few_path, mut settings) = write_case(&few_root, "room_dba");
    let mut store = secs_project::MeasurementStore::open(&few_path).unwrap();
    let secs_project::PyJson::Array(measurements) = &mut store.project_mut().measurements else {
        panic!("measurements must be an array")
    };
    measurements.truncate(2);
    store.save().unwrap();
    settings.mimo_on = true;
    let few = service();
    open(&few, &few_path);
    let Envelope::Err(error) = few.call(
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            mimo: true,
            ..Default::default()
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("two positions must be refused")
    };
    assert_eq!(error.code, ErrorCode::EngineInput);
    assert!(error.params.is_empty());
    assert!(error.detail.contains("MIMO 는 3곳 이상 필요"));
    let Response::Jobs(jobs) = call_ok(&few, Request::Jobs, Arc::new(NoEvents)) else {
        panic!()
    };
    assert!(jobs.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[derive(Debug)]
struct CancelOnMimo {
    service: Arc<Service>,
    events: Events,
    requested: std::sync::atomic::AtomicBool,
}

impl EventSink for CancelOnMimo {
    fn send(&self, event: JobEvent) {
        self.events.send(event.clone());
        if matches!(&event, JobEvent::Progress { stage, done: 0, .. } if stage == "engine.mimo")
            && !self.requested.swap(true, Ordering::SeqCst)
        {
            let job = match event {
                JobEvent::Progress { job, .. } => job,
                _ => unreachable!(),
            };
            let response = self.service.call(
                Request::JobCancel(JobCancelParams { job }),
                Arc::new(NoEvents),
            );
            assert!(matches!(response, Envelope::Ok(_)));
        }
    }
}

#[test]
fn cancelling_calibrate_during_mimo_keeps_only_the_new_calibration() {
    let root = temp_root("cancel-calibrate-mimo");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = Arc::new(service());
    open(&service, &path);
    let events = Arc::new(CancelOnMimo {
        service: service.clone(),
        events: Events::default(),
        requested: std::sync::atomic::AtomicBool::new(false),
    });
    let Response::Calibrate(started) = call_ok(
        &service,
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            mimo: true,
            ..Default::default()
        }),
        events.clone(),
    ) else {
        panic!("combined calibration must start")
    };
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(matches!(
        finished,
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
    assert!(events.requested.load(Ordering::SeqCst));
    let results = read_results(&service);
    assert!(!results.calibration.unwrap().stale);
    assert!(results.mimo.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mimo_failure_after_combined_calibration_keeps_calibration_and_marks_stage() {
    let root = temp_root("failed-calibrate-mimo");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    let mut store = secs_project::MeasurementStore::open(&path).unwrap();
    store
        .project_mut()
        .config
        .speakers
        .retain(|speaker| speaker.role == "L");
    store.project_mut().config.preset = "custom".into();
    store.save().unwrap();
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let Response::Calibrate(started) = call_ok(
        &service,
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            mimo: true,
            ..Default::default()
        }),
        events.clone(),
    ) else {
        panic!("combined calibration must start")
    };
    let JobEvent::Finished {
        status,
        result,
        error: Some(error),
        ..
    } = events.wait(started.job, Duration::from_secs(120))
    else {
        panic!("combined job must fail")
    };
    assert_eq!(status, JobStatus::Failed);
    assert!(result.is_none());
    assert_eq!(error.code, ErrorCode::EngineInput);
    assert_eq!(error.params.get("stage").map(String::as_str), Some("mimo"));
    let results = read_results(&service);
    assert!(!results.calibration.unwrap().stale);
    assert!(results.mimo.is_none());
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
    let direct_mimo = secs_engine::compute(
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
    let expected_mains = direct.design.main_roles().count();
    let expected_rows = direct.design.positions().count() * expected_mains;
    assert_eq!(calibration_summary.late_energy.len(), expected_rows);
    assert_eq!(summary.scorecard.len(), expected_mains);
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
    let main_position = direct.design.main_position();
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
    cpu: secs_engine::CpuRidgeBackend,
}
impl secs_engine::RidgeBackend for FakeRidge {
    fn factor<'a>(
        &'a self,
        ata: &'a [f64],
        atb: &'a [f64],
        cancel: &secs_compute::CancelToken,
    ) -> Result<Box<dyn secs_engine::ShiftedSystem + 'a>, secs_engine::AccelerationError> {
        if self.fail {
            Err(secs_engine::AccelerationError::failed(
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
                return secs_engine::AccelerationSelection {
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
            secs_engine::AccelerationSelection {
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
                    cpu: secs_engine::CpuRidgeBackend,
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
                    .notices
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
#[derive(Debug, Default)]
struct PausedReadProgress(PausedProgress);

impl EventSink for PausedReadProgress {
    fn send(&self, event: JobEvent) {
        if matches!(&event, JobEvent::Progress { stage, .. } if stage == "read_measurements") {
            let mut gate = lock(&self.0.gate);
            if !gate.0 {
                gate.0 = true;
                self.0.changed.notify_all();
                while !gate.1 {
                    gate = self.0.changed.wait(gate).unwrap();
                }
            }
        }
        self.0.events.send(event);
    }
}

impl PausedReadProgress {
    fn wait_until_paused(&self) {
        self.0.wait_until_paused();
    }

    fn release(&self) {
        self.0.release();
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
fn recommend_xo_preserves_xo_edits_made_while_the_job_runs() {
    for (case, edit) in [
        (
            "xo",
            secs_types::settings_edit::SettingEdit::Xo {
                group: "프론트".into(),
                hz: 177.0,
            },
        ),
        (
            "include",
            secs_types::settings_edit::SettingEdit::XoRecInclude { on: false },
        ),
    ] {
        let root = temp_root(&format!("xo-settings-race-{case}"));
        fs::create_dir_all(&root).unwrap();
        let (path, _) = write_case(&root, "room_dba");
        let service = service();
        open(&service, &path);
        let events = Arc::new(PausedProgress::default());
        let Response::RecommendXo(started) =
            call_ok(&service, Request::RecommendXo(None), events.clone())
        else {
            panic!("XO recommendation must start");
        };
        events.wait_until_paused();
        call_ok(
            &service,
            Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
                edit,
            }),
            Arc::new(NoEvents),
        );
        let Response::CalibrationSettingsGet(edited) = call_ok(
            &service,
            Request::CalibrationSettingsGet,
            Arc::new(NoEvents),
        ) else {
            panic!()
        };
        events.release();
        assert!(matches!(
            finished_result(events.events.wait(started.job, Duration::from_secs(120))),
            JobResult::Xo(_)
        ));
        let Response::CalibrationSettingsGet(current) = call_ok(
            &service,
            Request::CalibrationSettingsGet,
            Arc::new(NoEvents),
        ) else {
            panic!()
        };
        assert_eq!(current.xo, edited.xo, "{case}");
        assert_eq!(current.xo_rec_include, edited.xo_rec_include, "{case}");
        match case {
            "xo" => assert_eq!(
                current
                    .xo
                    .iter()
                    .find(|(group, _)| group == "프론트")
                    .map(|(_, hz)| *hz),
                Some(177.0)
            ),
            "include" => assert!(!current.xo_rec_include),
            _ => unreachable!(),
        }
        assert!(!current.xo_rec.is_empty(), "{case}");
        fs::remove_dir_all(root).unwrap();
    }
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
fn measurement_mdat_reads_outside_session_lock_and_cancels_during_read() {
    let root = temp_root("measurement-export-read");
    fs::create_dir_all(&root).unwrap();
    let (source, _) = write_case(&root, "dolby");
    let service = service();
    open(&service, &source);
    let folder = root.join("measurement-export");
    let events = Arc::new(PausedProgress::default());
    let Response::Export(started) = call_ok(
        &service,
        Request::Export(ExportParams {
            kind: ExportKind::BeforeAfterMdat,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope: Some(MdatScope::Measurements),
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    events.wait_until_paused();
    let Response::ProjectInfo(info) = call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert_eq!(info.path, source.to_string_lossy());
    call_ok(
        &service,
        Request::JobCancel(JobCancelParams { job: started.job }),
        Arc::new(NoEvents),
    );
    events.release();
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(matches!(
        finished,
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
    assert!(!folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn measurement_mdat_rejects_all_originals_that_exceed_one_budget_td130() {
    let root = temp_root("measurement-export-budget");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("large.secs");
    let mut store = MeasurementStore::new(&path, Project::from_preset("2.0").unwrap());
    for (role, position) in [("L", 1), ("R", 2)] {
        store
            .record(
                MeasurementResult {
                    role: role.into(),
                    pos: position,
                    ir: vec![0.0; 786_432],
                    sample_rate: 48_000,
                    t0_s: 0.0,
                    spl_db: None,
                    spl_offset_db: None,
                    snr_db: None,
                    uuid: None,
                    source: Some("test".into()),
                    source_info: None,
                },
                None,
            )
            .unwrap();
    }
    store.save().unwrap();

    let service = Service::new(Deps {
        memory: Arc::new(secs_compute::FixedMemory(64 * 1024 * 1024)),
        ..Deps::default()
    });
    open(&service, &path);
    let events = Arc::new(Events::default());
    let Response::Export(started) = call_ok(
        &service,
        Request::Export(ExportParams {
            kind: ExportKind::BeforeAfterMdat,
            folder: root.join("export").to_string_lossy().into_owned(),
            mdat_scope: Some(MdatScope::Measurements),
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    let first = events.try_wait(started.job, Duration::from_secs(1));
    if first.is_none() {
        call_ok(
            &service,
            Request::JobCancel(JobCancelParams { job: started.job }),
            Arc::new(NoEvents),
        );
        let _ = events.wait(started.job, Duration::from_secs(10));
    }
    assert!(matches!(
        first,
        Some(JobEvent::Finished {
            status: JobStatus::Failed,
            error: Some(ref error),
            ..
        }) if error.params.get("reason").map(String::as_str) == Some("memory_budget")
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_resample_replay_releases_read_memory_before_calibrating_td130() {
    let root = temp_root("cached-resample-memory");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("resample.secs");
    let mut project = Project::from_preset("2.0").unwrap();
    let mut settings = secs_project::calibration_from_project(&project);
    settings.sample_rate_policy = secs_types::calibration::SampleRatePolicy::Resample;
    secs_project::store_calibration(&mut project, &settings);
    let mut store = MeasurementStore::new(&path, project);
    for role in ["L", "R"] {
        let mut ir = vec![0.0; 8192];
        ir[100] = 1.0;
        store
            .record(
                MeasurementResult {
                    role: role.into(),
                    pos: 1,
                    ir,
                    sample_rate: 44_100,
                    t0_s: 0.0,
                    spl_db: None,
                    spl_offset_db: None,
                    snr_db: None,
                    uuid: None,
                    source: Some("test".into()),
                    source_info: None,
                },
                None,
            )
            .unwrap();
    }
    store.save().unwrap();

    let running = service();
    open(&running, &path);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let large_samples = vec![0.0_f32; 5 * 1024 * 1024 / std::mem::size_of::<f32>()];
    fs::write(
        secs_project::ir_file(&path, "L", 1),
        secs_project::write_wav_float32(&large_samples, 44_100),
    )
    .unwrap();

    let replay = Service::new(Deps {
        memory: Arc::new(secs_compute::FixedMemory(64 * 1024 * 1024)),
        ..Deps::default()
    });
    open(&replay, &path);
    let events = Arc::new(Events::default());
    let Response::Export(started) = call_ok(
        &replay,
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("export").to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    let Some(finished) = events.try_wait(started.job, Duration::from_secs(5)) else {
        call_ok(
            &replay,
            Request::JobCancel(JobCancelParams { job: started.job }),
            Arc::new(NoEvents),
        );
        let _ = events.wait(started.job, Duration::from_secs(10));
        panic!("cached resample replay kept the read permit while calibrating");
    };
    assert!(matches!(
        finished,
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_mdat_cancel_after_a_real_read_finishes_cancelled_td130() {
    let root = temp_root("cancel-cached-read");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    let events = Arc::new(PausedReadProgress::default());
    let Response::Export(started) = call_ok(
        &reopened,
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("cancelled-export").to_string_lossy().into_owned(),
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
    // The progress point is after the snapshot read. Removing a source now
    // proves cancellation does not begin before the real WAV read.
    fs::remove_file(secs_project::ir_file(&source, "L", 1)).unwrap();
    call_ok(
        &reopened,
        Request::JobCancel(JobCancelParams { job: started.job }),
        Arc::new(NoEvents),
    );
    events.release();
    let finished = events.0.events.wait(started.job, Duration::from_secs(120));
    assert!(matches!(
        finished,
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
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
fn cached_mdat_export_rejects_changed_calibration_inputs_before_writing() {
    let root = temp_root("edit-cached-export");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    let folder = root.join("export-edited");
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
        Request::TargetApplyPreset(TargetPresetParams {
            preset: "harman".into(),
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
fn a_cached_mdat_export_rejects_inputs_changed_after_planning_td155() {
    let root = temp_root("planned-cached-export");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings.clone());
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    let folder = root.join("export-replanned");
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
        Request::TargetApplyPreset(TargetPresetParams {
            preset: "harman".into(),
        }),
        Arc::new(NoEvents),
    );
    let recalibration_events = Arc::new(Events::default());
    let recalibration = start_calibration(&reopened, recalibration_events.clone(), settings);
    finished_result(recalibration_events.wait(recalibration, Duration::from_secs(120)));

    events.release();
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(
        matches!(finished, JobEvent::Finished { status: JobStatus::Failed, error: Some(ref error), .. }
            if error.code == ErrorCode::StaleInputs)
    );
    assert!(!folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_mdat_export_rejects_changed_sample_rate_policy_td116() {
    let root = temp_root("policy-cached-export");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    let folder = root.join("export-policy-edited");
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
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::SampleRatePolicy {
                policy: secs_types::calibration::SampleRatePolicy::Resample,
            },
        }),
        Arc::new(NoEvents),
    );
    events.release();
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(matches!(
        finished,
        JobEvent::Finished {
            status: JobStatus::Failed,
            error: Some(ref error),
            ..
        } if error.code == ErrorCode::StaleInputs
    ));
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
        Request::RecommendXo(None),
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
fn commit_results_saves_the_cache_outside_the_lock_and_reports_a_failed_save_once() {
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
        has_cache_warning(&calibration.notices),
        "{:?}",
        calibration.notices
    );
    assert!(has_cache_warning(&mimo.notices), "{:?}", mimo.notices);
    assert_eq!(
        calibration
            .notices
            .iter()
            .filter(|notice| notice.code == "cache_save_failed")
            .count(),
        1
    );
    assert_eq!(
        mimo.notices
            .iter()
            .filter(|notice| notice.code == "cache_save_failed")
            .count(),
        1
    );

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
    assert!(!has_cache_warning(&results.calibration.unwrap().notices));
    assert!(!has_cache_warning(&results.mimo.unwrap().notices));
    let cached: secs_types::ipc::CalibrationSummary = cached_field(&path, "calibration");
    assert!(
        cached
            .notices
            .iter()
            .all(|notice| notice.code != "cache_save_failed")
    );
    fs::remove_dir_all(root).unwrap();
}

fn export_mode(settings: &CalibrationSettings) -> String {
    secs_camilla::note::mode_name(settings.mode, settings.ultra_ms)
}

#[test]
fn rc_agreement_calibration_fails_with_typed_position_reason() {
    let root = temp_root("rc-agreement");
    fs::create_dir_all(&root).unwrap();
    let service = service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: root.join("empty.secs").to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let events = Arc::new(Events::default());
    let job = start_calibration(
        &service,
        events.clone(),
        CalibrationSettings {
            rc_hi_basis: secs_types::calibration::RcHiBasis::Agreement,
            ..Default::default()
        },
    );
    let JobEvent::Finished {
        error: Some(error), ..
    } = events.wait(job, Duration::from_secs(30))
    else {
        panic!("agreement must fail without measured positions")
    };
    assert_eq!(error.code, ErrorCode::EngineInput);
    assert_eq!(error.params["reason"], "rc_agreement_positions");
    assert_eq!(error.params["measured"], "0");
    assert_eq!(error.params["required"], "3");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rc_upper_edits_mark_calibration_stale() {
    use secs_types::{calibration::RcHiBasis, settings_edit::SettingEdit};
    let root = temp_root("rc-upper-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(job, Duration::from_secs(120)));
    for edit in [
        SettingEdit::RcHiBasis {
            basis: RcHiBasis::Lenient,
        },
        SettingEdit::RcHiTransition { factor: 2 },
    ] {
        call_ok(
            &service,
            Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
                edit,
            }),
            Arc::new(NoEvents),
        );
        assert!(read_results(&service).calibration.unwrap().stale);
        call_ok(
            &service,
            Request::CalibrationSettingsSet(secs_types::ipc::CalibrationSettingsParams {
                settings: settings.clone(),
            }),
            Arc::new(NoEvents),
        );
        assert!(!read_results(&service).calibration.unwrap().stale);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn typed_recommend_xo_can_ask_for_a_background_job_td157() {
    let request = Request::RecommendXo(Some(secs_types::ipc::RecommendXoParams {
        background: true,
    }));
    let root = temp_root("typed-background-xo");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let Response::RecommendXo(started) = call_ok(&service, request, events.clone()) else {
        panic!("expected recommend_xo response");
    };
    events.wait(started.job, Duration::from_secs(120));
    let values = lock(&events.values);
    let started = values
        .iter()
        .find(|event| matches!(event, JobEvent::Started { .. }))
        .unwrap();
    assert_eq!(
        serde_json::to_value(started).unwrap()["kind"],
        "recommend_xo"
    );
    drop(values);
    fs::remove_dir_all(root).unwrap();
}

/// ADR 0059 and ADR 0051 share this seam: an application effect registered
/// on the service runs once after a saved revision and never after a refusal.
#[test]
fn settings_effect_runs_once_after_a_successful_set_and_never_after_a_refusal() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct CountingEffect(Arc<AtomicUsize>);

    impl SettingsEffect for CountingEffect {
        fn changed(&self, _previous: &AppSettings, _next: &AppSettings) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let root = temp_root("app-settings-observer");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let calls = Arc::new(AtomicUsize::new(0));
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    service.register_settings_effect(Arc::new(CountingEffect(calls.clone())));

    let accepted = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings {
                language: "en".into(),
                ..AppSettings::default()
            },
        }),
        Arc::new(NoEvents),
    );
    assert!(matches!(accepted, Envelope::Ok(_)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let refused = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings {
                last_window_size: Some((0, 720)),
                ..AppSettings::default()
            },
        }),
        Arc::new(NoEvents),
    );
    assert!(matches!(refused, Envelope::Err(_)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    fs::remove_dir_all(root).unwrap();
}
