use std::sync::{Arc, Mutex};
use std::time::Duration;

use secs_types::ipc::{IpcError, JobEvent, JobId, JobKind, JobResult, JobSnapshot, JobStarted};

use crate::{EventSink, Service, lock};

#[derive(Debug, Default)]
pub(crate) struct WatchState {
    job: Arc<Mutex<Option<JobId>>>,
}

impl Service {
    pub(crate) fn start_status_watch(
        &self,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let deps = self.deps.clone();
        let jobs = self.jobs.clone();
        let state = self.status_watch.job.clone();
        let rew = self.rew.clone();
        let started = self.jobs.start(JobKind::StatusWatch, events, move |job| {
            *lock(&state) = Some(job.id());
            let mut previous = None;
            while !job.cancel().is_cancelled() {
                if !jobs.running_in_device_group() {
                    let status = super::measurement::measure_methods_for(&deps, &rew);
                    if previous.as_ref() != Some(&status) {
                        job.send(JobEvent::MeasureStatus {
                            job: job.id(),
                            status: status.clone(),
                        });
                        previous = Some(status);
                    }
                }
                sleep_cancellable(&job, Duration::from_secs(3));
            }
            let mut state = lock(&state);
            if *state == Some(job.id()) {
                *state = None;
            }
            Ok(JobResult::Stopped)
        })?;
        *lock(&self.status_watch.job) = Some(started.job);
        Ok(started)
    }

    pub(crate) fn stop_status_watch(&self) -> Result<Option<JobSnapshot>, IpcError> {
        let job = *lock(&self.status_watch.job);
        job.map(|job| self.jobs.cancel(job)).transpose()
    }
}

fn sleep_cancellable(job: &crate::jobs::JobContext, duration: Duration) {
    let deadline = std::time::Instant::now() + duration;
    while !job.cancel().is_cancelled() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}
