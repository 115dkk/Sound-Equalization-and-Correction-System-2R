// Adapted from Impulcifer-pip313 crates/impulcifer-jobs/src/registry.rs,
// commit cac2217 (MIT). This port uses pushed events and one active job per group.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use secs_compute::{CancelToken, Progress, ProgressSink};
use secs_types::ipc::{
    ErrorCode, IpcError, JobEvent, JobId, JobKind, JobProgress, JobResult, JobSnapshot, JobStarted,
    JobStatus,
};

use crate::{EventSink, Service};

const TERMINAL_HISTORY_LIMIT: usize = secs_types::rules::JOB_HISTORY_MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobGroup {
    Device,
    Compute,
    Export,
    SideMdat,
    StatusWatch,
}
fn group(kind: JobKind) -> JobGroup {
    match kind {
        JobKind::Measure | JobKind::SampleLoad | JobKind::MdatImport | JobKind::LevelMonitor => {
            JobGroup::Device
        }
        JobKind::Calibrate | JobKind::RecommendXo | JobKind::Mimo => JobGroup::Compute,
        JobKind::Export | JobKind::Apply => JobGroup::Export,
        JobKind::StatusWatch => JobGroup::StatusWatch,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct JobRegistry {
    inner: Arc<Mutex<Inner>>,
    changed: Arc<Condvar>,
    next_id: Arc<AtomicU64>,
}
#[derive(Debug, Default)]
struct Inner {
    jobs: Vec<JobRecord>,
    completed: VecDeque<JobId>,
}
#[derive(Debug)]
struct JobRecord {
    snapshot: JobSnapshot,
    group: JobGroup,
    cancel: CancelToken,
    measurement_cancel: crate::measure::CancelToken,
    attached: Option<crate::measure::CancelToken>,
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            changed: Arc::new(Condvar::new()),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }
}

#[derive(Clone)]
pub(crate) struct JobContext {
    id: JobId,
    registry: JobRegistry,
    events: Arc<dyn EventSink>,
    cancel: CancelToken,
    measurement_cancel: crate::measure::CancelToken,
    available_memory: u64,
}
impl JobContext {
    pub fn cancel(&self) -> &CancelToken {
        &self.cancel
    }
    pub fn progress_sink(&self) -> Arc<dyn ProgressSink> {
        Arc::new(JobProgressSink(self.clone()))
    }

    pub fn report_progress(&self, stage: &'static str, done: u64, total: u64) {
        self.progress_sink().report(&Progress {
            stage,
            done,
            total,
            eta: None,
        });
    }

    pub fn measurement_cancel(&self) -> &crate::measure::CancelToken {
        &self.measurement_cancel
    }

    pub fn available_memory(&self) -> u64 {
        self.available_memory
    }

    #[allow(dead_code)]
    pub fn attach_cancel(&self, token: crate::measure::CancelToken) {
        if let Some(job) = lock(&self.registry.inner)
            .jobs
            .iter_mut()
            .find(|job| job.snapshot.job == self.id && !job.snapshot.status.is_terminal())
        {
            job.attached = Some(token);
        }
    }

    pub fn role_progress(&self, stage: &str, done: u64, total: u64) {
        let value = JobProgress {
            stage: stage.into(),
            done,
            total,
        };
        self.registry.progress(self.id, value.clone());
        self.events.send(JobEvent::Progress {
            job: self.id,
            stage: value.stage,
            done,
            total,
        });
    }

    pub fn log(&self, notice: secs_types::ipc::Notice) {
        self.events.send(JobEvent::Log {
            job: self.id,
            code: notice.code,
            params: notice.params,
        });
    }

    pub fn send(&self, event: JobEvent) {
        self.events.send(event);
    }

    pub fn id(&self) -> JobId {
        self.id
    }

    pub fn begin_commit(&self) -> Result<(), IpcError> {
        let mut inner = lock(&self.registry.inner);
        let job = inner
            .jobs
            .iter_mut()
            .find(|job| job.snapshot.job == self.id)
            .ok_or_else(|| IpcError::new(ErrorCode::JobNotFound, "job not found"))?;
        if job.cancel.is_cancelled() {
            return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
        }
        job.snapshot.cancellable = false;
        Ok(())
    }
}
impl Service {
    pub(crate) fn start_job(
        &self,
        kind: JobKind,
        events: Arc<dyn EventSink>,
        body: impl FnOnce(JobContext) -> Result<JobResult, IpcError> + Send + 'static,
    ) -> Result<JobStarted, IpcError> {
        self.jobs
            .start(kind, events, self.deps.memory.clone(), body)
    }

    pub(crate) fn start_side_mdat_job(
        &self,
        events: Arc<dyn EventSink>,
        body: impl FnOnce(JobContext) -> Result<JobResult, IpcError> + Send + 'static,
    ) -> Result<JobStarted, IpcError> {
        self.jobs
            .start_side_mdat(events, self.deps.memory.clone(), body)
    }
}

struct JobProgressSink(JobContext);
impl std::fmt::Debug for JobProgressSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("JobProgressSink").field(&self.0.id).finish()
    }
}
impl ProgressSink for JobProgressSink {
    fn report(&self, progress: &Progress) {
        let value = JobProgress {
            stage: progress.stage.into(),
            done: progress.done,
            total: progress.total,
        };
        self.0.registry.progress(self.0.id, value.clone());
        self.0.events.send(JobEvent::Progress {
            job: self.0.id,
            stage: value.stage,
            done: value.done,
            total: value.total,
        });
    }
}

impl JobRegistry {
    pub fn start(
        &self,
        kind: JobKind,
        events: Arc<dyn EventSink>,
        memory: Arc<dyn secs_compute::MemoryProbe>,
        body: impl FnOnce(JobContext) -> Result<JobResult, IpcError> + Send + 'static,
    ) -> Result<JobStarted, IpcError> {
        self.start_in_group(kind, group(kind), events, memory.available_bytes(), body)
    }

    pub fn start_side_mdat(
        &self,
        events: Arc<dyn EventSink>,
        memory: Arc<dyn secs_compute::MemoryProbe>,
        body: impl FnOnce(JobContext) -> Result<JobResult, IpcError> + Send + 'static,
    ) -> Result<JobStarted, IpcError> {
        self.start_in_group(
            JobKind::Export,
            JobGroup::SideMdat,
            events,
            memory.available_bytes(),
            body,
        )
    }

    fn start_in_group(
        &self,
        kind: JobKind,
        job_group: JobGroup,
        events: Arc<dyn EventSink>,
        available_memory: u64,
        body: impl FnOnce(JobContext) -> Result<JobResult, IpcError> + Send + 'static,
    ) -> Result<JobStarted, IpcError> {
        let id = JobId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let cancel = CancelToken::new();
        let measurement_cancel = crate::measure::CancelToken::default();
        {
            let mut inner = lock(&self.inner);
            if job_group != JobGroup::SideMdat
                && inner
                    .jobs
                    .iter()
                    .any(|job| job.group == job_group && !job.snapshot.status.is_terminal())
            {
                return Err(IpcError::new(
                    ErrorCode::JobBusy,
                    "another job in this group is running",
                )
                .with_param("kind", format!("{kind:?}")));
            }
            inner.jobs.push(JobRecord {
                snapshot: JobSnapshot {
                    job: id,
                    kind,
                    status: JobStatus::Running,
                    cancellable: true,
                    progress: None,
                    error: None,
                },
                group: job_group,
                cancel: cancel.clone(),
                measurement_cancel: measurement_cancel.clone(),
                attached: None,
            });
        }
        events.send(JobEvent::Started { job: id, kind });
        let context = JobContext {
            id,
            registry: self.clone(),
            events: events.clone(),
            cancel,
            measurement_cancel,
            available_memory,
        };
        let registry = self.clone();
        let worker_events = events.clone();
        let spawn = std::thread::Builder::new()
            .name(format!("secs-job-{}", id.0))
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| body(context.clone())))
                    .unwrap_or_else(|panic| {
                        let detail = panic
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| panic.downcast_ref::<&str>().map(|v| (*v).to_owned()))
                            .unwrap_or_else(|| "job panicked".into());
                        Err(IpcError::new(ErrorCode::Internal, detail))
                    });
                registry.finish(id, outcome, worker_events);
            });
        if let Err(error) = spawn {
            let failure = IpcError::new(ErrorCode::Internal, error.to_string());
            self.finish(id, Err(failure.clone()), events);
            return Err(failure);
        }
        Ok(JobStarted { job: id, kind })
    }

    pub fn cancel(&self, id: JobId) -> Result<JobSnapshot, IpcError> {
        let mut inner = lock(&self.inner);
        let job = inner
            .jobs
            .iter_mut()
            .find(|job| job.snapshot.job == id)
            .ok_or_else(|| {
                IpcError::new(ErrorCode::JobNotFound, "job not found")
                    .with_param("job", id.0.to_string())
            })?;
        if !job.snapshot.status.is_terminal() && job.snapshot.cancellable {
            job.snapshot.status = JobStatus::CancelRequested;
            job.cancel.cancel();
            job.measurement_cancel.cancel();
            if let Some(attached) = &job.attached {
                attached.cancel();
            }
        }
        Ok(job.snapshot.clone())
    }

    pub fn snapshots(&self) -> Vec<JobSnapshot> {
        lock(&self.inner)
            .jobs
            .iter()
            .map(|job| job.snapshot.clone())
            .collect()
    }

    pub fn running(&self, kind: JobKind) -> Option<JobId> {
        lock(&self.inner)
            .jobs
            .iter()
            .find(|job| job.snapshot.kind == kind && !job.snapshot.status.is_terminal())
            .map(|job| job.snapshot.job)
    }

    pub fn running_in_device_group(&self) -> bool {
        lock(&self.inner)
            .jobs
            .iter()
            .any(|job| job.group == JobGroup::Device && !job.snapshot.status.is_terminal())
    }

    /// The kind of a running measurement-group job other than the level
    /// monitor, which the screen restarts itself (TD-91).
    pub fn running_measurement(&self) -> Option<JobKind> {
        [JobKind::Measure, JobKind::SampleLoad, JobKind::MdatImport]
            .into_iter()
            .find(|kind| self.running(*kind).is_some())
    }

    pub fn cancel_all(&self) {
        let mut inner = lock(&self.inner);
        for job in &mut inner.jobs {
            if job.group != JobGroup::SideMdat
                && !job.snapshot.status.is_terminal()
                && job.snapshot.cancellable
            {
                job.snapshot.status = JobStatus::CancelRequested;
                job.cancel.cancel();
                job.measurement_cancel.cancel();
                if let Some(attached) = &job.attached {
                    attached.cancel();
                }
            }
        }
    }

    pub fn wait_for_device_and_side_mdat(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut inner = lock(&self.inner);
        while inner.jobs.iter().any(|job| {
            matches!(job.group, JobGroup::Device | JobGroup::SideMdat)
                && !job.snapshot.status.is_terminal()
        }) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, wait) = self.changed.wait_timeout(inner, remaining).unwrap();
            inner = next;
            if wait.timed_out() {
                return false;
            }
        }
        true
    }

    fn progress(&self, id: JobId, progress: JobProgress) {
        if let Some(job) = lock(&self.inner)
            .jobs
            .iter_mut()
            .find(|job| job.snapshot.job == id)
        {
            job.snapshot.progress = Some(progress);
        }
    }

    fn finish(&self, id: JobId, outcome: Result<JobResult, IpcError>, events: Arc<dyn EventSink>) {
        let (status, result, error) = {
            let mut inner = lock(&self.inner);
            let Some(job) = inner.jobs.iter_mut().find(|job| job.snapshot.job == id) else {
                return;
            };
            let cancelled = || IpcError::new(ErrorCode::Cancelled, "cancelled");
            let (status, result, error) = match outcome {
                Ok(_) if job.cancel.is_cancelled() => {
                    (JobStatus::Cancelled, None, Some(cancelled()))
                }
                Ok(result) => (JobStatus::Succeeded, Some(result), None),
                Err(error) if error.code == ErrorCode::Cancelled || job.cancel.is_cancelled() => {
                    (JobStatus::Cancelled, None, Some(cancelled()))
                }
                Err(error) => (JobStatus::Failed, None, Some(error)),
            };
            job.snapshot.status = status;
            job.snapshot.cancellable = false;
            job.snapshot.error = error.clone();
            inner.completed.push_back(id);
            while inner.completed.len() > TERMINAL_HISTORY_LIMIT {
                let oldest = inner
                    .completed
                    .pop_front()
                    .expect("history exceeds its limit");
                inner.jobs.retain(|record| {
                    record.snapshot.job != oldest || !record.snapshot.status.is_terminal()
                });
            }
            (status, result, error)
        };
        self.changed.notify_all();
        events.send(JobEvent::Finished {
            job: id,
            status,
            result: result.map(Box::new),
            error,
        });
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A job panic is caught above; poisoning must not make later IPC calls panic.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::sync::{Condvar, mpsc};
    use std::time::{Duration, Instant};

    use super::*;
    use secs_types::ipc::ExportResult;

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
    fn memory() -> Arc<dyn secs_compute::MemoryProbe> {
        Arc::new(secs_compute::FixedMemory(u64::MAX))
    }

    impl Events {
        fn wait_finished(&self, timeout: Duration) -> Vec<JobEvent> {
            let deadline = Instant::now() + timeout;
            let mut values = lock(&self.values);
            while !values
                .iter()
                .any(|event| matches!(event, JobEvent::Finished { .. }))
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                values = self.changed.wait_timeout(values, remaining).unwrap().0;
            }
            values.clone()
        }
    }

    #[test]
    fn job_context_reports_the_memory_available_at_start() {
        let registry = JobRegistry::default();
        let events = Arc::new(Events::default());
        registry
            .start(
                JobKind::Export,
                events.clone(),
                Arc::new(secs_compute::FixedMemory(123)),
                |job| {
                    assert_eq!(job.available_memory(), 123);
                    Ok(JobResult::Stopped)
                },
            )
            .unwrap();
        let values = events.wait_finished(Duration::from_secs(2));
        assert!(matches!(
            values.last(),
            Some(JobEvent::Finished {
                status: JobStatus::Succeeded,
                ..
            })
        ));
    }

    #[test]
    fn running_answers_the_live_job_of_a_kind_and_none_after_it_ends() {
        let registry = JobRegistry::default();
        let events = Arc::new(Events::default());
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_gate = gate.clone();
        let started = registry
            .start(JobKind::Export, events.clone(), memory(), move |_| {
                let mut released = lock(&worker_gate.0);
                while !*released {
                    released = worker_gate.1.wait(released).unwrap();
                }
                Ok(JobResult::Stopped)
            })
            .unwrap();
        assert_eq!(registry.running(JobKind::Export), Some(started.job));
        assert_eq!(registry.running(JobKind::Calibrate), None);
        *lock(&gate.0) = true;
        gate.1.notify_all();
        events.wait_finished(Duration::from_secs(2));
        assert_eq!(registry.running(JobKind::Export), None);
    }

    #[test]
    fn attach_cancel_is_cancelled_with_the_job_and_only_the_last_attached_token_counts() {
        let registry = JobRegistry::default();
        let events = Arc::new(Events::default());
        let first = crate::measure::CancelToken::default();
        let second = crate::measure::CancelToken::default();
        let worker_first = first.clone();
        let worker_second = second.clone();
        let (ready, waiting) = mpsc::channel();
        let started = registry
            .start(
                JobKind::LevelMonitor,
                events.clone(),
                memory(),
                move |job| {
                    job.attach_cancel(worker_first);
                    job.attach_cancel(worker_second);
                    ready.send(()).unwrap();
                    while !job.cancel().is_cancelled() {
                        std::thread::yield_now();
                    }
                    Ok(JobResult::Stopped)
                },
            )
            .unwrap();
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        registry.cancel(started.job).unwrap();
        events.wait_finished(Duration::from_secs(2));
        assert!(!first.is_cancelled());
        assert!(second.is_cancelled());
    }

    #[test]
    fn jobs_push_ordered_events_and_exclude_the_same_group() {
        let registry = JobRegistry::default();
        let events = Arc::new(Events::default());
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_gate = gate.clone();
        registry
            .start(JobKind::Calibrate, events.clone(), memory(), move |ctx| {
                ctx.progress_sink().report(&Progress {
                    stage: "test",
                    done: 1,
                    total: 1,
                    eta: None,
                });
                let (open, changed) = &*worker_gate;
                let mut open = lock(open);
                while !*open {
                    open = changed.wait(open).unwrap();
                }
                Ok(JobResult::Export(ExportResult {
                    files: vec![],
                    folder: String::new(),
                    stale: false,
                    output_rate: 48_000,
                    notes: Vec::new(),
                }))
            })
            .unwrap();
        let error = registry
            .start(JobKind::Mimo, events.clone(), memory(), |_| {
                unreachable!("busy jobs do not start")
            })
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::JobBusy);
        *lock(&gate.0) = true;
        gate.1.notify_all();
        let values = events.wait_finished(Duration::from_secs(2));
        assert!(matches!(values.first(), Some(JobEvent::Started { .. })));
        assert!(matches!(values.get(1), Some(JobEvent::Progress { .. })));
        assert!(matches!(
            values.last(),
            Some(JobEvent::Finished {
                status: JobStatus::Succeeded,
                ..
            })
        ));
    }

    #[test]
    fn terminal_history_is_bounded_by_completion_and_active_jobs_keep_their_events() {
        let registry = JobRegistry::default();
        let active_events = Arc::new(Events::default());
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_gate = gate.clone();
        let active = registry
            .start(
                JobKind::Calibrate,
                active_events.clone(),
                memory(),
                move |_| {
                    let mut ready = lock(&worker_gate.0);
                    while !*ready {
                        ready = worker_gate.1.wait(ready).unwrap();
                    }
                    Ok(JobResult::Calibration)
                },
            )
            .unwrap();
        let mut completed = Vec::new();
        for index in 0..96 {
            let events = Arc::new(Events::default());
            let folder = format!("export-{index}");
            let expected = folder.clone();
            let job = registry
                .start(JobKind::Export, events.clone(), memory(), move |_| {
                    Ok(JobResult::Export(ExportResult {
                        folder,
                        files: vec!["filter.wav".into()],
                        stale: false,
                        output_rate: 48_000,
                        notes: Vec::new(),
                    }))
                })
                .unwrap();
            let values = events.wait_finished(Duration::from_secs(2));
            let Some(JobEvent::Finished {
                status: JobStatus::Succeeded,
                result: Some(result),
                ..
            }) = values.last()
            else {
                panic!("finish must be delivered")
            };
            assert!(
                matches!(result.as_ref(), JobResult::Export(value) if value.folder == expected)
            );
            completed.push(job.job);
            let snapshots = registry.snapshots();
            assert!(
                snapshots
                    .iter()
                    .any(|snapshot| snapshot.job == active.job && !snapshot.status.is_terminal())
            );
            assert_eq!(
                snapshots
                    .iter()
                    .filter(|snapshot| snapshot.status.is_terminal())
                    .count(),
                completed.len().min(TERMINAL_HISTORY_LIMIT)
            );
            assert!(
                serde_json::to_value(snapshots)
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|snapshot| snapshot.get("result").is_none())
            );
        }
        assert_eq!(
            registry.cancel(completed[0]).unwrap_err().code,
            ErrorCode::JobNotFound
        );
        *lock(&gate.0) = true;
        gate.1.notify_all();
        let values = active_events.wait_finished(Duration::from_secs(2));
        assert!(
            matches!(values.last(), Some(JobEvent::Finished { result: Some(result), .. }) if **result == JobResult::Calibration)
        );
        let snapshots = registry.snapshots();
        assert_eq!(snapshots.len(), TERMINAL_HISTORY_LIMIT);
        assert!(snapshots.iter().any(|snapshot| snapshot.job == active.job));
        for id in &completed[completed.len() - (TERMINAL_HISTORY_LIMIT - 1)..] {
            assert!(snapshots.iter().any(|snapshot| snapshot.job == *id));
        }
    }

    #[test]
    fn a_body_that_returns_cancelled_finishes_like_an_external_cancel_td165() {
        fn cancelled_error(events: &Events) -> IpcError {
            let values = events.wait_finished(Duration::from_secs(2));
            let Some(JobEvent::Finished {
                status,
                result,
                error,
                ..
            }) = values.last()
            else {
                panic!("job did not finish")
            };
            assert_eq!(*status, JobStatus::Cancelled);
            assert!(result.is_none());
            let error = error.clone().expect("cancelled jobs carry one error");
            assert_eq!(error.code, ErrorCode::Cancelled);
            assert_eq!(error.detail, "cancelled");
            assert!(error.params.is_empty());
            error
        }

        let registry = JobRegistry::default();
        let external_events = Arc::new(Events::default());
        let external = registry
            .start(
                JobKind::Calibrate,
                external_events.clone(),
                memory(),
                |ctx| {
                    while !ctx.cancel().is_cancelled() {
                        std::thread::yield_now();
                    }
                    Ok(JobResult::Calibration)
                },
            )
            .unwrap();
        registry.cancel(external.job).unwrap();
        let external_error = cancelled_error(&external_events);

        let body_events = Arc::new(Events::default());
        registry
            .start(JobKind::Calibrate, body_events.clone(), memory(), |_| {
                Err(IpcError::new(ErrorCode::Cancelled, "cancelled"))
            })
            .unwrap();
        let body_error = cancelled_error(&body_events);
        assert_eq!(external_error, body_error);
    }

    #[test]
    fn cancellation_finishes_within_one_second() {
        let registry = JobRegistry::default();
        let events = Arc::new(Events::default());
        let started = registry
            .start(JobKind::Calibrate, events.clone(), memory(), |ctx| {
                loop {
                    ctx.cancel()
                        .check()
                        .map_err(|_| IpcError::new(ErrorCode::Cancelled, "cancelled"))?;
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
            .unwrap();
        let start = Instant::now();
        registry.cancel(started.job).unwrap();
        let values = events.wait_finished(Duration::from_secs(1));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(matches!(
            values.last(),
            Some(JobEvent::Finished {
                status: JobStatus::Cancelled,
                ..
            })
        ));
    }
}
