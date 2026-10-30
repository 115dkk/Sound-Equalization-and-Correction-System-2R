//! Deterministic pink-noise and subwoofer-sine level checks.
use crate::audio::*;
use crate::calibration::{MicCalibration, SplReference};
use crate::invalid;
use crate::measurement::power;
use crate::policy::open_input_with_preference;
use crate::session::{PlaybackBuffer, SessionRequest, play_and_record};
use crate::sim_room::Noise;
use secs_dsp::fft;
use std::f64::consts::PI;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::time::Duration;

pub const TARGET_WINDOW: (f64, f64) = (-30.0, -15.0);

#[derive(Clone, Copy, Debug)]
pub enum LevelSignal {
    Pink,
    SubSine { frequency: f64 },
}
#[derive(Clone, Debug)]
pub struct LevelOutput {
    pub endpoint: Endpoint,
    pub channel: u16,
    pub sample_rate: u32,
    pub signal: LevelSignal,
    pub share: SharePreference,
}
#[derive(Clone, Debug)]
pub struct LevelInput {
    pub endpoint: Endpoint,
    pub channel: u16,
    pub calibration: Option<MicCalibration>,
    pub spl_reference: Option<SplReference>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelReport {
    pub input_dbfs: f64,
    pub peak_dbfs: f64,
    pub clip: bool,
    pub spl_db: Option<f64>,
}

/// Signal RMS equals `level_dbfs`; reject levels whose deterministic peak
/// would exceed full scale. Pink has 1/f power from 20 Hz to min(20 kHz, .45 fs).
pub fn level_signal(
    signal: LevelSignal,
    sample_rate: u32,
    level_dbfs: f64,
    seconds: f64,
) -> Result<Vec<f64>, AudioError> {
    if !(1000..=384_000).contains(&sample_rate)
        || !seconds.is_finite()
        || !(0.05..=30.0).contains(&seconds)
        || !level_dbfs.is_finite()
        || !(-120.0..=0.0).contains(&level_dbfs)
    {
        return Err(invalid("invalid level burst"));
    }
    let n = (seconds * f64::from(sample_rate)).round() as usize;
    let mut samples = match signal {
        LevelSignal::SubSine { frequency } => {
            if !frequency.is_finite()
                || frequency <= 0.0
                || frequency >= f64::from(sample_rate) / 2.0
            {
                return Err(invalid("invalid sine frequency"));
            }
            (0..n)
                .map(|i| (2.0 * PI * frequency * i as f64 / f64::from(sample_rate)).sin())
                .collect::<Vec<_>>()
        }
        LevelSignal::Pink => {
            let mut rng = Noise::new(0x53454353);
            let mut spectrum = vec![fft::Complex64::default(); n / 2 + 1];
            for (i, x) in spectrum.iter_mut().enumerate().skip(1) {
                let f = i as f64 * f64::from(sample_rate) / n as f64;
                if (20.0..=20_000.0_f64.min(f64::from(sample_rate) * 0.45)).contains(&f) {
                    *x = fft::Complex64::from_polar(
                        1.0 / f.sqrt(),
                        rng.sample() / 3_f64.sqrt() * PI,
                    );
                }
            }
            fft::irfft(&spectrum, n)
        }
    };
    let gain = 10_f64.powf(level_dbfs / 20.0) / power(&samples).sqrt();
    samples.iter_mut().for_each(|x| *x *= gain);
    if samples.iter().any(|x| x.abs() > 1.0) {
        return Err(invalid("level burst would clip output"));
    }
    Ok(samples)
}

pub fn analyse_level(
    samples: &[f64],
    sample_rate: u32,
    calibration: Option<&MicCalibration>,
    reference: Option<SplReference>,
) -> Result<LevelReport, AudioError> {
    if samples.is_empty() || sample_rate == 0 || !samples.iter().all(|x| x.is_finite()) {
        return Err(invalid("invalid level samples"));
    }
    let input_dbfs = 10.0 * power(samples).log10();
    let peak_dbfs = 20.0
        * samples
            .iter()
            .map(|x| x.abs())
            .fold(0.0_f64, f64::max)
            .log10();
    let reference = reference.or_else(|| calibration.and_then(MicCalibration::spl_reference));
    let spl_db = if let Some(reference) = reference {
        if !reference.offset_db.is_finite() {
            return Err(invalid("invalid SPL offset"));
        }
        let corrected = match calibration {
            Some(c) => c.corrected_rms_dbfs(samples, sample_rate)?,
            None => input_dbfs,
        };
        Some(corrected + reference.offset_db)
    } else {
        None
    };
    Ok(LevelReport {
        input_dbfs,
        peak_dbfs,
        clip: samples.iter().any(|x| x.abs() > 10_f64.powf(-1.0 / 20.0)),
        spl_db,
    })
}

/// Reports consecutive 0.1-second windows from one input channel until
/// cancellation or `stop_when` requests a normal stop.
///
/// Capture discontinuities and windows containing non-finite samples are
/// skipped. The callback runs on this function's caller, never in a backend
/// audio callback.
pub fn input_meter(
    backend: &dyn AudioBackend,
    input: &LevelInput,
    sample_rate: u32,
    share: SharePreference,
    cancel: &CancelToken,
    stop_when: &mut dyn FnMut() -> bool,
    on_level: &mut dyn FnMut(LevelReport),
) -> Result<(), AudioError> {
    crate::check_cancel(cancel)?;
    if !(1_000..=384_000).contains(&sample_rate)
        || input.channel == 0
        || input.channel > input.endpoint.max_input_channels
        || input.channel > 64
    {
        return Err(invalid("invalid input meter format"));
    }
    let channels = usize::from(input.channel);
    let window_frames = usize::try_from(sample_rate / 10)
        .map_err(|_| invalid("input meter window exceeds usize"))?
        .max(1);
    let sample_count = window_frames
        .checked_mul(channels)
        .ok_or_else(|| invalid("input meter buffer overflow"))?;
    let mut interleaved = vec![0.0; sample_count];
    let mut window = Vec::new();
    window
        .try_reserve_exact(window_frames)
        .map_err(|error| AudioError::Backend(format!("input meter allocation: {error}")))?;
    let (mut session, _) = open_input_with_preference(
        backend,
        &input.endpoint,
        StreamSpec {
            sample_rate,
            channels: input.channel,
        },
        share,
    )?;
    let captured = catch_unwind(AssertUnwindSafe(|| -> Result<(), AudioError> {
        session.start()?;
        let mut skip_window = false;
        loop {
            crate::check_cancel(cancel)?;
            if stop_when() {
                return Ok(());
            }
            let read = session.read_into(&mut interleaved, cancel)?;
            if read.frames > window_frames {
                return Err(AudioError::Backend(
                    "capture backend returned too many frames".into(),
                ));
            }
            skip_window |= read.discontinuity;
            for frame in interleaved[..read.frames * channels].chunks_exact(channels) {
                window.push(f64::from(frame[channels - 1]));
                if window.len() == window_frames {
                    if !skip_window && window.iter().all(|sample| sample.is_finite()) {
                        on_level(analyse_level(
                            &window,
                            sample_rate,
                            input.calibration.as_ref(),
                            input.spl_reference,
                        )?);
                    }
                    window.clear();
                    skip_window = false;
                    crate::check_cancel(cancel)?;
                    if stop_when() {
                        return Ok(());
                    }
                }
            }
            if read.frames == 0 {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }));
    let stopped = session.stop();
    match captured {
        Ok(result) => {
            result?;
            stopped
        }
        Err(payload) => {
            let _ = stopped;
            resume_unwind(payload)
        }
    }
}

pub fn level_check(
    backend: &dyn AudioBackend,
    output: &LevelOutput,
    input: &LevelInput,
    level_dbfs: f64,
    seconds: f64,
) -> Result<LevelReport, AudioError> {
    level_check_cancellable(
        backend,
        output,
        input,
        level_dbfs,
        seconds,
        &CancelToken::new(),
    )
}

pub fn level_check_cancellable(
    backend: &dyn AudioBackend,
    output: &LevelOutput,
    input: &LevelInput,
    level_dbfs: f64,
    seconds: f64,
    cancel: &CancelToken,
) -> Result<LevelReport, AudioError> {
    level_check_streaming(
        backend,
        output,
        input,
        level_dbfs,
        seconds,
        cancel,
        &mut |_| {},
    )
}

#[doc(hidden)]
pub fn level_check_streaming(
    backend: &dyn AudioBackend,
    output: &LevelOutput,
    input: &LevelInput,
    level_dbfs: f64,
    seconds: f64,
    cancel: &CancelToken,
    on_level: &mut dyn FnMut(LevelReport),
) -> Result<LevelReport, AudioError> {
    crate::check_cancel(cancel)?;
    if output.channel == 0
        || output.channel > output.endpoint.max_output_channels
        || input.channel == 0
        || input.channel > input.endpoint.max_input_channels
    {
        return Err(invalid("invalid level channel"));
    }
    let signal = level_signal(output.signal, output.sample_rate, level_dbfs, seconds)?;
    let count = signal
        .len()
        .checked_mul(usize::from(output.channel))
        .ok_or_else(|| invalid("level buffer overflow"))?;
    if count > 134_217_728 || input.channel > 64 {
        return Err(invalid("level buffer exceeds budget"));
    }
    let mut interleaved = vec![0.0; count];
    for (frame, &sample) in interleaved
        .chunks_exact_mut(usize::from(output.channel))
        .zip(&signal)
    {
        frame[usize::from(output.channel - 1)] = sample as f32;
    }
    let mut request = SessionRequest::new(
        output.endpoint.clone(),
        input.endpoint.clone(),
        PlaybackBuffer {
            sample_rate: output.sample_rate,
            channels: output.channel,
            interleaved,
        },
        input.channel,
    );
    request.share = output.share;
    // Include a latency margin, then select the loudest burst-sized window;
    // leading capture/output skew must not dilute the RMS of a short burst.
    request.tail_seconds = 1.0;
    let input_channel = usize::from(input.channel);
    let recording = play_and_record(backend, request, cancel, &mut |_| {})?;
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
    let samples: Vec<_> = recording
        .interleaved
        .chunks_exact(input_channel)
        .map(|f| f64::from(f[input_channel - 1]))
        .collect();
    let n = signal.len();
    let mut energy = samples[..n].iter().map(|x| x * x).sum::<f64>();
    let mut best = (energy, 0);
    for i in n..samples.len() {
        energy += samples[i] * samples[i] - samples[i - n] * samples[i - n];
        if energy > best.0 {
            best = (energy, i + 1 - n);
        }
    }
    let settled = &samples[best.1..best.1 + n];
    let window = usize::try_from(output.sample_rate / 10)
        .map_err(|_| invalid("level window exceeds usize"))?
        .max(1);
    for samples in settled.chunks(window) {
        if samples.len() == window {
            on_level(analyse_level(
                samples,
                output.sample_rate,
                input.calibration.as_ref(),
                input.spl_reference,
            )?);
        }
    }
    analyse_level(
        settled,
        output.sample_rate,
        input.calibration.as_ref(),
        input.spl_reference,
    )
}
