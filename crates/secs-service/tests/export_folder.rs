#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use secs_service::{Deps, Host, LocalTime, NoEvents, Service};
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
        secs_service::export_folder::before_after_folder(base, false, "mixed", now),
        base.join("SECS_mdat_RC_BA_mixed_20240102-030405")
    );
    assert_eq!(
        secs_service::export_folder::before_after_folder(base, true, "linear", now),
        base.join("SECS_mdat_RC_BA_MIMO_linear_20240102-030405")
    );
    assert_eq!(
        secs_service::export_folder::camilladsp_folder(base),
        PathBuf::from("exports/camilladsp")
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
