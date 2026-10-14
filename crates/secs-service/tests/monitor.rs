#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use secs_service::{EventSink, NoEvents, Service};
use secs_types::ipc::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

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
    fn wait_meters(&self, job: JobId, count: usize, timeout: Duration) {
        self.wait_until(timeout, |events| {
            events
                .iter()
                .filter(|event| matches!(event, JobEvent::Meter { job: id, .. } if *id == job))
                .count()
                >= count
        });
    }

    fn wait_finished(&self, job: JobId, timeout: Duration) -> JobStatus {
        self.wait_until(timeout, |events| {
            events
                .iter()
                .any(|event| matches!(event, JobEvent::Finished { job: id, .. } if *id == job))
        });
        self.values
            .lock()
            .unwrap()
            .iter()
            .find_map(|event| match event {
                JobEvent::Finished {
                    job: id, status, ..
                } if *id == job => Some(*status),
                _ => None,
            })
            .unwrap()
    }

    fn wait_until(&self, timeout: Duration, predicate: impl Fn(&[JobEvent]) -> bool) {
        let deadline = Instant::now() + timeout;
        let mut values = self.values.lock().unwrap();
        while !predicate(&values) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "event timeout: {values:?}");
            values = self.changed.wait_timeout(values, remaining).unwrap().0;
        }
    }
}

fn root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "secs-monitor-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn call(service: &Service, request: Request, events: Arc<dyn EventSink>) -> Response {
    match service.call(request, events) {
        Envelope::Ok(response) => *response,
        Envelope::Err(error) => panic!("{error:?}"),
    }
}

fn configure_mock(service: &Service, path: &std::path::Path) {
    call(
        service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
        }),
        Arc::new(NoEvents),
    );
    call(
        service,
        Request::MeasureConfigure(MeasureConfigureParams {
            method: "mock".into(),
            ..Default::default()
        }),
        Arc::new(NoEvents),
    );
}

#[test]
fn mock_monitor_emits_stores_excludes_and_stops() {
    let root = root();
    let project = root.join("room.secs");
    let service = Service::new(Default::default());
    configure_mock(&service, &project);
    let events = Arc::new(Events::default());
    let Response::LevelMonitorStart(started) = call(
        &service,
        Request::LevelMonitorStart(LevelMonitorStartParams {
            role: Some("L".into()),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    let busy = service.call(
        Request::MeasurePosition(MeasurePositionParams { position: 1 }),
        Arc::new(NoEvents),
    );
    assert!(matches!(
        busy,
        Envelope::Err(IpcError {
            code: ErrorCode::JobBusy,
            ..
        })
    ));
    call(
        &service,
        Request::LevelPlay(LevelPlayParams {
            role: "L".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    events.wait_meters(started.job, 20, Duration::from_secs(10));
    call(&service, Request::LevelMonitorStop, Arc::new(NoEvents));
    assert_eq!(
        events.wait_finished(started.job, Duration::from_secs(1)),
        JobStatus::Cancelled
    );
    call(
        &service,
        Request::ProjectSave(ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );
    let text = fs::read_to_string(&project).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(saved["levels"]["L"].as_f64().is_some(), "{text}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn generic_cancel_ends_monitor() {
    let root = root();
    let service = Service::new(Default::default());
    configure_mock(&service, &root.join("room.secs"));
    let events = Arc::new(Events::default());
    let Response::LevelMonitorStart(started) = call(
        &service,
        Request::LevelMonitorStart(LevelMonitorStartParams::default()),
        events.clone(),
    ) else {
        panic!()
    };
    call(
        &service,
        Request::JobCancel(JobCancelParams { job: started.job }),
        Arc::new(NoEvents),
    );
    assert_eq!(
        events.wait_finished(started.job, Duration::from_secs(1)),
        JobStatus::Cancelled
    );
    fs::remove_dir_all(root).unwrap();
}
