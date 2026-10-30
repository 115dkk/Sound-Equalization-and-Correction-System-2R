use super::*;
use crate::{Deps, EventSink, NoEvents, Service, lock};
use crate::{LocalTime, compute::ComputeIdentity};
use secs_types::ipc::{CalibrateParams, Envelope, JobEvent, Request, Response};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[path = "../examples/support/case_project.rs"]
mod case_project;

#[derive(Debug, Default)]
struct Completion {
    event: Mutex<Option<JobEvent>>,
    ready: Condvar,
}

impl EventSink for Completion {
    fn send(&self, event: JobEvent) {
        if matches!(event, JobEvent::Finished { .. }) {
            *lock(&self.event) = Some(event);
            self.ready.notify_all();
        }
    }
}

#[test]
fn mimo_cap_text_matches_upstream_for_zero_and_limited_supports() {
    assert_eq!(secs_engine::mimo::cap_text(0), "");
    assert_eq!(secs_engine::mimo::cap_text(3), " · 입력당 서포트 ≤ 3");
}

#[test]
fn live_and_reopened_results_have_equal_descriptions_for_every_export_kind() {
    // Same persisted oracle project and reopening path as tests/service.rs.
    let root = std::env::temp_dir().join(format!("secs-export-description-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (path, settings) = case_project::write(&root, "dolby");
    let service = Service::new(Deps::default());
    let open = |service: &Service| {
        assert!(matches!(
            service.call(
                Request::ProjectOpen(secs_types::ipc::ProjectPathParams {
                    path: path.to_string_lossy().into_owned(),
                }),
                Arc::new(NoEvents)
            ),
            Envelope::Ok(_)
        ));
    };
    open(&service);
    let completion = Arc::new(Completion::default());
    assert!(
        matches!(service.call(Request::Calibrate(CalibrateParams { settings: Some(settings), ..Default::default() }), completion.clone()), Envelope::Ok(response) if matches!(*response, Response::Calibrate(_)))
    );
    let (event, wait) = completion
        .ready
        .wait_timeout_while(lock(&completion.event), Duration::from_secs(120), |event| {
            event.is_none()
        })
        .unwrap();
    assert!(!wait.timed_out(), "calibration time limit");
    assert!(matches!(
        event.as_ref().unwrap(),
        JobEvent::Finished {
            status: secs_types::ipc::JobStatus::Succeeded,
            ..
        }
    ));
    let reopened = Service::new(Deps::default());
    open(&reopened);
    let compute = ComputeIdentity::from(&service.acceleration().status);
    let now = LocalTime {
        year: 2024,
        month: 1,
        day: 2,
        hour: 3,
        minute: 4,
        second: 5,
    };
    let live = lock(&service.session);
    let live = live.require().unwrap();
    let cached = lock(&reopened.session);
    let cached = cached.require().unwrap();
    // One description serves every export kind of a result.
    assert_eq!(
        describe(&live.results, live.store.project(), &compute, now).unwrap(),
        describe(&cached.results, cached.store.project(), &compute, now).unwrap(),
    );
    std::fs::remove_dir_all(root).unwrap();
}
