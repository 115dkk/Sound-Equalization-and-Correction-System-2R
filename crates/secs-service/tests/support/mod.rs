#![forbid(unsafe_code)]
#![allow(dead_code, unused_imports)]

#[path = "../../examples/support/case_project.rs"]
pub(crate) mod case_project;

pub(crate) use std::fs;
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::sync::atomic::{AtomicU64, Ordering};
pub(crate) use std::sync::{Arc, Condvar, Mutex, MutexGuard};
pub(crate) use std::time::{Duration, Instant};

pub(crate) use secs_service::{
    Deps, EventSink, LocalTime, LogLevel, NoEvents, Service, SettingsEffect,
};
pub(crate) use secs_types::ipc::{Envelope, ErrorCode, JobEvent, ProjectNewParams, Request};
pub(crate) use sha2::{Digest, Sha256};

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
pub(crate) use secs_project::{MeasurementResult, MeasurementStore, Project};
pub(crate) use secs_types::calibration::CalibrationSettings;
pub(crate) use secs_types::ipc::{
    AppSettings, AppSettingsParams, CalibrateParams, DspSetParams, ExportKind, ExportParams,
    JobCancelParams, JobId, JobKind, JobResult, JobStatus, MdatScope, MimoParams, OutputRateChoice,
    Response, TargetPresetParams,
};

pub(crate) static TEMP: AtomicU64 = AtomicU64::new(0);

pub(crate) fn service() -> Service {
    Service::new(Deps::default())
}

pub(crate) fn deterministic_service() -> Service {
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

pub(crate) const FIXED_MS: i64 = 1_704_164_645_000;

#[derive(Debug, Default)]
pub(crate) struct Events {
    pub(crate) values: Mutex<Vec<JobEvent>>,
    pub(crate) changed: Condvar,
    pub(crate) hold_progress: Mutex<bool>,
    pub(crate) progress_released: Condvar,
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
    pub(crate) fn holding_progress() -> Self {
        Self {
            hold_progress: Mutex::new(true),
            ..Self::default()
        }
    }

    pub(crate) fn wait_started(&self, kind: JobKind, timeout: Duration) -> JobId {
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

    pub(crate) fn release_progress(&self) {
        *lock(&self.hold_progress) = false;
        self.progress_released.notify_all();
    }

    pub(crate) fn started_jobs(&self, kind: JobKind) -> Vec<JobId> {
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

    pub(crate) fn wait(&self, job: JobId, timeout: Duration) -> JobEvent {
        self.try_wait(job, timeout)
            .unwrap_or_else(|| panic!("job {job:?} did not finish"))
    }

    pub(crate) fn try_wait(&self, job: JobId, timeout: Duration) -> Option<JobEvent> {
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

pub(crate) fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "secs-service-{name}-{}-{}",
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    ))
}

pub(crate) fn call_ok(service: &Service, request: Request, events: Arc<dyn EventSink>) -> Response {
    let Envelope::Ok(response) = service.call(request, events) else {
        panic!("request must succeed");
    };
    *response
}

#[cfg(windows)]
pub(crate) struct ReadDenyAcl {
    pub(crate) path: PathBuf,
    pub(crate) user: String,
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

#[cfg(windows)]
pub(crate) fn check_opening_an_unreadable_project_is_a_permission_error_td208() {
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

pub(crate) fn read_results(service: &Service) -> secs_types::ipc::ResultsSummary {
    let Response::Results(results) = call_ok(service, Request::Results, Arc::new(NoEvents)) else {
        panic!()
    };
    results
}

pub(crate) fn cached_field<T: serde::de::DeserializeOwned>(project: &Path, name: &str) -> T {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(project.with_extension("irs").join("secs_run_cache.json")).unwrap(),
    )
    .unwrap();
    serde_json::from_value(value["results"][name].clone()).unwrap()
}
pub(crate) fn cached_chain(project: &Path) -> secs_types::chain::SignalChain {
    cached_field(project, "calibration_chain")
}
pub(crate) fn cached_paths(project: &Path) -> secs_types::chain::MimoPaths {
    cached_field(project, "mimo_paths")
}

pub(crate) fn write_case(root: &Path, name: &str) -> (PathBuf, CalibrationSettings) {
    case_project::write(root, name)
}

pub(crate) fn stored_titles_and_rates(path: &Path) -> Vec<(u32, String, u32)> {
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

pub(crate) fn open(service: &Service, path: &Path) {
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

pub(crate) fn start_calibration(
    service: &Service,
    events: Arc<dyn EventSink>,
    settings: CalibrationSettings,
) -> JobId {
    start_calibration_in_mode(service, events, settings, false)
}

pub(crate) fn start_calibration_in_mode(
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

pub(crate) fn snapshot_relative_files(root: &Path, files: &[String]) -> Vec<(PathBuf, Vec<u8>)> {
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

pub(crate) fn assert_snapshots_equal(left: &[(PathBuf, Vec<u8>)], right: &[(PathBuf, Vec<u8>)]) {
    assert_eq!(
        left.iter().map(|(path, _)| path).collect::<Vec<_>>(),
        right.iter().map(|(path, _)| path).collect::<Vec<_>>()
    );
    for ((path, left_bytes), (_, right_bytes)) in left.iter().zip(right) {
        assert!(left_bytes == right_bytes, "{} differs", path.display());
    }
}

pub(crate) fn compare_snapshot(
    root: &Path,
    snapshot: &[(PathBuf, Vec<u8>)],
    direct_files: &[PathBuf],
) {
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

pub(crate) fn finished_result(event: JobEvent) -> JobResult {
    match event {
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            result: Some(result),
            ..
        } => *result,
        other => panic!("job must succeed: {other:?}"),
    }
}

pub(crate) fn export_and_wait(
    service: &Service,
    kind: ExportKind,
    folder: &Path,
    events: Arc<Events>,
) -> secs_types::ipc::ExportResult {
    export_scope_and_wait(service, kind, folder, None, events)
}

pub(crate) fn export_scope_and_wait(
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
pub(crate) struct CancelFirstExportProgress {
    pub(crate) service: Arc<Service>,
    pub(crate) events: Arc<Events>,
    pub(crate) requested: std::sync::atomic::AtomicBool,
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

pub(crate) fn calibrate_and_run_mimo(service: &Service, settings: &CalibrationSettings) {
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

pub(crate) fn replace_measurement(service: &Service, role: &str) {
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

pub(crate) fn automatic_camilla_has_mimo(service: &Service, root: &Path) -> bool {
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

#[derive(Debug)]
pub(crate) struct CancelOnMimo {
    pub(crate) service: Arc<Service>,
    pub(crate) events: Events,
    pub(crate) requested: std::sync::atomic::AtomicBool,
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

#[derive(Debug)]
pub(crate) struct FakeRidge {
    pub(crate) fail: bool,
    pub(crate) cpu: secs_engine::CpuRidgeBackend,
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

#[derive(Debug, Default)]
pub(crate) struct PausedProgress {
    pub(crate) events: Events,
    pub(crate) gate: Mutex<(bool, bool)>,
    pub(crate) changed: Condvar,
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
pub(crate) struct PausedReadProgress(pub(crate) PausedProgress);

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
    pub(crate) fn wait_until_paused(&self) {
        self.0.wait_until_paused();
    }

    pub(crate) fn release(&self) {
        self.0.release();
    }
}

impl PausedProgress {
    pub(crate) fn wait_until_paused(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut gate = lock(&self.gate);
        while !gate.0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "worker did not reach progress gate");
            gate = self.changed.wait_timeout(gate, remaining).unwrap().0;
        }
    }
    pub(crate) fn release(&self) {
        lock(&self.gate).1 = true;
        self.changed.notify_all();
    }
}

pub(crate) fn mimo_notes(files: &[String]) -> Vec<String> {
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

pub(crate) fn spl_has_mimo_curve(service: &Service) -> bool {
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

pub(crate) fn support_counts(service: &Service) -> Vec<Option<usize>> {
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

/// A file swapped for a named pipe: whoever reads it waits until
/// [`Fifo::release`] hands over the original bytes, once per call.
#[cfg(unix)]
pub(crate) struct Fifo {
    pub(crate) path: PathBuf,
    pub(crate) bytes: Vec<u8>,
}

#[cfg(unix)]
impl Fifo {
    pub(crate) fn replace(path: &Path) -> Self {
        let bytes = fs::read(path).unwrap();
        fs::remove_file(path).unwrap();
        Self::create(path, bytes)
    }

    pub(crate) fn create(path: &Path, bytes: Vec<u8>) -> Self {
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

    pub(crate) fn release(&self) -> std::thread::JoinHandle<()> {
        let (path, bytes) = (self.path.clone(), self.bytes.clone());
        std::thread::spawn(move || fs::write(path, bytes).unwrap())
    }
}

#[cfg(unix)]
pub(crate) fn call_within(
    service: &Arc<Service>,
    request: Request,
    timeout: Duration,
) -> Option<Envelope> {
    call_with_events(service, request, Arc::new(NoEvents), timeout)
}

#[cfg(unix)]
pub(crate) fn call_with_events(
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

pub(crate) fn has_cache_warning(notes: &[secs_types::ipc::Notice]) -> bool {
    notes.iter().any(|notice| {
        notice.code == "cache_save_failed"
            && notice
                .params
                .get("error")
                .is_some_and(|error| !error.is_empty())
    })
}

pub(crate) fn export_mode(settings: &CalibrationSettings) -> String {
    secs_camilla::note::mode_name(settings.mode, settings.ultra_ms)
}
