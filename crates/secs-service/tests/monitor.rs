#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use secs_service::{Deps, EventSink, NoEvents, Service};
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

    fn wait_play_end(&self, job: JobId, role: &str, outcome: &str, timeout: Duration) {
        self.wait_until(timeout, |events| {
            events.iter().any(|event| {
                matches!(event, JobEvent::Log { job: id, code, params }
                    if *id == job
                        && code == "level_play_end"
                        && params.get("role").is_some_and(|value| value == role)
                        && params.get("outcome").is_some_and(|value| value == outcome))
            })
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

fn configure(service: &Service, path: &std::path::Path, method: &str) {
    call(
        service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    call(
        service,
        Request::MeasureConfigure(MeasureConfigureParams {
            method: method.into(),
            ..Default::default()
        }),
        Arc::new(NoEvents),
    );
}

fn configure_mock(service: &Service, path: &std::path::Path) {
    configure(service, path, "mock");
}

#[derive(Debug, Default)]
struct NativeAudioState {
    history: Vec<&'static str>,
    active_inputs_during_play: Vec<usize>,
    active_inputs: usize,
}

#[derive(Clone, Debug, Default)]
struct NativeAudio {
    state: Arc<Mutex<NativeAudioState>>,
}

impl NativeAudio {
    fn endpoint() -> secs_audio::Endpoint {
        secs_audio::Endpoint {
            id: "native-fake".into(),
            name: "native-fake".into(),
            host_api: "fake".into(),
            max_input_channels: 1,
            max_output_channels: 2,
            default_samplerate: 48_000.0,
            is_default_input: true,
            is_default_output: true,
        }
    }
}

impl secs_audio::AudioBackend for NativeAudio {
    fn name(&self) -> &'static str {
        "native-monitor-fake"
    }

    fn enumerate(&self) -> Result<Vec<secs_audio::Endpoint>, secs_audio::AudioError> {
        Ok(vec![Self::endpoint()])
    }

    fn probe(
        &self,
        _endpoint: &secs_audio::Endpoint,
        _direction: secs_audio::Direction,
        spec: secs_audio::StreamSpec,
        mode: secs_audio::ShareMode,
    ) -> Result<secs_audio::ProbeResult, secs_audio::AudioError> {
        Ok(secs_audio::ProbeResult {
            supported: true,
            mode,
            native_sample_rate: spec.sample_rate,
            native_channels: spec.channels,
            detail: String::new(),
        })
    }

    fn open_output(
        &self,
        _endpoint: &secs_audio::Endpoint,
        spec: secs_audio::StreamSpec,
        mode: secs_audio::ShareMode,
    ) -> Result<Box<dyn secs_audio::OutputSession>, secs_audio::AudioError> {
        self.state.lock().unwrap().history.push("output open");
        Ok(Box::new(NativeOutput {
            state: self.state.clone(),
            spec,
            mode,
        }))
    }

    fn open_input(
        &self,
        _endpoint: &secs_audio::Endpoint,
        spec: secs_audio::StreamSpec,
        _mode: secs_audio::ShareMode,
    ) -> Result<Box<dyn secs_audio::InputSession>, secs_audio::AudioError> {
        self.state.lock().unwrap().history.push("input open");
        Ok(Box::new(NativeInput {
            state: self.state.clone(),
            spec,
            started: false,
        }))
    }
}

struct NativeInput {
    state: Arc<Mutex<NativeAudioState>>,
    spec: secs_audio::StreamSpec,
    started: bool,
}

impl secs_audio::InputSession for NativeInput {
    fn start(&mut self) -> Result<(), secs_audio::AudioError> {
        self.started = true;
        let mut state = self.state.lock().unwrap();
        state.active_inputs += 1;
        state.history.push("input start");
        Ok(())
    }

    fn read_into(
        &mut self,
        dst: &mut [f32],
        cancel: &secs_audio::CancelToken,
    ) -> Result<secs_audio::CaptureRead, secs_audio::AudioError> {
        if cancel.is_cancelled() {
            return Err(secs_audio::AudioError::Cancelled);
        }
        dst.fill(0.1);
        std::thread::sleep(Duration::from_millis(2));
        Ok(secs_audio::CaptureRead {
            frames: dst.len() / usize::from(self.spec.channels),
            discontinuity: false,
            silent: false,
        })
    }

    fn stop(&mut self) -> Result<(), secs_audio::AudioError> {
        let mut state = self.state.lock().unwrap();
        if self.started {
            state.active_inputs -= 1;
            self.started = false;
        }
        state.history.push("input stop");
        Ok(())
    }
}

struct NativeOutput {
    state: Arc<Mutex<NativeAudioState>>,
    spec: secs_audio::StreamSpec,
    mode: secs_audio::ShareMode,
}

impl secs_audio::OutputSession for NativeOutput {
    fn play_to_completion(
        &mut self,
        data: &[f32],
        cancel: &secs_audio::CancelToken,
    ) -> Result<secs_audio::PlaybackReport, secs_audio::AudioError> {
        if cancel.is_cancelled() {
            return Err(secs_audio::AudioError::Cancelled);
        }
        let mut state = self.state.lock().unwrap();
        let active_inputs = state.active_inputs;
        state.active_inputs_during_play.push(active_inputs);
        state.history.push("output play");
        drop(state);
        let frames = (data.len() / usize::from(self.spec.channels)) as u64;
        Ok(secs_audio::PlaybackReport {
            frames_submitted: frames,
            frames_drained: frames,
            mode: self.mode,
            underruns: 0,
            cancelled: false,
        })
    }
}

#[test]
fn native_monitor_meters_while_idle_and_closes_it_before_a_play() {
    let root = root();
    let audio = NativeAudio::default();
    let injected = audio.clone();
    let service = Service::new(Deps {
        audio_backend: Arc::new(move || Box::new(injected.clone())),
        ..Default::default()
    });
    configure(&service, &root.join("room.secs"), "native");
    let events = Arc::new(Events::default());
    let Response::LevelMonitorStart(started) = call(
        &service,
        Request::LevelMonitorStart(LevelMonitorStartParams::default()),
        events.clone(),
    ) else {
        panic!()
    };
    events.wait_meters(started.job, 5, Duration::from_secs(2));
    call(
        &service,
        Request::LevelPlay(LevelPlayParams {
            role: "L".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    events.wait_play_end(started.job, "L", "done", Duration::from_secs(10));
    call(&service, Request::LevelMonitorStop, Arc::new(NoEvents));
    assert_eq!(
        events.wait_finished(started.job, Duration::from_secs(2)),
        JobStatus::Cancelled
    );

    let state = audio.state.lock().unwrap();
    let idle_stop = state
        .history
        .iter()
        .position(|entry| *entry == "input stop")
        .unwrap();
    let output_open = state
        .history
        .iter()
        .position(|entry| *entry == "output open")
        .unwrap();
    assert!(idle_stop < output_open, "{:?}", state.history);
    assert_eq!(state.active_inputs_during_play, [1]);
    drop(state);
    fs::remove_dir_all(root).unwrap();
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
    events.wait_play_end(started.job, "L", "done", Duration::from_secs(10));
    let values = events.values.lock().unwrap();
    let last_meter = values
        .iter()
        .rposition(|event| matches!(event, JobEvent::Meter { job, .. } if *job == started.job))
        .unwrap();
    let ended = values
        .iter()
        .position(|event| {
            matches!(event, JobEvent::Log { job, code, params }
            if *job == started.job && code == "level_play_end"
                && params.get("role").is_some_and(|value| value == "L")
                && params.get("outcome").is_some_and(|value| value == "done"))
        })
        .unwrap();
    assert!(last_meter < ended, "{values:?}");
    drop(values);
    let Response::LevelsGet(levels) = call(&service, Request::LevelsGet, Arc::new(NoEvents)) else {
        panic!()
    };
    assert!(levels.levels.contains_key("L"));
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
fn monitor_started_in_another_project_does_not_store_late_level() {
    let root = root();
    let first = root.join("first.secs");
    let second = root.join("second.secs");
    let service = Service::new(Default::default());
    configure_mock(&service, &first);
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
        Request::LevelPlay(LevelPlayParams {
            role: "L".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    events.wait_meters(started.job, 1, Duration::from_secs(1));
    configure_mock(&service, &second);
    call(
        &service,
        Request::ProjectSave(ProjectSaveParams::default()),
        Arc::new(NoEvents),
    );
    events.wait_play_end(started.job, "L", "done", Duration::from_secs(10));
    let Response::LevelsGet(levels) = call(&service, Request::LevelsGet, Arc::new(NoEvents)) else {
        panic!()
    };
    assert!(levels.levels.is_empty());
    assert!(levels.spl.is_empty());
    let Response::ProjectInfo(info) = call(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert!(!info.dirty);
    call(&service, Request::LevelMonitorStop, Arc::new(NoEvents));
    assert_eq!(
        events.wait_finished(started.job, Duration::from_secs(1)),
        JobStatus::Cancelled
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn level_stop_cancels_only_the_active_burst_and_is_idempotent() {
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
        Request::LevelPlay(LevelPlayParams {
            role: "L".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    std::thread::sleep(Duration::from_millis(50));
    call(&service, Request::LevelStop, Arc::new(NoEvents));
    call(&service, Request::LevelStop, Arc::new(NoEvents));
    events.wait_play_end(started.job, "L", "stopped", Duration::from_secs(1));
    assert!(
        !events.values.lock().unwrap().iter().any(|event| {
            matches!(event, JobEvent::Finished { job, .. } if *job == started.job)
        })
    );
    call(
        &service,
        Request::LevelPlay(LevelPlayParams {
            role: "R".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    events.wait_meters(started.job, 1, Duration::from_secs(5));
    call(&service, Request::LevelMonitorStop, Arc::new(NoEvents));
    assert_eq!(
        events.wait_finished(started.job, Duration::from_secs(1)),
        JobStatus::Cancelled
    );
    assert!(matches!(
        service.call(Request::LevelStop, Arc::new(NoEvents)),
        Envelope::Ok(response) if *response == Response::LevelStop(())
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_new_level_play_stops_the_active_burst() {
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
        Request::LevelPlay(LevelPlayParams {
            role: "L".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    events.wait_meters(started.job, 1, Duration::from_secs(1));
    call(
        &service,
        Request::LevelPlay(LevelPlayParams {
            role: "R".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    events.wait_play_end(started.job, "L", "stopped", Duration::from_secs(1));
    events.wait_play_end(started.job, "R", "done", Duration::from_secs(10));
    call(&service, Request::LevelMonitorStop, Arc::new(NoEvents));
    assert_eq!(
        events.wait_finished(started.job, Duration::from_secs(1)),
        JobStatus::Cancelled
    );
    fs::remove_dir_all(root).unwrap();
}

#[derive(Debug, Clone, Default)]
struct RecordingRew {
    calls: Arc<Mutex<Vec<secs_rew::Request>>>,
}

impl RecordingRew {
    fn calls(&self) -> Vec<secs_rew::Request> {
        self.calls.lock().unwrap().clone()
    }
}

impl secs_rew::Transport for RecordingRew {
    fn request(
        &self,
        request: &secs_rew::Request,
    ) -> Result<secs_rew::Response, secs_rew::RewError> {
        self.calls.lock().unwrap().push(request.clone());
        let body = match request.path.as_str() {
            "/audio/driver" => serde_json::json!({"driver":"Java"}),
            "/audio/java/output-channels" => serde_json::json!(["L", "R"]),
            _ => serde_json::json!({"message":"ok"}),
        };
        Ok(secs_rew::Response::ok(body))
    }
}

#[derive(Debug, Clone, Default)]
struct FailingRew {
    calls: Arc<Mutex<Vec<secs_rew::Request>>>,
}

impl secs_rew::Transport for FailingRew {
    fn request(
        &self,
        request: &secs_rew::Request,
    ) -> Result<secs_rew::Response, secs_rew::RewError> {
        self.calls.lock().unwrap().push(request.clone());
        if request.path == "/generator/command"
            && request
                .body
                .as_ref()
                .is_some_and(|body| body["command"] == "Play")
        {
            return Err(secs_rew::RewError::Connection {
                path: request.path.clone(),
                message: "injected failure after play".into(),
            });
        }
        Ok(secs_rew::Response::ok(serde_json::json!({"message":"ok"})))
    }
}

#[test]
fn rew_monitor_cleans_up_after_loop_error() {
    let root = root();
    let transport = FailingRew::default();
    let injected = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(injected.clone())),
        ..Default::default()
    });
    configure(&service, &root.join("room.secs"), "rew");
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
        Request::LevelPlay(LevelPlayParams {
            role: "L".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    assert_eq!(
        events.wait_finished(started.job, Duration::from_secs(2)),
        JobStatus::Failed
    );
    let calls = transport.calls.lock().unwrap();
    assert!(calls.iter().any(|call| {
        call.path == "/generator/command"
            && call
                .body
                .as_ref()
                .is_some_and(|body| body["command"] == "Stop")
    }));
    assert!(
        calls
            .iter()
            .any(|call| call.path == "/input-levels/unsubscribe")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rew_level_play_ends_once_after_its_duration() {
    let root = root();
    let transport = RecordingRew::default();
    let injected = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(injected.clone())),
        ..Default::default()
    });
    configure(&service, &root.join("room.secs"), "rew");
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
        Request::LevelPlay(LevelPlayParams {
            role: "L".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    events.wait_play_end(started.job, "L", "done", Duration::from_secs(4));
    call(&service, Request::LevelMonitorStop, Arc::new(NoEvents));
    assert_eq!(
        events.wait_finished(started.job, Duration::from_secs(1)),
        JobStatus::Cancelled
    );
    let calls = transport.calls();
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.path == "/generator/command"
                && call
                    .body
                    .as_ref()
                    .is_some_and(|body| body["command"] == "Play"))
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.path == "/generator/command"
                && call
                    .body
                    .as_ref()
                    .is_some_and(|body| body["command"] == "Stop"))
            .count(),
        2
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rew_level_stop_stops_generator_but_keeps_subscription_alive() {
    let root = root();
    let transport = RecordingRew::default();
    let injected = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(injected.clone())),
        ..Default::default()
    });
    configure(&service, &root.join("room.secs"), "rew");
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
        Request::LevelPlay(LevelPlayParams {
            role: "L".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while transport
        .calls()
        .iter()
        .filter(|call| {
            call.path == "/generator/command"
                && call
                    .body
                    .as_ref()
                    .is_some_and(|body| body["command"] == "Play")
        })
        .count()
        < 1
    {
        assert!(Instant::now() < deadline, "REW generator did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    call(&service, Request::LevelStop, Arc::new(NoEvents));
    events.wait_play_end(started.job, "L", "stopped", Duration::from_secs(1));
    let deadline = Instant::now() + Duration::from_secs(1);
    while transport
        .calls()
        .iter()
        .filter(|call| {
            call.path == "/generator/command"
                && call
                    .body
                    .as_ref()
                    .is_some_and(|body| body["command"] == "Stop")
        })
        .count()
        < 1
    {
        assert!(
            Instant::now() < deadline,
            "REW generator did not stop promptly"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !events.values.lock().unwrap().iter().any(|event| {
            matches!(event, JobEvent::Finished { job, .. } if *job == started.job)
        })
    );
    call(&service, Request::LevelMonitorStop, Arc::new(NoEvents));
    assert_eq!(
        events.wait_finished(started.job, Duration::from_secs(1)),
        JobStatus::Cancelled
    );
    let calls = transport.calls();
    let unsubscribe = calls
        .iter()
        .position(|call| call.path == "/input-levels/unsubscribe")
        .unwrap();
    let first_stop = calls
        .iter()
        .position(|call| {
            call.path == "/generator/command"
                && call
                    .body
                    .as_ref()
                    .is_some_and(|body| body["command"] == "Stop")
        })
        .unwrap();
    assert!(first_stop < unsubscribe);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn levels_methods_fill_only_missing_defaults() {
    let root = root();
    let service = Service::new(Default::default());
    configure_mock(&service, &root.join("room.secs"));
    let Response::LevelsGet(empty) = call(&service, Request::LevelsGet, Arc::new(NoEvents)) else {
        panic!()
    };
    assert!(empty.levels.is_empty());
    let Response::LevelsFillDefaults(defaults) =
        call(&service, Request::LevelsFillDefaults, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert_eq!(defaults.levels.len(), 2);
    assert!(defaults.levels.contains_key("L") && defaults.levels.contains_key("R"));
    assert_eq!(
        call(&service, Request::LevelsFillDefaults, Arc::new(NoEvents)),
        Response::LevelsFillDefaults(defaults)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn generic_cancel_ends_monitor_during_an_active_burst() {
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
        Request::LevelPlay(LevelPlayParams {
            role: "L".into(),
            master_dbfs: -30.0,
        }),
        Arc::new(NoEvents),
    );
    events.wait_meters(started.job, 1, Duration::from_secs(1));
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

fn failed_error(events: &Events, job: JobId) -> IpcError {
    assert_eq!(
        events.wait_finished(job, Duration::from_secs(10)),
        JobStatus::Failed
    );
    events
        .values
        .lock()
        .unwrap()
        .iter()
        .find_map(|event| match event {
            JobEvent::Finished {
                job: id,
                error: Some(error),
                ..
            } if *id == job => Some(error.clone()),
            _ => None,
        })
        .unwrap()
}

fn monitor_error(service: &Service, role: &str, play: bool) -> IpcError {
    let events = Arc::new(Events::default());
    let Response::LevelMonitorStart(started) = call(
        service,
        Request::LevelMonitorStart(LevelMonitorStartParams::default()),
        events.clone(),
    ) else {
        panic!()
    };
    if play {
        call(
            service,
            Request::LevelPlay(LevelPlayParams {
                role: role.into(),
                master_dbfs: -30.0,
            }),
            Arc::new(NoEvents),
        );
    }
    failed_error(&events, started.job)
}

#[test]
fn monitor_errors_are_classified() {
    // mock: a role outside the plan; native: no audio device (the null backend).
    for (method, role, code, retryable) in [
        ("mock", "Nope", ErrorCode::MeasurementInput, false),
        ("native", "L", ErrorCode::DeviceUnavailable, true),
    ] {
        let root = root();
        let service = Service::new(Default::default());
        configure(&service, &root.join("room.secs"), method);
        let got = monitor_error(&service, role, method == "mock");
        assert_eq!(
            (got.code, got.retryable),
            (code, retryable),
            "{method}: {got:?}"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn jobs_lists_a_running_job_and_then_its_end() {
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
    let snapshot = |service: &Service| {
        let Response::Jobs(jobs) = call(service, Request::Jobs, Arc::new(NoEvents)) else {
            panic!()
        };
        jobs.into_iter()
            .find(|job| job.job == started.job)
            .expect("the started job is listed")
    };
    let running = snapshot(&service);
    assert_eq!(running.kind, JobKind::LevelMonitor);
    assert_eq!(running.status, JobStatus::Running);
    assert!(running.cancellable);
    call(&service, Request::LevelMonitorStop, Arc::new(NoEvents));
    assert_eq!(
        events.wait_finished(started.job, Duration::from_secs(5)),
        JobStatus::Cancelled
    );
    let ended = snapshot(&service);
    assert_eq!(ended.status, JobStatus::Cancelled);
    assert!(!ended.cancellable);
    fs::remove_dir_all(root).unwrap();
}
