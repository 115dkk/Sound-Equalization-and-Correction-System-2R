use std::sync::{Arc, Mutex};
use std::time::Duration;

use secs_project::{Project, PyJson};
use secs_types::ipc::{
    ErrorCode, IpcError, JobEvent, JobId, JobKind, JobResult, JobSnapshot, JobStarted,
    LevelMonitorStartParams, LevelPlayParams,
};

use crate::measure::json::JsonExt;
use crate::measure::{MeasurementSettings, plan_for};
use crate::{EventSink, Service, lock};

#[derive(Debug, Default)]
pub(crate) struct LevelState {
    inner: Arc<Mutex<LevelInner>>,
}

#[derive(Debug, Default)]
struct LevelInner {
    job: Option<JobId>,
    play: Option<PlayRequest>,
}

#[derive(Debug, Clone)]
struct PlayRequest {
    role: String,
    master_dbfs: f64,
    generation: u64,
}

fn method(project: &Project) -> &str {
    project
        .extra
        .iter()
        .find(|(key, _)| key == "devices")
        .and_then(|(_, value)| value.get("method"))
        .and_then(JsonExt::str)
        .unwrap_or("native")
}

fn input(field: &str, detail: impl Into<String>) -> IpcError {
    IpcError::new(ErrorCode::MeasurementInput, detail).with_param("field", field)
}

fn level_error(error: impl std::fmt::Display) -> IpcError {
    IpcError::new(ErrorCode::DeviceUnavailable, error.to_string())
}

impl Service {
    pub(crate) fn start_level_monitor(
        &self,
        params: LevelMonitorStartParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let project = {
            let session = lock(&self.session);
            session.require()?.store.project().clone()
        };
        let settings = MeasurementSettings::from_project(&project).map_err(level_error)?;
        let plan = plan_for(&project, &settings);
        if let Some(role) = params.role.as_ref()
            && !plan.roles.contains(role)
        {
            return Err(input("role", "unknown measurement role"));
        }
        let deps = self.deps.clone();
        let state = self.level.inner.clone();
        let session = self.session.clone();
        let started = self.jobs.start(JobKind::LevelMonitor, events, move |job| {
            lock(&state).job = Some(job.id());
            let outcome = monitor_loop(&deps, &session, &state, &project, settings, plan, &job);
            let mut state = lock(&state);
            if state.job == Some(job.id()) {
                state.job = None;
                state.play = None;
            }
            outcome
        })?;
        lock(&self.level.inner).job = Some(started.job);
        Ok(started)
    }

    pub(crate) fn stop_level_monitor(&self) -> Result<Option<JobSnapshot>, IpcError> {
        let job = lock(&self.level.inner).job;
        job.map(|job| self.jobs.cancel(job)).transpose()
    }

    pub(crate) fn level_play(&self, params: LevelPlayParams) -> Result<(), IpcError> {
        if !params.master_dbfs.is_finite() || !(-120.0..=0.0).contains(&params.master_dbfs) {
            return Err(input("master_dbfs", "level outside -120..0 dBFS"));
        }
        let mut state = lock(&self.level.inner);
        if state.job.is_none() {
            return Err(IpcError::new(
                ErrorCode::JobNotFound,
                "level monitor is not running",
            ));
        }
        let generation = state.play.as_ref().map_or(1, |play| play.generation + 1);
        state.play = Some(PlayRequest {
            role: params.role,
            master_dbfs: params.master_dbfs,
            generation,
        });
        Ok(())
    }
}

fn monitor_loop(
    deps: &crate::Deps,
    session: &Arc<Mutex<crate::session::Session>>,
    state: &Arc<Mutex<LevelInner>>,
    project: &Project,
    settings: MeasurementSettings,
    plan: secs_project::MeasurementPlan,
    job: &crate::jobs::JobContext,
) -> Result<JobResult, IpcError> {
    match method(project) {
        "rew" => rew_loop(deps, session, state, &plan, job),
        "native" | "mock" => sampled_loop(deps, session, state, settings, plan, job),
        _ => Err(input("method", "unknown measurement method")),
    }
}

fn sampled_loop(
    deps: &crate::Deps,
    session: &Arc<Mutex<crate::session::Session>>,
    state: &Arc<Mutex<LevelInner>>,
    settings: MeasurementSettings,
    plan: secs_project::MeasurementPlan,
    job: &crate::jobs::JobContext,
) -> Result<JobResult, IpcError> {
    let method = method(lock(session).require()?.store.project()).to_owned();
    let mut measurer = match method.as_str() {
        "native" => SampledMeasurer::Native(crate::measure::NativeMeasurer::from_backend(
            (deps.audio_backend)(),
            settings,
        )),
        "mock" => SampledMeasurer::Mock((deps.mock_measurer)()),
        _ => return Err(input("method", "unsupported sampled monitor")),
    };
    let mut handled_generation = 0;
    while !job.cancel().is_cancelled() {
        let play = lock(state).play.clone();
        let Some(play) = play.filter(|play| play.generation != handled_generation) else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        let mut play_plan = plan.clone();
        play_plan.sweep_level_db = play.master_dbfs;
        let report = match measurer.level_check_streaming(
            &play_plan,
            &play.role,
            2.5,
            job.measurement_cancel(),
            &mut |report| send_meter(job, report),
        ) {
            Ok(report) => Some(report),
            Err(crate::measure::MeasureError::Audio(secs_audio::AudioError::Cancelled))
                if job.cancel().is_cancelled() =>
            {
                None
            }
            Err(error) => return Err(level_error(error)),
        };
        let Some(report) = report else {
            break;
        };
        send_meter(job, report);
        store_level(session, &play.role, report.input_dbfs, report.spl_db)?;
        handled_generation = play.generation;
    }
    Ok(JobResult::Stopped)
}

enum SampledMeasurer {
    Native(crate::measure::NativeMeasurer),
    Mock(crate::measure::MockMeasurer),
}

impl SampledMeasurer {
    fn level_check_streaming(
        &mut self,
        plan: &secs_project::MeasurementPlan,
        role: &str,
        seconds: f64,
        cancel: &crate::measure::CancelToken,
        on_level: &mut dyn FnMut(secs_audio::LevelReport),
    ) -> Result<secs_audio::LevelReport, crate::measure::MeasureError> {
        match self {
            Self::Native(measurer) => {
                measurer.level_check_streaming(plan, role, seconds, cancel, on_level)
            }
            Self::Mock(measurer) => {
                measurer.level_check_streaming(plan, role, seconds, cancel, on_level)
            }
        }
    }
}

fn send_meter(job: &crate::jobs::JobContext, report: secs_audio::LevelReport) {
    job.send(JobEvent::Meter {
        job: job.id(),
        rms_dbfs: report.input_dbfs,
        peak_dbfs: report.peak_dbfs,
        spl_db: report.spl_db,
        clipping: report.clip,
    });
}

fn rew_loop(
    deps: &crate::Deps,
    session: &Arc<Mutex<crate::session::Session>>,
    state: &Arc<Mutex<LevelInner>>,
    plan: &secs_project::MeasurementPlan,
    job: &crate::jobs::JobContext,
) -> Result<JobResult, IpcError> {
    struct Transport(Box<dyn secs_rew::Transport>);
    impl secs_rew::Transport for Transport {
        fn request(
            &self,
            request: &secs_rew::Request,
        ) -> Result<secs_rew::Response, secs_rew::RewError> {
            self.0.request(request)
        }
    }
    let pending = Arc::new(Mutex::new(Vec::<secs_rew::Level>::new()));
    let callback_pending = pending.clone();
    let mut rew = secs_rew::RewClient::new(Transport((deps.rew_transport)()));
    secs_rew::LevelMonitor::start(
        &mut rew,
        Arc::new(move |level| lock(&callback_pending).push(level)),
    )
    .map_err(level_error)?;
    let mut handled_generation = 0;
    let mut active: Option<(PlayRequest, std::time::Instant, Option<secs_rew::Level>)> = None;
    while !job.cancel().is_cancelled() {
        let requested = lock(state).play.clone();
        if let Some(play) = requested.filter(|play| play.generation != handled_generation) {
            let output = u32::try_from(plan.output_for(&play.role))
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| input("role", "role has no output"))?;
            secs_rew::LevelMonitor::play_pink(&mut rew, &play.role, output, play.master_dbfs)
                .map_err(level_error)?;
            active = Some((play, std::time::Instant::now(), None));
        }
        for level in std::mem::take(&mut *lock(&pending)) {
            job.send(JobEvent::Meter {
                job: job.id(),
                rms_dbfs: level.input_dbfs,
                peak_dbfs: level.peak_dbfs,
                spl_db: level.spl_db,
                clipping: level.clip,
            });
            if let Some((_, _, settled)) = active.as_mut() {
                *settled = Some(level);
            }
        }
        if active
            .as_ref()
            .is_some_and(|(_, started, _)| started.elapsed() >= Duration::from_secs_f64(2.5))
        {
            secs_rew::LevelMonitor::stop_generator(&mut rew);
            if let Some((play, _, settled)) = active.take() {
                if let Some(level) = settled {
                    store_level(session, &play.role, level.input_dbfs, level.spl_db)?;
                }
                handled_generation = play.generation;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    secs_rew::LevelMonitor::stop_generator(&mut rew);
    secs_rew::LevelMonitor::stop(&mut rew);
    Ok(JobResult::Stopped)
}

fn store_level(
    session: &Arc<Mutex<crate::session::Session>>,
    role: &str,
    dbfs: f64,
    spl_db: Option<f64>,
) -> Result<(), IpcError> {
    let mut session = lock(session);
    let open = session.require_mut()?;
    put_extra_map(open.store.project_mut(), "levels", role, dbfs);
    if let Some(spl) = spl_db {
        put_extra_map(open.store.project_mut(), "spl", role, spl);
    }
    open.dirty = true;
    Ok(())
}

fn put_extra_map(project: &mut Project, key: &str, role: &str, value: f64) {
    let entry = if let Some((_, value)) = project.extra.iter_mut().find(|(name, _)| name == key) {
        value
    } else {
        project
            .extra
            .push((key.to_owned(), PyJson::Object(Vec::new())));
        &mut project.extra.last_mut().expect("just inserted").1
    };
    if !matches!(entry, PyJson::Object(_)) {
        *entry = PyJson::Object(Vec::new());
    }
    let PyJson::Object(values) = entry else {
        unreachable!()
    };
    let rounded = (value * 100.0).round() / 100.0;
    if let Some((_, stored)) = values.iter_mut().find(|(name, _)| name == role) {
        *stored = PyJson::Float(rounded);
    } else {
        values.push((role.to_owned(), PyJson::Float(rounded)));
    }
}
