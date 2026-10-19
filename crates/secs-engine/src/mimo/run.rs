//! Port of SECS 2 `calibration.compute_mimo` and `export_camilladsp.mimo_paths`
//! (GPL-3.0). Export filters and the displayed response share one run value.

use secs_compute::Context;
use secs_dsp::{fft, response::lr4};
use secs_types::{
    calibration::CalibrationSettings,
    chain::{MimoFilter, MimoPath, MimoPaths},
};

use super::{IirSupport, MimoDesign, MimoRequest, support_path};
use crate::{
    C, Design, EngineConfig, EngineError, MeasurementSet, Note, Tuning, grid::design_freqs,
};

/// Low-rate taps a FIR support solves for (upstream `mimo.FIR_TAPS_LOW`):
/// 8,000 taps at 48 kHz are 100 at 600 Hz, and the 31-tap Fsiso low-pass
/// takes 30 of them.
pub(crate) const FIR_TAPS_LOW: usize = 70;

#[derive(Debug, Clone, PartialEq)]
pub struct MimoKey {
    pub fsiso: f64,
    pub f_low: Option<f64>,
    pub exclude: Vec<String>,
    pub max_supports: usize,
    pub fir: bool,
}

impl MimoKey {
    /// Normalize the typed settings using `CalibrationSettings.from_project`'s bounds.
    pub fn from_settings(settings: &CalibrationSettings) -> Self {
        let mut exclude: Vec<_> = settings
            .subs_manual
            .iter()
            .filter(|(_, s)| !s.mimo_support)
            .map(|(r, _)| r.clone())
            .collect();
        exclude.sort();
        Self {
            fsiso: if settings.fsiso.is_finite() {
                settings.fsiso.clamp(50.0, 150.0)
            } else {
                150.0
            },
            f_low: settings
                .mimo_low
                .filter(|f| f.is_finite())
                .map(|f| f.clamp(15.0, 150.0)),
            exclude,
            max_supports: settings.mimo_max_supports.clamp(0, 12) as usize,
            fir: settings.mimo_fir,
        }
    }

    pub(crate) fn request(&self) -> MimoRequest {
        MimoRequest {
            fsiso: self.fsiso,
            f_low: self.f_low,
            exclude: self.exclude.clone(),
            max_supports: (self.max_supports != 0).then_some(self.max_supports),
            taps_low: if self.fir {
                FIR_TAPS_LOW
            } else {
                MimoRequest::default().taps_low
            },
            ..MimoRequest::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MimoRun {
    pub key: MimoKey,
    pub ideal: MimoDesign,
    pub paths: MimoPaths,
    pub final_resp: Vec<(String, Vec<C>)>,
    pub applied: Vec<(String, Vec<C>)>,
    /// Upstream's 1390-point geometric SPL grid, 5–20,000 Hz, smoothed at 1/24 octave.
    pub spl_mimo: Vec<(String, Vec<f64>)>,
    pub notes: Vec<Note>,
    pub fir: bool,
    pub compute: secs_types::compute::ComputeReport,
}

#[derive(Debug, Clone)]
pub(crate) enum Support {
    Iir(IirSupport),
    Fir { taps: Vec<f64>, fs_low: f64 },
}
pub(crate) type Supports = Vec<(String, Vec<(String, Support)>)>;

pub(crate) fn fir_supports(ideal: &MimoDesign) -> Supports {
    ideal
        .supports
        .iter()
        .filter(|(_, s)| !s.is_empty())
        .map(|(r, supports)| {
            (
                r.clone(),
                supports
                    .iter()
                    .map(|(s, taps)| {
                        (
                            s.clone(),
                            Support::Fir {
                                taps: taps.clone(),
                                fs_low: ideal.fs_low,
                            },
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

/// Low-rate DTFT and upstream's linear 10% edge ramp (its comment says cosine).
pub(crate) fn fir_response(taps: &[f64], fs_low: f64, frequencies: &[f64]) -> Vec<C> {
    let edge = fs_low / 2.0;
    frequencies
        .iter()
        .map(|&f| {
            if f >= edge {
                return C::new(0.0, 0.0);
            }
            let mut sum = C::new(0.0, 0.0);
            for (k, &tap) in taps.iter().enumerate() {
                let phase = (-2.0 * std::f64::consts::PI * (f * k as f64)) / fs_low;
                sum += C::from_polar(1.0, phase) * tap;
            }
            sum * ((edge - f) / (0.1 * edge)).clamp(0.0, 1.0)
        })
        .collect()
}

/// Band-limited interpolation to 48 kHz, cropped to the low-rate FIR's duration.
/// Empty taps or a non-positive/non-finite sample rate produce an empty filter.
pub fn fir_to_48k(taps_low: &[f64], fs_low: f64) -> Vec<f64> {
    if taps_low.is_empty() || !fs_low.is_finite() || fs_low <= 0.0 {
        return Vec::new();
    }
    let n48 = (taps_low.len() as f64 * (48000.0 / fs_low)).ceil() as usize;
    let nfft = (n48 * 2).next_power_of_two();
    let f: Vec<_> = (0..=nfft / 2)
        .map(|k| k as f64 * (48000.0 / nfft as f64))
        .collect();
    let mut taps = fft::irfft(&fir_response(taps_low, fs_low, &f), nfft);
    taps.truncate(n48);
    taps
}

pub fn availability(positions: usize) -> (bool, String) {
    if positions < 3 {
        (
            false,
            format!("측정 위치 {positions}곳 — MIMO 는 3곳 이상 필요"),
        )
    } else {
        (true, String::new())
    }
}

pub fn kind_text(fir: bool) -> &'static str {
    if fir { "FIR" } else { "IIR" }
}

fn grouped(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i != 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn fir_budget_text(run: &MimoRun, budget: usize) -> String {
    if !run.fir || budget == 0 {
        return String::new();
    }
    let total: usize = run
        .paths
        .paths
        .iter()
        .map(|p| match &p.filter {
            MimoFilter::Fir { taps_48k } => taps_48k.len(),
            MimoFilter::Iir { .. } => 0,
        })
        .sum();
    format!(
        "FIR 서포트 총 {}탭 — DSP FIR 예산 {}탭 {}",
        grouped(total),
        grouped(budget),
        if total <= budget { "안" } else { "초과" }
    )
}

pub fn cap_text(max_supports: usize) -> String {
    if max_supports == 0 {
        String::new()
    } else {
        format!(" · 입력당 서포트 ≤ {max_supports}")
    }
}

/// Upstream's estimate with project input count and worker count made explicit.
pub fn estimate_seconds(inputs: usize, positions: usize, workers: usize, fir: bool) -> f64 {
    let rounds = if fir {
        0
    } else {
        inputs.max(1).div_ceil(workers.max(1))
    };
    (10.0_f64).max((40.0 + rounds as f64 * 355.0) * (positions.max(3) as f64 / 9.0))
}

fn check(ctx: &Context) -> Result<(), EngineError> {
    ctx.cancel().check().map_err(|_| EngineError::Cancelled)
}

pub fn compute(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    settings: &CalibrationSettings,
    tuning: &Tuning,
    ctx: &Context,
) -> Result<MimoRun, EngineError> {
    compute_observed(
        design,
        measurements,
        config,
        settings,
        tuning,
        ctx,
        &|_, _| {},
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_observed(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    settings: &CalibrationSettings,
    tuning: &Tuning,
    ctx: &Context,
    observe: &(dyn Fn(&str, std::time::Duration) + Sync),
) -> Result<MimoRun, EngineError> {
    compute_observed_accelerated(
        design,
        measurements,
        config,
        settings,
        tuning,
        ctx,
        observe,
        None,
    )
}

/// Application entry point with an explicitly selected optional accelerator.
#[allow(clippy::too_many_arguments)]
pub fn compute_with_acceleration(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    settings: &CalibrationSettings,
    tuning: &Tuning,
    ctx: &Context,
    acceleration: &super::AccelerationSelection,
) -> Result<MimoRun, EngineError> {
    if acceleration.status.preference == secs_types::compute::ComputePreference::Gpu
        && (acceleration.backend.is_none()
            || acceleration.status.selected != secs_types::compute::ComputeBackend::Cuda)
    {
        return Err(EngineError::Input("requested GPU is unavailable".into()));
    }
    compute_observed_accelerated(
        design,
        measurements,
        config,
        settings,
        tuning,
        ctx,
        &|_, _| {},
        Some(acceleration),
    )
}

#[allow(clippy::too_many_arguments)]
fn compute_observed_accelerated(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    settings: &CalibrationSettings,
    tuning: &Tuning,
    ctx: &Context,
    observe: &(dyn Fn(&str, std::time::Duration) + Sync),
    acceleration: Option<&super::AccelerationSelection>,
) -> Result<MimoRun, EngineError> {
    check(ctx)?;
    if !settings.mimo_on {
        return Err(EngineError::Input("MIMO is disabled".into()));
    }
    let (ok, reason) = availability(design.spectra.positions.len());
    if !ok {
        return Err(EngineError::Input(reason));
    }
    let key = MimoKey::from_settings(settings);
    // Capped IIR fitting amplifies even tiny changes in its ideal target.
    // Retain the exact reference target for IIR, including explicit GPU requests.
    let iir_reference = acceleration.filter(|_| !key.fir).map(|selection| {
        let mut selection = selection.clone();
        selection.backend = None;
        selection.status.reason = secs_types::compute::ComputeReason::IirReference;
        selection
    });
    let acceleration = iir_reference.as_ref().or(acceleration);
    let req = key.request();
    let progress = ctx.stage("engine.mimo", 2);
    let started = std::time::Instant::now();
    let prepared = super::prepared::Prepared::new(design, measurements, config)?;
    observe("preparation", started.elapsed());
    let started = std::time::Instant::now();
    let acceleration = acceleration.map(super::acceleration::RunAcceleration::new);
    let ideal = super::design::design_mimo_prepared_accelerated(
        design,
        &prepared,
        &req,
        ctx,
        None,
        observe,
        acceleration.as_ref(),
    )?
    .0;
    observe("design_wall", started.elapsed());
    progress.advance(1);
    check(ctx)?;
    let started = std::time::Instant::now();
    let supports = if key.fir {
        fir_supports(&ideal)
    } else if ideal.supports.is_empty() {
        Vec::new()
    } else {
        super::iir::design_mimo_iir_prepared(design, &prepared, &ideal, &req, tuning, ctx)?
            .supports
            .into_iter()
            .map(|(r, s)| {
                (
                    r,
                    s.into_iter().map(|(r, s)| (r, Support::Iir(s))).collect(),
                )
            })
            .collect()
    };
    observe("realization", started.elapsed());
    let started = std::time::Instant::now();
    let mut run = assemble_prepared(design, &prepared, &ideal, supports, key, ctx)?;
    if let Some(acceleration) = acceleration {
        run.compute = acceleration.report();
    }
    observe("assemble", started.elapsed());
    progress.advance(1);
    Ok(run)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn assemble(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    ideal: &MimoDesign,
    supports: Supports,
    key: MimoKey,
    ctx: &Context,
) -> Result<MimoRun, EngineError> {
    check(ctx)?;
    let prepared = super::prepared::Prepared::before(design, measurements, config)?;
    assemble_prepared(design, &prepared, ideal, supports, key, ctx)
}

pub(crate) fn assemble_prepared(
    design: &Design,
    prepared: &super::prepared::Prepared,
    ideal: &MimoDesign,
    supports: Supports,
    key: MimoKey,
    ctx: &Context,
) -> Result<MimoRun, EngineError> {
    check(ctx)?;
    let spectra = &prepared.spectra;
    let f = design_freqs();
    let mut paths = Vec::new();
    for (input, sups) in &supports {
        for (support, sp) in sups {
            check(ctx)?;
            let low_hz = ideal
                .ranges
                .iter()
                .find(|(r, _)| r == support)
                .and_then(|(_, r)| *r)
                .map_or(50.0, |r| r.0);
            let delay_ms = prepared
                .align_delay_ms
                .iter()
                .find(|(r, _)| r == support)
                .map_or(0.0, |(_, d)| *d)
                + design.chain.main_extra_ms;
            let filter = match sp {
                Support::Iir(s) => MimoFilter::Iir {
                    gain_db: s.gain_db,
                    delay_ms: s.delay_ms,
                    polarity: s.polarity,
                    peq: s.peq.clone(),
                    allpass: s.allpass.clone(),
                },
                Support::Fir { taps, fs_low } => MimoFilter::Fir {
                    taps_48k: fir_to_48k(taps, *fs_low),
                },
            };
            paths.push(MimoPath {
                input: input.clone(),
                support: support.clone(),
                filter,
                low_hz,
                delay_ms,
            });
        }
    }
    let mut ideal = ideal.clone();
    if key.max_supports != 0 {
        ideal
            .notes
            .push(Note::Mimo(crate::notes::MimoNote::SupportLimit {
                max_supports: key.max_supports,
            }));
    }
    if key.fir {
        let total: usize = paths
            .iter()
            .map(|p| match &p.filter {
                MimoFilter::Fir { taps_48k } => taps_48k.len(),
                _ => 0,
            })
            .sum();
        ideal
            .notes
            .push(Note::Mimo(crate::notes::MimoNote::FirBudget {
                support_count: paths.len(),
                taps_total: total,
            }));
    }
    let lp = if key.fir {
        vec![C::new(1.0, 0.0); f.len()]
    } else {
        lr4(f, key.fsiso, "LP")
    };
    let spl_grid = crate::design::geomspace(5.0, 20000.0, 1390);
    let mut final_resp = Vec::new();
    let mut applied = Vec::new();
    let mut spl_mimo = Vec::new();
    for main in &design.chain.mains {
        check(ctx)?;
        let role = &main.role;
        let base = crate::stage_responses(design, role, spectra, design.spectra.main_position)
            .ok_or_else(|| EngineError::Input(format!("missing MIMO final response {role}")))?
            .output;
        let mut extra = vec![C::new(0.0, 0.0); f.len()];
        if let Some((_, sups)) = supports.iter().find(|(r, _)| r == role) {
            for (support, sp) in sups {
                check(ctx)?;
                let path = support_path(
                    &ideal,
                    support,
                    spectra,
                    design.spectra.main_position,
                    &prepared.align_delay_ms,
                )?;
                let response = match sp {
                    Support::Iir(s) => s.response(f),
                    Support::Fir { taps, fs_low } => fir_response(taps, *fs_low, f),
                };
                for k in 0..f.len() {
                    extra[k] += path[k] * lp[k] * response[k];
                }
            }
        }
        let sum: Vec<_> = base.iter().zip(extra).map(|(b, e)| b + e).collect();
        let spl = crate::design::spl_db(&sum, &spl_grid);
        final_resp.push((role.clone(), base));
        applied.push((role.clone(), sum));
        spl_mimo.push((role.clone(), spl));
    }
    check(ctx)?;
    Ok(MimoRun {
        compute: secs_types::compute::ComputeReport::default(),
        notes: ideal.notes.clone(),
        ideal,
        paths: MimoPaths {
            fsiso: key.fsiso,
            paths,
        },
        final_resp,
        applied,
        spl_mimo,
        fir: key.fir,
        key,
    })
}
