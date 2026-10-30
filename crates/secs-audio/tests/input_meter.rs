#![forbid(unsafe_code)]

use std::cell::Cell;
use std::sync::{Arc, Mutex};

use secs_audio::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Fault {
    #[default]
    None,
    Start,
    Read,
    Stop,
}

#[derive(Debug, Default)]
struct State {
    opens: Vec<(StreamSpec, ShareMode)>,
    starts: usize,
    reads: usize,
    stops: usize,
}

#[derive(Clone, Debug, Default)]
struct Fake {
    state: Arc<Mutex<State>>,
    fault: Fault,
}

impl AudioBackend for Fake {
    fn name(&self) -> &'static str {
        "input-meter-fake"
    }

    fn enumerate(&self) -> Result<Vec<Endpoint>, AudioError> {
        Ok(vec![endpoint()])
    }

    fn probe(
        &self,
        _endpoint: &Endpoint,
        _direction: Direction,
        spec: StreamSpec,
        mode: ShareMode,
    ) -> Result<ProbeResult, AudioError> {
        Ok(ProbeResult {
            supported: true,
            mode,
            native_sample_rate: spec.sample_rate,
            native_channels: spec.channels,
            detail: String::new(),
        })
    }

    fn open_output(
        &self,
        _endpoint: &Endpoint,
        _spec: StreamSpec,
        _mode: ShareMode,
    ) -> Result<Box<dyn OutputSession>, AudioError> {
        Err(AudioError::Backend("output must not open".into()))
    }

    fn open_input(
        &self,
        _endpoint: &Endpoint,
        spec: StreamSpec,
        mode: ShareMode,
    ) -> Result<Box<dyn InputSession>, AudioError> {
        self.state.lock().unwrap().opens.push((spec, mode));
        Ok(Box::new(Input {
            state: self.state.clone(),
            fault: self.fault,
            spec,
            read_index: 0,
        }))
    }
}

struct Input {
    state: Arc<Mutex<State>>,
    fault: Fault,
    spec: StreamSpec,
    read_index: usize,
}

impl InputSession for Input {
    fn start(&mut self) -> Result<(), AudioError> {
        self.state.lock().unwrap().starts += 1;
        if self.fault == Fault::Start {
            Err(AudioError::Backend("start".into()))
        } else {
            Ok(())
        }
    }

    fn read_into(
        &mut self,
        dst: &mut [f32],
        _cancel: &CancelToken,
    ) -> Result<CaptureRead, AudioError> {
        self.state.lock().unwrap().reads += 1;
        if self.fault == Fault::Read {
            return Err(AudioError::Backend("read".into()));
        }
        let channels = usize::from(self.spec.channels);
        let frames = dst.len() / channels;
        let selected = match self.read_index {
            0 | 1 => 0.25,
            2 => f32::NAN,
            _ => 0.5,
        };
        for frame in dst.chunks_exact_mut(channels) {
            frame.fill(0.9);
            frame[channels - 1] = selected;
        }
        let discontinuity = self.read_index == 0;
        self.read_index += 1;
        Ok(CaptureRead {
            frames,
            discontinuity,
            silent: false,
        })
    }

    fn stop(&mut self) -> Result<(), AudioError> {
        self.state.lock().unwrap().stops += 1;
        if self.fault == Fault::Stop {
            Err(AudioError::Backend("stop".into()))
        } else {
            Ok(())
        }
    }
}

fn endpoint() -> Endpoint {
    Endpoint {
        id: "meter".into(),
        name: "meter".into(),
        host_api: "fake".into(),
        max_input_channels: 3,
        max_output_channels: 0,
        default_samplerate: 1_000.0,
        is_default_input: true,
        is_default_output: false,
    }
}

fn input() -> LevelInput {
    LevelInput {
        endpoint: endpoint(),
        channel: 2,
        calibration: None,
        spl_reference: None,
    }
}

#[test]
fn input_meter_reports_selected_channel_and_tolerates_skipped_windows() {
    let fake = Fake::default();
    let reports = Arc::new(Mutex::new(Vec::new()));
    let report_count = Cell::new(0);
    input_meter(
        &fake,
        &input(),
        1_000,
        SharePreference::Shared,
        &CancelToken::new(),
        &mut || report_count.get() == 2,
        &mut |report| {
            reports.lock().unwrap().push(report);
            report_count.set(report_count.get() + 1);
        },
    )
    .unwrap();

    let reports = reports.lock().unwrap();
    assert_eq!(reports.len(), 2);
    assert!((reports[0].input_dbfs - 20.0 * 0.25_f64.log10()).abs() < 1e-12);
    assert!((reports[1].input_dbfs - 20.0 * 0.5_f64.log10()).abs() < 1e-12);
    let state = fake.state.lock().unwrap();
    assert_eq!(
        state.opens,
        [(
            StreamSpec {
                sample_rate: 1_000,
                channels: 2,
            },
            ShareMode::SharedAutoConvert,
        )]
    );
    assert_eq!((state.starts, state.reads, state.stops), (1, 4, 1));
}

#[test]
fn input_meter_stops_on_predicate_and_cancellation() {
    let stopped = Fake::default();
    input_meter(
        &stopped,
        &input(),
        1_000,
        SharePreference::Auto,
        &CancelToken::new(),
        &mut || true,
        &mut |_| panic!("no report after an immediate stop"),
    )
    .unwrap();
    let state = stopped.state.lock().unwrap();
    assert_eq!((state.starts, state.reads, state.stops), (1, 0, 1));
    drop(state);

    let cancelled = Fake::default();
    let cancel = CancelToken::new();
    let callback_cancel = cancel.clone();
    let error = input_meter(
        &cancelled,
        &input(),
        1_000,
        SharePreference::Auto,
        &cancel,
        &mut || false,
        &mut |_| callback_cancel.cancel(),
    )
    .unwrap_err();
    assert!(matches!(error, AudioError::Cancelled));
    let state = cancelled.state.lock().unwrap();
    assert_eq!((state.starts, state.reads, state.stops), (1, 2, 1));
}

#[test]
fn input_meter_returns_backend_errors_and_always_stops_an_open_stream() {
    for (fault, detail, starts, reads) in [
        (Fault::Start, "start", 1, 0),
        (Fault::Read, "read", 1, 1),
        (Fault::Stop, "stop", 1, 0),
    ] {
        let fake = Fake {
            fault,
            ..Fake::default()
        };
        let error = input_meter(
            &fake,
            &input(),
            1_000,
            SharePreference::Auto,
            &CancelToken::new(),
            &mut || fault == Fault::Stop,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(matches!(error, AudioError::Backend(ref got) if got == detail));
        let state = fake.state.lock().unwrap();
        assert_eq!((state.starts, state.reads, state.stops), (starts, reads, 1));
    }
}
