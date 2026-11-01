#![forbid(unsafe_code)]

#[path = "../examples/support/case_project.rs"]
mod case_project;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, UNIX_EPOCH};

use secs_service::{Deps, EventSink, Host, LocalTime, NoEvents, Service};
use secs_types::ipc::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct Home(PathBuf);
impl Host for Home {
    fn home_dir(&self) -> Result<PathBuf, IpcError> {
        Ok(self.0.clone())
    }
}

fn call(service: &Service, request: Request) -> Result<Response, IpcError> {
    match service.call(request, Arc::new(NoEvents)) {
        Envelope::Ok(response) => Ok(*response),
        Envelope::Err(error) => Err(error),
    }
}

/// 2024-01-02 03:04:05 in the user's zone.
fn fixed_clock() -> LocalTime {
    LocalTime::from_system(UNIX_EPOCH + Duration::from_secs(1_704_164_645), 0)
}

fn deps(home: PathBuf) -> Deps {
    Deps {
        host: Arc::new(Home(home)),
        clock: Arc::new(fixed_clock),
        ..Deps::default()
    }
}

fn root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "secs-export-folder-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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

fn calibrated(root: &Path) -> Service {
    let mut dependencies = deps(root.join("home"));
    dependencies.unix_ms = Arc::new(|| 1_704_164_645_000);
    let (project, settings) = case_project::write(root, "room");
    let service = Service::new(dependencies);
    call(
        &service,
        Request::ProjectOpen(ProjectPathParams {
            path: project.to_string_lossy().into_owned(),
        }),
    )
    .unwrap();
    let events = Arc::new(Events::default());
    let Envelope::Ok(response) = service.call(
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            ..Default::default()
        }),
        events.clone(),
    ) else {
        panic!("calibration must start")
    };
    let Response::Calibrate(started) = *response else {
        panic!("calibration response is wrong")
    };
    assert!(matches!(
        events.wait(started.job),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    service
}

#[test]
fn export_folder_defaults_with_and_without_project() {
    let root = root();
    let service = Service::new(deps(root.join("home")));
    let Response::ExportFolderGet(without) = call(&service, Request::ExportFolderGet).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        PathBuf::from(without.folder),
        root.join("home/Documents/SECS_export/2024-01-02")
    );
    let project = root.join("My Room.secs");
    call(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: project.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
    )
    .unwrap();
    let Response::ExportFolderGet(with_project) = call(&service, Request::ExportFolderGet).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        PathBuf::from(with_project.folder),
        root.join("SECS_export/2024-01-02 My Room")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn derived_export_subfolders_use_the_fixed_clock() {
    let now = fixed_clock();
    let base = Path::new("exports");
    assert_eq!(
        secs_service::export_folder::mdat_folder(base, false, "mixed", now),
        base.join("SECS_mdat_RC_BA_mixed_20240102-030405")
    );
    assert_eq!(
        secs_service::export_folder::mdat_folder(base, true, "linear", now),
        base.join("SECS_mdat_RC_BA_MIMO_linear_20240102-030405")
    );
    assert_eq!(
        secs_service::export_folder::camilladsp_folder(base, false, "mixed", now),
        PathBuf::from("exports/SECS_camilla_RC_BA_mixed_20240102-030405/camilladsp")
    );
    assert_eq!(
        secs_service::export_folder::text_folder(base, false, "ultra", now),
        base.join("SECS_text_RC_BA_ultra_20240102-030405")
    );
    assert_eq!(
        secs_service::export_folder::eqapo_folder(base, true, "linear", now),
        base.join("SECS_eqapo_RC_BA_MIMO_linear_20240102-030405")
    );
    assert_eq!(
        secs_service::export_folder::convolution_folder(base, true, "mixed", now),
        base.join("SECS_convolution_RC_BA_MIMO_mixed_20240102-030405")
    );
}

#[test]
fn stored_export_folder_survives_save_and_open() {
    let root = root();
    let project = root.join("room.secs");
    let stored = root.join("chosen");
    let service = Service::new(deps(root.join("home")));
    call(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: project.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
    )
    .unwrap();
    call(
        &service,
        Request::ExportFolderSet(ExportFolderSetParams {
            folder: stored.to_string_lossy().into_owned(),
        }),
    )
    .unwrap();
    call(
        &service,
        Request::ProjectSave(ProjectSaveParams { path: None }),
    )
    .unwrap();
    let reopened = Service::new(deps(root.join("home")));
    call(
        &reopened,
        Request::ProjectOpen(ProjectPathParams {
            path: project.to_string_lossy().into_owned(),
        }),
    )
    .unwrap();
    let Response::ExportFolderGet(result) = call(&reopened, Request::ExportFolderGet).unwrap()
    else {
        panic!()
    };
    assert!(result.stored);
    assert_eq!(PathBuf::from(result.folder), stored);
    fs::remove_dir_all(root).unwrap();
}

#[derive(Debug)]
struct CancelConvolution {
    service: Arc<Service>,
    events: Events,
    requested: std::sync::atomic::AtomicBool,
}

impl EventSink for CancelConvolution {
    fn send(&self, event: JobEvent) {
        self.events.send(event.clone());
        if matches!(&event, JobEvent::Progress { stage, done: 1, .. } if stage == "export.convolution")
            && !self.requested.swap(true, Ordering::SeqCst)
        {
            let job = match event {
                JobEvent::Progress { job, .. } => job,
                _ => unreachable!(),
            };
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
fn cancelled_convolution_compute_installs_no_files() {
    let root = root();
    let destination = root.join("convolution");
    fs::create_dir_all(&destination).unwrap();
    fs::write(destination.join("existing.txt"), b"keep").unwrap();
    let service = Arc::new(calibrated(&root));
    let events = Arc::new(CancelConvolution {
        service: service.clone(),
        events: Events::default(),
        requested: std::sync::atomic::AtomicBool::new(false),
    });
    let response = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::Convolution,
            folder: destination.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: vec![96_000],
        }),
        events.clone(),
    );
    let Envelope::Ok(response) = response else {
        panic!("export must start: {response:?}")
    };
    let Response::Export(started) = *response else {
        panic!("export response is wrong")
    };
    assert!(matches!(
        events.events.wait(started.job),
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
    assert!(events.requested.load(Ordering::SeqCst));
    let mut installed = fs::read_dir(&destination)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    installed.sort();
    assert_eq!(installed, ["existing.txt"]);
    assert_eq!(fs::read(destination.join("existing.txt")).unwrap(), b"keep");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_unnamed_export_merges_the_chosen_folder_into_valid_existing_index() {
    let root = root();
    let parent = root.join("parent");
    let out = parent.join("out");
    let sibling = parent.join("sibling");
    let locked = parent.join("locked");
    let outside = root.join("outside");
    fs::create_dir_all(&sibling).unwrap();
    fs::create_dir_all(&locked).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let foreign_manifest = br#"{
        "design_name":"foreign","design_id":"outside","export_kind":"filter_text",
        "created_utc":"2024-01-01T00:00:00.000Z","mode":"mixed","stages":{"mimo":"none"}
    }"#;
    fs::write(sibling.join("secs-design.json"), foreign_manifest).unwrap();
    fs::write(outside.join("secs-design.json"), foreign_manifest).unwrap();
    fs::write(
        parent.join("designs.json"),
        br#"[
            {"design_name":"foreign","design_id":"outside","export_kind":"filter_text",
             "folder":"sibling","created_utc":"2024-01-01T00:00:00.000Z","mode":"mixed","mimo":"none"},
            {"design_name":"stale","design_id":"gone","export_kind":"filter_text",
             "folder":"gone","created_utc":"2023-01-01T00:00:00.000Z","mode":"mixed","mimo":"none"}
        ]"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&locked, fs::Permissions::from_mode(0)).unwrap();
    }
    let linked = parent.join("linked");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &linked).unwrap();
    #[cfg(windows)]
    if let Err(error) = std::os::windows::fs::symlink_dir(&outside, &linked) {
        eprintln!("skipping Windows link half: {error}");
    }

    let service = calibrated(&root);
    let events = Arc::new(Events::default());
    let response = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::EqualizerApo,
            folder: out.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    );
    let Envelope::Ok(response) = response else {
        panic!("export must start: {response:?}")
    };
    let Response::Export(started) = *response else {
        panic!("export response is wrong")
    };
    assert!(matches!(
        events.wait(started.job),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(parent.join("designs.json")).unwrap()).unwrap();
    let entries = index.as_array().unwrap();
    assert_eq!(entries.len(), 2);
    let folders = entries
        .iter()
        .map(|entry| entry["folder"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(folders.contains(&"out"));
    assert!(folders.contains(&"sibling"));
    assert!(!folders.contains(&"gone"));
    assert_eq!(
        fs::read(outside.join("secs-design.json")).unwrap(),
        foreign_manifest
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::remove_dir_all(root).unwrap();
}
