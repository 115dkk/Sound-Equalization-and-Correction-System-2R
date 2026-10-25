#![forbid(unsafe_code)]
//! A tutorial session writes only into its own temporary folder (TD-90).
//!
//! This binary holds one test because it changes the process's current
//! directory, which every other test in the same process would share.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use secs_service::{Deps, EventSink, NoEvents, Service};
use secs_types::ipc::*;

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
    fn succeeded(&self, job: JobId) {
        let deadline = Instant::now() + Duration::from_secs(300);
        let mut values = self.values.lock().unwrap();
        loop {
            if let Some(event) = values
                .iter()
                .find(|event| matches!(event, JobEvent::Finished { job: id, .. } if *id == job))
            {
                assert!(
                    matches!(
                        event,
                        JobEvent::Finished {
                            status: JobStatus::Succeeded,
                            ..
                        }
                    ),
                    "{event:?}"
                );
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "job {job:?} did not finish");
            values = self.changed.wait_timeout(values, remaining).unwrap().0;
        }
    }
}

fn call(service: &Service, request: Request, events: Arc<dyn EventSink>) -> Response {
    match service.call(request, events) {
        Envelope::Ok(response) => *response,
        Envelope::Err(error) => panic!("{error:?}"),
    }
}

fn tutorial(service: &Service) -> ProjectInfo {
    // As the screen starts it (`apps/secs2r/ui/src/state/session.ts`).
    let Response::ProjectNew(info) = call(
        service,
        Request::ProjectNew(ProjectNewParams {
            path: String::new(),
            preset: "7.1.4".into(),
            sub_count: 3,
            sample: true,
            tutorial: true,
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    info
}

fn tutorial_folders() -> Vec<PathBuf> {
    let prefix = format!("secs2r-tutorial-{}-", std::process::id());
    let mut folders: Vec<_> = fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect();
    folders.sort();
    folders
}

#[test]
fn tutorial_measurements_and_results_stay_in_a_temporary_folder_removed_with_the_session() {
    let base = std::env::temp_dir();
    let cwd = base.join(format!("secs-tutorial-cwd-{}", std::process::id()));
    let other = base.join(format!("secs-tutorial-other-{}.secs", std::process::id()));
    let _ = fs::remove_dir_all(&cwd);
    fs::create_dir_all(&cwd).unwrap();
    std::env::set_current_dir(&cwd).unwrap();

    let service = Service::new(Deps::default());
    let info = tutorial(&service);
    assert!(info.tutorial);
    assert_eq!(info.path, "", "the given path is reported back");
    let events = Arc::new(Events::default());
    let Response::SampleLoad(loaded) = call(
        &service,
        Request::SampleLoad(SampleLoadParams { positions: None }),
        events.clone(),
    ) else {
        panic!()
    };
    events.succeeded(loaded.job);
    let Response::Calibrate(calibrated) = call(
        &service,
        Request::Calibrate(CalibrateParams::default()),
        events.clone(),
    ) else {
        panic!()
    };
    events.succeeded(calibrated.job);
    let written: Vec<_> = fs::read_dir(&cwd)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert!(
        written.is_empty(),
        "the tutorial wrote into the current folder: {written:?}"
    );
    let Response::Results(results) = call(&service, Request::Results, Arc::new(NoEvents)) else {
        panic!()
    };
    assert!(results.calibration.is_some());

    let folders = tutorial_folders();
    assert_eq!(folders.len(), 1, "{folders:?}");
    let folder = &folders[0];
    assert!(
        folder
            .join("tutorial.irs")
            .join("secs_run_cache.json")
            .is_file()
    );
    assert!(
        fs::read_dir(folder.join("tutorial.irs"))
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "wav"))
    );

    // Replacing the session removes its folder.
    call(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: other.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    assert!(!folder.exists(), "replaced tutorial folder is still there");

    // So does ending the service while a tutorial is open.
    tutorial(&service);
    let second = tutorial_folders();
    assert_eq!(second.len(), 1, "{second:?}");
    drop(service);
    assert!(!second[0].exists(), "tutorial folder outlived the service");

    std::env::set_current_dir(&base).unwrap();
    fs::remove_dir_all(&cwd).unwrap();
    let _ = fs::remove_file(&other);
    let _ = fs::remove_dir_all(other.with_extension("irs"));
}
