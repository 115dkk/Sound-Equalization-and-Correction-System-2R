//! SECS 2 sample_dolby_714.npz, exported by oracle/export_sample_room.py.
//! Data provenance: tools/make_sample_dolby.py, SECS 2 (GPL-3.0-only).
//! Only position 1 has real data. No synthetic positions or missing-role fallback
//! are presented as measured room data.
use super::*;
use secs_project::{MeasurementPlan, MeasurementResult};
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Debug, serde::Deserialize)]
pub struct SampleIndex {
    pub sample_rate: u32,
    pub positions: Vec<i64>,
    pub source: String,
    pub entries: Vec<SampleEntry>,
}
#[derive(Debug, serde::Deserialize)]
pub struct SampleEntry {
    pub role: String,
    pub offset: usize,
    pub samples: usize,
    pub t0_s: f64,
    pub source: serde_json::Value,
}
pub const SAMPLE_BYTES: &[u8] = include_bytes!("../../data/sample_room.f32");
pub fn index() -> &'static SampleIndex {
    static INDEX: OnceLock<SampleIndex> = OnceLock::new();
    INDEX.get_or_init(|| {
        serde_json::from_str(include_str!("../../data/sample_room.json"))
            .expect("generated sample index")
    })
}
pub fn ir(role: &str, pos: i64) -> Result<(Vec<f32>, u32, f64), MeasureError> {
    if pos != 1 {
        return Err(MeasureError::Input("sample.single_position".into()));
    }
    let entry = index()
        .entries
        .iter()
        .find(|e| e.role == role)
        .ok_or_else(|| MeasureError::Input(format!("sample.role_unavailable: {role}")))?;
    let values = SAMPLE_BYTES[entry.offset..entry.offset + entry.samples * 4]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_le_bytes(*v))
        .collect();
    Ok((values, index().sample_rate, entry.t0_s))
}

const SYNTHETIC_RATE: u32 = 48_000;
const SYNTHETIC_SECONDS: u32 = 1;

/// Builds the deterministic deployment fallback used for roles without embedded data.
pub fn fallback_ir(role: &str) -> Vec<f32> {
    let rate = SYNTHETIC_RATE as usize;
    let length = rate * SYNTHETIC_SECONDS as usize;
    let direct = (0.003 * SYNTHETIC_RATE as f64).round() as usize;
    let mut random = Pcg32::new(20_260_916 + fnv1a(role.as_bytes()) % 100_000);
    let mut values = vec![0.0_f64; length];
    values[direct] = 1.0;
    let reflection_end = (0.08 * SYNTHETIC_RATE as f64) as usize;
    for _ in 0..12 {
        let index = direct + random.range(20, reflection_end);
        if index < length {
            values[index] += random.normal() * 0.18;
        }
    }
    for (index, value) in values.iter_mut().enumerate() {
        *value += 0.04 * random.normal() * (-(index as f64) / (0.12 * SYNTHETIC_RATE as f64)).exp();
    }
    let is_sub = role.to_ascii_uppercase().starts_with("SW");
    let modes = if is_sub {
        [28.0, 43.0, 67.0]
    } else {
        [52.0, 118.0, 240.0]
    };
    for (mode, frequency) in modes.into_iter().enumerate() {
        for (index, value) in values.iter_mut().enumerate() {
            let time = index as f64 / SYNTHETIC_RATE as f64;
            *value += 0.05 / (mode + 1) as f64
                * (std::f64::consts::TAU * frequency * time).sin()
                * (-time / 0.25).exp();
        }
    }
    if is_sub {
        let alpha = 1.0 - (-std::f64::consts::TAU * 120.0 / SYNTHETIC_RATE as f64).exp();
        for _ in 0..2 {
            let mut accumulator = 0.0;
            for value in &mut values {
                accumulator += alpha * (*value - accumulator);
                *value = accumulator;
            }
        }
    }
    values.into_iter().map(|value| value as f32).collect()
}

/// Applies the upstream per-position delay and attenuation to a synthetic IR.
pub fn position_variation(values: &[f32], position: u32) -> Vec<f32> {
    let step = position.saturating_sub(1);
    if step == 0 {
        return values.to_vec();
    }
    let delay = (f64::from(step) * 0.35e-3 * f64::from(SYNTHETIC_RATE)).round() as usize;
    let gain = 10.0_f32.powf(-0.4 * step as f32 / 20.0);
    let mut output = vec![0.0; values.len()];
    if delay < values.len() {
        for (target, value) in output[delay..].iter_mut().zip(values) {
            *target = *value * gain;
        }
    }
    output
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

struct Pcg32 {
    state: u64,
    spare: Option<f64>,
}

impl Pcg32 {
    fn new(seed: u64) -> Self {
        let mut random = Self {
            state: seed.wrapping_add(0x853c_49e6_748f_ea9b),
            spare: None,
        };
        let _ = random.next();
        random
    }

    fn next(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let shifted = (((old >> 18) ^ old) >> 27) as u32;
        shifted.rotate_right((old >> 59) as u32)
    }

    fn uniform(&mut self) -> f64 {
        (f64::from(self.next()) + 0.5) / (f64::from(u32::MAX) + 1.0)
    }

    fn range(&mut self, start: usize, end: usize) -> usize {
        start + (self.next() as usize % (end - start))
    }

    fn normal(&mut self) -> f64 {
        if let Some(value) = self.spare.take() {
            return value;
        }
        let radius = (-2.0 * self.uniform().ln()).sqrt();
        let angle = std::f64::consts::TAU * self.uniform();
        self.spare = Some(radius * angle.sin());
        radius * angle.cos()
    }
}
/// Exact, unmodified sample source for the REW adapter's FakeRew.
#[derive(Debug, Clone, Copy)]
pub struct SampleSource;
impl secs_rew::IrSource for SampleSource {
    fn impulse_response(
        &self,
        role: &str,
        pos: u32,
        sample_rate: u32,
    ) -> Result<secs_rew::ImpulseResponse, secs_rew::RewError> {
        let (ir, rate, t0_s) =
            ir(role, i64::from(pos)).map_err(|e| secs_rew::RewError::Schema {
                path: "sample".into(),
                message: e.to_string(),
            })?;
        if rate != sample_rate {
            return Err(secs_rew::RewError::Schema {
                path: "sample".into(),
                message: "sample.rate_unavailable".into(),
            });
        }
        Ok(secs_rew::ImpulseResponse {
            ir,
            sample_rate: rate,
            t0_s,
        })
    }
}

/// Embed all original samples in causal paths using a common origin. Round each
/// path offset to a sample, retaining inter-channel arrival within one sample.
pub fn room(plan: &MeasurementPlan, pos: i64) -> Result<(secs_audio::SimRoom, f64), MeasureError> {
    if pos != 1 {
        return Err(MeasureError::Input("sample.single_position".into()));
    }
    if plan.sample_rate != i64::from(index().sample_rate) {
        return Err(MeasureError::Input("sample.rate_unavailable".into()));
    }
    let origin_samples = index()
        .entries
        .iter()
        .map(|e| e.t0_s * f64::from(index().sample_rate))
        .fold(f64::INFINITY, f64::min)
        .floor();
    let outputs = plan
        .roles
        .iter()
        .map(|r| positive(plan.output_for(r), "output"))
        .collect::<Result<Vec<_>, _>>()?;
    let count = outputs.iter().copied().max().unwrap_or(0) as usize;
    if count == 0
        || count > 64
        || outputs
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != outputs.len()
    {
        return Err(MeasureError::Input(
            "sample outputs must be distinct channels in 1..64".into(),
        ));
    }
    let mut paths = vec![vec![vec![0.0]]; count];
    for (role, output) in plan.roles.iter().zip(outputs) {
        let (samples, rate, t0) = ir(role, pos)?;
        let offset = (t0 * f64::from(rate) - origin_samples).round() as usize;
        let mut path = vec![0.0; offset];
        path.extend(samples.into_iter().map(f64::from));
        paths[output as usize - 1] = vec![path];
    }
    Ok((
        secs_audio::SimRoom::new(index().sample_rate, paths, 0, f64::NEG_INFINITY, 1)?,
        origin_samples / f64::from(index().sample_rate),
    ))
}

/// Default is the real single-position sample. Explicit test rooms may contain
/// multiple positions; they are never synthesized from the real sample silently.
#[derive(Debug, Default)]
pub struct MockMeasurer {
    rooms: Option<BTreeMap<i64, secs_audio::SimRoom>>,
    delay: std::time::Duration,
}
impl MockMeasurer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_rooms(rooms: BTreeMap<i64, secs_audio::SimRoom>) -> Self {
        Self {
            rooms: Some(rooms),
            delay: std::time::Duration::ZERO,
        }
    }
    /// Adds a cancellable simulated device wait, without changing room samples.
    pub fn with_delay(mut self, delay: std::time::Duration) -> Self {
        self.delay = delay;
        self
    }
    pub(crate) fn level_check_streaming(
        &mut self,
        plan: &MeasurementPlan,
        role: &str,
        seconds: f64,
        cancel: &CancelToken,
        on_level: &mut dyn FnMut(secs_audio::LevelReport),
    ) -> Result<secs_audio::LevelReport, MeasureError> {
        let (mut native, plan, _) = self.native(plan, 1)?;
        let mut next = std::time::Instant::now();
        native.level_check_streaming(&plan, role, seconds, cancel, &mut |report| {
            let wait = next.saturating_duration_since(std::time::Instant::now());
            if !wait.is_zero() {
                std::thread::sleep(wait);
            }
            on_level(report);
            next += std::time::Duration::from_millis(100);
        })
    }
    fn native(
        &self,
        plan: &MeasurementPlan,
        pos: i64,
    ) -> Result<(NativeMeasurer, MeasurementPlan, f64), MeasureError> {
        let (room, origin) = match &self.rooms {
            Some(rooms) => (
                rooms
                    .get(&pos)
                    .cloned()
                    .ok_or_else(|| MeasureError::Input("mock.position_unavailable".into()))?,
                0.0,
            ),
            None => room(plan, pos)?,
        };
        let mut p = plan.clone();
        p.output_device = room.endpoint().id.clone();
        p.input_device = room.endpoint().id;
        let mut settings = MeasurementSettings::default();
        // Preserve the sample's 200 ms pre-onset material, not just its peak.
        settings.window.pre_roll_seconds = 0.25;
        settings.window.noise_seconds = 0.5;
        settings.window.max_latency_seconds = 0.75;
        Ok((
            NativeMeasurer::from_backend(Box::new(room), settings),
            p,
            origin,
        ))
    }
}
impl Measurer for MockMeasurer {
    fn devices(&mut self) -> Result<Vec<Device>, MeasureError> {
        Ok(vec![Device {
            id: "sim-room".into(),
            name: "SECS sample room".into(),
            input_channels: 1,
            output_channels: 14,
        }])
    }
    fn status(&mut self) -> Result<Status, MeasureError> {
        Ok(Status {
            available: true,
            version: None,
            pro: None,
        })
    }
    fn level_check(
        &mut self,
        plan: &MeasurementPlan,
        role: &str,
        seconds: f64,
        cancel: &CancelToken,
    ) -> Result<secs_audio::LevelReport, MeasureError> {
        let (mut native, plan, _) = self.native(plan, 1)?;
        native.level_check(&plan, role, seconds, cancel)
    }
    fn measure_position(
        &mut self,
        plan: &MeasurementPlan,
        pos: i64,
        progress: Option<ProgressSink>,
        cancel: &CancelToken,
    ) -> Result<Vec<MeasurementResult>, MeasureError> {
        let start = std::time::Instant::now();
        while start.elapsed() < self.delay {
            if cancel.is_cancelled() {
                return Ok(vec![]);
            }
            std::thread::sleep(
                (self.delay - start.elapsed().min(self.delay))
                    .min(std::time::Duration::from_millis(5)),
            );
        }
        let (mut native, plan, origin) = self.native(plan, pos)?;
        let mut results = native.measure_position(&plan, pos, progress, cancel)?;
        for result in &mut results {
            result.t0_s += origin;
        }
        Ok(results)
    }
}
