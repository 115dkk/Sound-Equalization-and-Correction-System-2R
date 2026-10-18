//! One clock and one duplex session per position. Channels are one-based.
use crate::audio::*;
use crate::estimator::{SweepEstimator, sweep_length_samples};
use crate::session::{PlaybackBuffer, SessionEvent, SessionRequest, play_and_record};
use crate::{check_cancel, invalid};

pub const LOW_SNR_DB: f64 = 20.0;

#[derive(Clone, Debug)]
pub struct RoleOutput {
    pub role: String,
    pub channel: u16,
}

#[derive(Clone, Debug)]
pub struct MeasurementPlan {
    pub output: Endpoint,
    pub input: Endpoint,
    pub roles: Vec<RoleOutput>,
    pub input_channel: u16,
    /// A wired copy of the first role's output; it must contain that sweep.
    pub loopback_input_channel: Option<u16>,
    pub sample_rate: u32,
    pub sweep_length: String,
    pub start_frequency: f64,
    pub end_frequency: f64,
    pub level_dbfs: f64,
    pub share: SharePreference,
    pub window: MeasurementWindow,
}

#[derive(Clone, Copy, Debug)]
pub struct MeasurementWindow {
    /// Requested decay gap; extended to preserve the complete IR window.
    pub gap_seconds: f64,
    /// 10 ms retains the engine's 2 ms pre-onset window and filter pre-ringing.
    pub pre_roll_seconds: f64,
    /// 2 s is the engine's long decay-analysis window, without a tail taper.
    pub post_onset_seconds: f64,
    /// Search/capture budget includes device start skew and room propagation.
    pub max_latency_seconds: f64,
    /// Independent, silent baseline before any sweep (not a previous room tail).
    pub noise_seconds: f64,
}
impl Default for MeasurementWindow {
    fn default() -> Self {
        Self {
            gap_seconds: 1.5,
            pre_roll_seconds: 0.01,
            post_onset_seconds: 2.0,
            max_latency_seconds: 0.5,
            noise_seconds: 0.25,
        }
    }
}
impl MeasurementPlan {
    pub fn new(output: Endpoint, input: Endpoint, roles: Vec<RoleOutput>) -> Self {
        Self {
            output,
            input,
            roles,
            input_channel: 1,
            loopback_input_channel: None,
            sample_rate: 48_000,
            sweep_length: "256k".into(),
            start_frequency: 20.0,
            end_frequency: 20_000.0,
            level_dbfs: -20.0,
            share: SharePreference::Auto,
            window: MeasurementWindow::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MeasuredRole {
    pub role: String,
    pub position: u32,
    pub sample_rate: u32,
    pub ir: Vec<f32>,
    /// First sample relative to that role's sweep start, in seconds. Common
    /// capture/output start skew cancels in `t0 + onset/fs` between roles.
    pub t0: f64,
    /// Direct 10 ms IR energy / expected deconvolved baseline-noise energy.
    pub snr_db: f64,
}

#[derive(Clone, Debug)]
pub struct PositionMeasurement {
    pub roles: Vec<MeasuredRole>,
    pub latency_samples: Option<usize>,
}

#[derive(Debug)]
pub struct SweepSequence {
    pub playback: PlaybackBuffer,
    pub sweep_starts: Vec<usize>,
    pub stride: usize,
    pub baseline_frames: usize,
}

/// Impulcifer's leading silence + ordered sweeps + silence, adapted to explicit
/// channel routing. The gap is at least post-onset + latency + two pre-rolls,
/// so the next role cannot leak into the retained long window.
pub fn sweep_sequence(
    plan: &MeasurementPlan,
    estimator: &SweepEstimator,
) -> Result<SweepSequence, AudioError> {
    let w = plan.window;
    let rate = f64::from(plan.sample_rate);
    if plan.sample_rate != estimator.fs
        || plan.roles.is_empty()
        || plan.roles.len() > 64
        || plan.input_channel == 0
        || plan.input_channel > plan.input.max_input_channels
        || plan
            .loopback_input_channel
            .is_some_and(|c| c == 0 || c > plan.input.max_input_channels || c == plan.input_channel)
        || [
            w.gap_seconds,
            w.pre_roll_seconds,
            w.post_onset_seconds,
            w.max_latency_seconds,
            w.noise_seconds,
        ]
        .iter()
        .any(|x| !x.is_finite() || *x <= 0.0 || *x > 30.0)
        || w.post_onset_seconds < 2.0
        || w.pre_roll_seconds >= w.noise_seconds
        || w.noise_seconds - w.pre_roll_seconds < 0.05
        || w.max_latency_seconds * rate < 1.0
        || w.pre_roll_seconds * rate < 1.0
        || plan
            .input_channel
            .max(plan.loopback_input_channel.unwrap_or(1))
            > 64
    {
        return Err(invalid("invalid measurement channels or window"));
    }
    for (i, role) in plan.roles.iter().enumerate() {
        if role.role.is_empty()
            || role.channel == 0
            || role.channel > plan.output.max_output_channels
            || plan.roles[..i].iter().any(|r| r.role == role.role)
        {
            return Err(invalid("empty/duplicate role or invalid output channel"));
        }
    }
    let channels = plan.roles.iter().map(|r| r.channel).max().unwrap();
    let baseline_frames = (w.noise_seconds * rate).round() as usize;
    let gap = (w
        .gap_seconds
        .max(w.post_onset_seconds + w.max_latency_seconds + 2.0 * w.pre_roll_seconds)
        * rate)
        .ceil() as usize;
    let stride = estimator.test_signal.len() + gap;
    let frames = baseline_frames + stride * plan.roles.len();
    let count = frames
        .checked_mul(usize::from(channels))
        .ok_or_else(|| invalid("sequence size overflow"))?;
    // Bounded up front, before opening hardware (512 MiB interleaved transport).
    if count > 134_217_728 {
        return Err(invalid("measurement sequence exceeds 512 MiB"));
    }
    let mut interleaved = vec![0.0; count];
    let mut sweep_starts = Vec::new();
    for (i, role) in plan.roles.iter().enumerate() {
        let start = baseline_frames + stride * i;
        sweep_starts.push(start);
        for (j, &x) in estimator.test_signal.iter().enumerate() {
            interleaved[(start + j) * usize::from(channels) + usize::from(role.channel - 1)] =
                x as f32;
        }
    }
    Ok(SweepSequence {
        playback: PlaybackBuffer {
            sample_rate: plan.sample_rate,
            channels,
            interleaved,
        },
        sweep_starts,
        stride,
        baseline_frames,
    })
}

pub fn measure_position(
    backend: &dyn AudioBackend,
    plan: &MeasurementPlan,
    position: u32,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(SessionEvent),
) -> Result<PositionMeasurement, AudioError> {
    check_cancel(cancel)?;
    if position == 0 {
        return Err(invalid("position must be positive"));
    }
    let estimator = SweepEstimator::new(
        plan.sample_rate,
        plan.start_frequency,
        plan.end_frequency,
        sweep_length_samples(&plan.sweep_length)?,
        plan.level_dbfs,
        0.005,
    )?;
    check_cancel(cancel)?;
    let sequence = sweep_sequence(plan, &estimator)?;
    let input_channels = plan
        .input_channel
        .max(plan.loopback_input_channel.unwrap_or(1));
    let mut request = SessionRequest::new(
        plan.output.clone(),
        plan.input.clone(),
        sequence.playback,
        input_channels,
    );
    request.share = plan.share;
    let recording = play_and_record(backend, request, cancel, on_event)?;
    check_cancel(cancel)?;
    if recording.capture.discontinuity {
        return Err(AudioError::Recording {
            issue: RecordingIssue::Gap,
            detail: "capture reported a discontinuity".into(),
        });
    }
    if recording.playback.underruns > 0 {
        return Err(AudioError::Recording {
            issue: RecordingIssue::Underrun,
            detail: format!(
                "playback reported {} underrun(s)",
                recording.playback.underruns
            ),
        });
    }
    if !recording.interleaved.iter().all(|x| x.is_finite()) {
        return Err(AudioError::Recording {
            issue: RecordingIssue::NonFinite,
            detail: "capture contained non-finite samples".into(),
        });
    }
    let channel = |c: u16| -> Vec<f64> {
        recording
            .interleaved
            .chunks_exact(usize::from(input_channels))
            .map(|f| f64::from(f[usize::from(c - 1)]))
            .collect()
    };
    let mic = channel(plan.input_channel);
    let rate = f64::from(plan.sample_rate);
    let pre = (plan.window.pre_roll_seconds * rate).round() as usize;
    let post = (plan.window.post_onset_seconds * rate).ceil() as usize;
    let max_latency = (plan.window.max_latency_seconds * rate).ceil() as usize;
    let baseline = &mic[..sequence.baseline_frames.saturating_sub(pre)];
    let noise_power = power(baseline);
    let loopback = plan.loopback_input_channel.map(channel);
    let mut latency_samples = None;
    let mut roles = Vec::new();
    for (index, role) in plan.roles.iter().enumerate() {
        check_cancel(cancel)?;
        let start = sequence.sweep_starts[index] - pre;
        let end = start + sequence.stride;
        if let Some(loopback) = &loopback
            && index == 0
        {
            let ir = estimator.estimate(&loopback[start..end])?;
            let peak = peak_index(&ir[..(pre + max_latency).min(ir.len())]);
            if ir[peak].abs() < 1e-8 || peak < pre {
                return Err(invalid("missing loopback reference"));
            }
            latency_samples = Some(peak - pre);
        }
        let ir = estimator.estimate_with_noise(&mic[start..end], noise_power)?;
        check_cancel(cancel)?;
        let search = &ir[..(pre + max_latency).min(ir.len())];
        let peak = peak_index(search);
        if search[peak].abs() < 1e-12 {
            return Err(invalid("no measurable direct sound"));
        }
        let onset = (0..=peak)
            .find(|&i| ir[i].abs() >= ir[peak].abs() * 0.1)
            .unwrap_or(peak);
        let crop_start = onset as isize - pre as isize;
        let cropped: Vec<f32> = (0..pre + post)
            .map(|i| {
                let j = crop_start + i as isize;
                if j < 0 {
                    0.0
                } else {
                    ir.get(j as usize).copied().unwrap_or(0.0) as f32
                }
            })
            .collect();
        // SNR is measured on the Farina IR before colour flattening: stationary
        // white baseline noise has variance sigma² * sum(inverse²). This avoids
        // spuriously treating the inverse's processing gain as microphone gain.
        let raw = estimator.estimate_raw(&mic[start..end]);
        let noise_ir_power =
            noise_power * estimator.inverse_filter.iter().map(|x| x * x).sum::<f64>();
        let raw_origin = estimator.test_signal.len() - 1;
        let direct_start =
            raw_origin + peak_index(&raw[raw_origin..raw_origin + pre + max_latency]);
        let direct_end = (direct_start + (rate * 0.01).round() as usize).min(raw.len());
        let direct_power = power(&raw[direct_start..direct_end]);
        let snr_db = 10.0
            * ((direct_power - noise_ir_power).max(f64::MIN_POSITIVE) / noise_ir_power).log10();
        let t0 = (crop_start - pre as isize - latency_samples.unwrap_or(0) as isize) as f64 / rate;
        roles.push(MeasuredRole {
            role: role.role.clone(),
            position,
            sample_rate: plan.sample_rate,
            ir: cropped,
            t0,
            snr_db,
        });
    }
    check_cancel(cancel)?;
    Ok(PositionMeasurement {
        roles,
        latency_samples,
    })
}

pub(crate) fn power(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().map(|x| x * x).sum::<f64>() / samples.len() as f64
}
pub(crate) fn peak_index(samples: &[f64]) -> usize {
    samples
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
        .map_or(0, |(i, _)| i)
}
