use std::sync::Arc;
use std::time::Duration;

use secs_types::ipc::{IpcError, JobEvent, JobKind, JobResult, JobSnapshot, JobStarted};

use crate::{EventSink, Service};

impl Service {
    pub(crate) fn start_status_watch(
        &self,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let deps = self.deps.clone();
        let jobs = self.jobs.clone();
        let rew = self.rew.clone();
        self.start_job(JobKind::StatusWatch, events, move |job| {
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
            Ok(JobResult::Stopped)
        })
    }

    pub(crate) fn stop_status_watch(&self) -> Result<Option<JobSnapshot>, IpcError> {
        self.jobs
            .running(JobKind::StatusWatch)
            .map(|job| self.jobs.cancel(job))
            .transpose()
    }
}

fn sleep_cancellable(job: &crate::jobs::JobContext, duration: Duration) {
    let deadline = std::time::Instant::now() + duration;
    while !job.cancel().is_cancelled() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}
