//! Deterministic, device-free convolution backend. One duplex session at a time.
use std::sync::{Arc, Mutex};

use crate::audio::*;
use crate::estimator::convolve;
use crate::{check_cancel, invalid};

#[derive(Debug)]
struct Room {
    sample_rate: u32,
    paths: Vec<Vec<Vec<f64>>>,
    latency: usize,
    noise_rms: f64,
    seed: u64,
}

#[derive(Debug, Default)]
struct State {
    input_open: bool,
    output_open: bool,
    captured: Option<Vec<Vec<f64>>>,
}

/// `paths[output][input][sample]`. Noise is uniform with the specified RMS
/// dBFS, generated in capture order from a fixed seed. `NEG_INFINITY` is silent.
/// The simulator does not clip: overload is observable by the level checker.
#[derive(Clone, Debug)]
pub struct SimRoom {
    room: Arc<Room>,
    state: Arc<Mutex<State>>,
}

impl SimRoom {
    pub fn new(
        sample_rate: u32,
        paths: Vec<Vec<Vec<f64>>>,
        latency: usize,
        noise_dbfs: f64,
        seed: u64,
    ) -> Result<Self, AudioError> {
        let inputs = paths.first().map_or(0, Vec::len);
        if sample_rate == 0
            || paths.is_empty()
            || paths.len() > 64
            || inputs == 0
            || inputs > 64
            || latency > sample_rate as usize * 10
            || !(noise_dbfs == f64::NEG_INFINITY || noise_dbfs.is_finite() && noise_dbfs <= 0.0)
            || paths.iter().any(|p| {
                p.len() != inputs
                    || p.iter()
                        .any(|ir| ir.is_empty() || !ir.iter().all(|x| x.is_finite()))
            })
        {
            return Err(invalid("invalid room paths, rate, latency or noise"));
        }
        Ok(Self {
            room: Arc::new(Room {
                sample_rate,
                paths,
                latency,
                noise_rms: 10_f64.powf(noise_dbfs / 20.0),
                seed,
            }),
            state: Arc::new(Mutex::new(State::default())),
        })
    }

    pub fn endpoint(&self) -> Endpoint {
        Endpoint {
            id: "sim-room".into(),
            name: "Simulated room".into(),
            host_api: "simulation".into(),
            max_input_channels: self.room.paths[0].len() as u16,
            max_output_channels: self.room.paths.len() as u16,
            default_samplerate: f64::from(self.room.sample_rate),
            is_default_input: true,
            is_default_output: true,
        }
    }

    fn validate(
        &self,
        endpoint: &Endpoint,
        spec: StreamSpec,
        direction: Direction,
    ) -> Result<(), AudioError> {
        if endpoint.id != "sim-room" {
            return Err(AudioError::DeviceNotFound(endpoint.id.clone()));
        }
        let max = match direction {
            Direction::Input => self.room.paths[0].len(),
            Direction::Output => self.room.paths.len(),
        };
        if spec.sample_rate != self.room.sample_rate
            || spec.channels == 0
            || usize::from(spec.channels) > max
        {
            return Err(invalid("unsupported simulation format"));
        }
        Ok(())
    }
}

impl AudioBackend for SimRoom {
    fn name(&self) -> &'static str {
        "simulation"
    }
    fn enumerate(&self) -> Result<Vec<Endpoint>, AudioError> {
        Ok(vec![self.endpoint()])
    }
    fn probe(
        &self,
        endpoint: &Endpoint,
        direction: Direction,
        spec: StreamSpec,
        mode: ShareMode,
    ) -> Result<ProbeResult, AudioError> {
        self.validate(endpoint, spec, direction)?;
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
        endpoint: &Endpoint,
        spec: StreamSpec,
        mode: ShareMode,
    ) -> Result<Box<dyn OutputSession>, AudioError> {
        self.validate(endpoint, spec, Direction::Output)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| invalid("simulation lock poisoned"))?;
        if state.output_open {
            return Err(invalid("simulation output is busy"));
        }
        state.output_open = true;
        Ok(Box::new(Output {
            backend: self.clone(),
            spec,
            mode,
        }))
    }
    fn open_input(
        &self,
        endpoint: &Endpoint,
        spec: StreamSpec,
        _: ShareMode,
    ) -> Result<Box<dyn InputSession>, AudioError> {
        self.validate(endpoint, spec, Direction::Input)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| invalid("simulation lock poisoned"))?;
        if state.input_open {
            return Err(invalid("simulation input is busy"));
        }
        state.input_open = true;
        state.captured = None;
        Ok(Box::new(Input {
            backend: self.clone(),
            spec,
            cursor: 0,
            rng: Noise::new(self.room.seed),
            started: false,
        }))
    }
}

struct Output {
    backend: SimRoom,
    spec: StreamSpec,
    mode: ShareMode,
}
impl OutputSession for Output {
    fn play_to_completion(
        &mut self,
        interleaved: &[f32],
        cancel: &CancelToken,
    ) -> Result<PlaybackReport, AudioError> {
        let channels = usize::from(self.spec.channels);
        if !interleaved.len().is_multiple_of(channels) || !interleaved.iter().all(|x| x.is_finite())
        {
            return Err(invalid("invalid simulated playback"));
        }
        let frames = interleaved.len() / channels;
        let room = &self.backend.room;
        let mut captured = vec![Vec::<f64>::new(); room.paths[0].len()];
        for output in 0..channels {
            check_cancel(cancel)?;
            let signal: Vec<_> = interleaved
                .chunks_exact(channels)
                .map(|x| f64::from(x[output]))
                .collect();
            for (input, result) in captured.iter_mut().enumerate() {
                check_cancel(cancel)?;
                let response = convolve(&signal, &room.paths[output][input]);
                result.resize(result.len().max(response.len() + room.latency), 0.0);
                for (i, x) in response.iter().enumerate() {
                    result[i + room.latency] += x;
                }
            }
        }
        check_cancel(cancel)?;
        self.backend
            .state
            .lock()
            .map_err(|_| invalid("simulation lock poisoned"))?
            .captured = Some(captured);
        Ok(PlaybackReport {
            frames_submitted: frames as u64,
            frames_drained: frames as u64,
            mode: self.mode,
            underruns: 0,
            cancelled: false,
        })
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        if let Ok(mut s) = self.backend.state.lock() {
            s.output_open = false;
        }
    }
}

struct Input {
    backend: SimRoom,
    spec: StreamSpec,
    cursor: usize,
    rng: Noise,
    started: bool,
}
impl InputSession for Input {
    fn start(&mut self) -> Result<(), AudioError> {
        self.started = true;
        Ok(())
    }
    fn read_into(
        &mut self,
        dst: &mut [f32],
        cancel: &CancelToken,
    ) -> Result<CaptureRead, AudioError> {
        check_cancel(cancel)?;
        if !self.started {
            return Err(invalid("simulation input not started"));
        }
        let state = self
            .backend
            .state
            .lock()
            .map_err(|_| invalid("simulation lock poisoned"))?;
        let Some(data) = &state.captured else {
            return Ok(CaptureRead {
                frames: 0,
                discontinuity: false,
                silent: true,
            });
        };
        let channels = usize::from(self.spec.channels);
        let frames = dst.len() / channels;
        for (i, frame) in dst.chunks_exact_mut(channels).enumerate() {
            for (c, sample) in frame.iter_mut().enumerate() {
                *sample = (data[c].get(self.cursor + i).copied().unwrap_or(0.0)
                    + self.rng.sample() * self.backend.room.noise_rms)
                    as f32;
            }
        }
        self.cursor += frames;
        Ok(CaptureRead {
            frames,
            discontinuity: false,
            silent: dst.iter().all(|&x| x == 0.0),
        })
    }
    fn stop(&mut self) -> Result<(), AudioError> {
        self.started = false;
        Ok(())
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        if let Ok(mut s) = self.backend.state.lock() {
            s.input_open = false;
            s.captured = None;
        }
    }
}

pub(crate) struct Noise(u64);
impl Noise {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    pub(crate) fn sample(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0) * 3_f64.sqrt()
    }
}
