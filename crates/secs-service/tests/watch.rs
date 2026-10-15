#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use secs_service::{Deps, EventSink, NoEvents, Service};
use secs_types::ipc::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct ToggleTransport(Arc<AtomicBool>);

impl secs_rew::Transport for ToggleTransport {
    fn request(
        &self,
        request: &secs_rew::Request,
    ) -> Result<secs_rew::Response, secs_rew::RewError> {
        if self.0.load(Ordering::SeqCst) && request.path == "/version" {
            Ok(secs_rew::Response::ok(
                serde_json::json!({"message": "5.40 Beta 135"}),
            ))
        } else {
            Err(secs_rew::RewError::Connection {
                path: request.path.clone(),
                message: "offline".into(),
            })
        }
    }
}

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
    fn status_count(&self, job: JobId) -> usize {
        self.values
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, JobEvent::MeasureStatus { job: id, .. } if *id == job))
            .count()
    }

    fn wait_status_count(&self, job: JobId, count: usize, timeout: Duration) {
        self.wait_until(timeout, |events| {
            events
                .iter()
                .filter(
                    |event| matches!(event, JobEvent::MeasureStatus { job: id, .. } if *id == job),
                )
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
        "secs-watch-{}-{}",
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
fn watch_reports_transitions_deduplicates_and_pauses_for_device_jobs() {
    let root = root();
    let online = Arc::new(AtomicBool::new(false));
    let transport_online = online.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(ToggleTransport(transport_online.clone()))),
        ..Deps::default()
    });
    configure_mock(&service, &root.join("room.secs"));

    let events = Arc::new(Events::default());
    let Response::StatusWatchStart(watch) =
        call(&service, Request::StatusWatchStart, events.clone())
    else {
        panic!()
    };
    events.wait_status_count(watch.job, 1, Duration::from_secs(1));

    online.store(true, Ordering::SeqCst);
    events.wait_status_count(watch.job, 2, Duration::from_secs(4));
    std::thread::sleep(Duration::from_millis(3200));
    assert_eq!(events.status_count(watch.job), 2);

    let monitor_events = Arc::new(Events::default());
    let Response::LevelMonitorStart(monitor) = call(
        &service,
        Request::LevelMonitorStart(LevelMonitorStartParams::default()),
        monitor_events.clone(),
    ) else {
        panic!()
    };
    online.store(false, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(3200));
    assert_eq!(events.status_count(watch.job), 2);

    call(&service, Request::LevelMonitorStop, Arc::new(NoEvents));
    assert_eq!(
        monitor_events.wait_finished(monitor.job, Duration::from_secs(1)),
        JobStatus::Cancelled
    );
    events.wait_status_count(watch.job, 3, Duration::from_secs(4));

    call(&service, Request::StatusWatchStop, Arc::new(NoEvents));
    assert_eq!(
        events.wait_finished(watch.job, Duration::from_secs(1)),
        JobStatus::Cancelled
    );
    fs::remove_dir_all(root).unwrap();
}
