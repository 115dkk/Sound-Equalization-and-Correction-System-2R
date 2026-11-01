//! Port of SECS 2 `calibration.compute_mimo` and `export_camilladsp.mimo_paths`
//! (GPL-3.0). Export filters and the displayed response share one run value.

use secs_compute::Context;
use secs_dsp::response::lr4;
use secs_types::{
    calibration::CalibrationSettings,
    chain::{MimoFilter, MimoPath, MimoPaths},
};

use super::{IirDerivatives, IirSolveOptions, IirSupport, MimoDesign, MimoRequest, support_path};
use crate::{
    C, Design, EngineConfig, EngineError, MeasurementSet, Note, PeqDerivatives, Tuning,
    grid::design_freqs, scorecard::Metrics,
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
pub struct MimoRoleScore {
    pub role: String,
    pub after_mimo: Metrics,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MimoRoleLateEnergy {
    pub role: String,
    pub position: u32,
    pub after_mimo_db: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MimoRun {
    pub key: MimoKey,
    pub ideal: MimoDesign,
    pub paths: MimoPaths,
    pub final_resp: Vec<(String, Vec<C>)>,
    /// Main-position responses consumed by the `보정 후 + MIMO` mdat items.
    pub applied: Vec<(String, Vec<C>)>,
    /// Scorecard metrics computed from those exact main-position responses,
    /// with the common input cut divided back out (ADR 0031).
    pub scorecard: Vec<MimoRoleScore>,
    /// Late energy after MIMO, position-major and in main-chain order.
    pub late_energy: Vec<MimoRoleLateEnergy>,
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
    secs_dsp::fir::lowrate_response(taps, fs_low, frequencies)
}

/// Band-limited interpolation to 48 kHz, cropped to the low-rate FIR's duration.
/// Invalid or oversized low-rate recipes produce an empty filter.
pub fn fir_to_48k(taps_low: &[f64], fs_low: f64) -> Vec<f64> {
    secs_dsp::fir::lowrate_to_48k(taps_low, fs_low)
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
    let total: usize = run.paths.paths.iter().map(|p| p.filter.fir_len_48k()).sum();
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
    let iir = IirSolveOptions {
        derivatives: if settings.mimo_iir_analytic_derivatives {
            IirDerivatives::Analytic
        } else {
            IirDerivatives::CentralDifference
        },
        seed_peq_derivatives: if settings.peq_analytic_derivatives {
            PeqDerivatives::Analytic
        } else {
            PeqDerivatives::TwoPoint
        },
        ..IirSolveOptions::default()
    };
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
    let prepared = super::prepared::Prepared::new(design, measurements, config, ctx)?;
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
        super::iir::design_mimo_iir_prepared_with_options(
            design, &prepared, &ideal, &req, tuning, &iir, ctx,
        )?
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

/// `response` without the common input cut, for the scorecard: ADR 0031 keeps
/// every verdict as it is without the cut, and the cut multiplies every path
/// alike, so dividing it back out leaves the response the verdict scores
/// without it. Late energy and the SPL curves keep the cut. The cut is zero
/// only at DC, where every path is zero too, so that bin stays as it is.
pub(crate) fn without_input_cut<'a>(
    response: &'a [C],
    cut: Option<&[C]>,
) -> std::borrow::Cow<'a, [C]> {
    match cut {
        None => std::borrow::Cow::Borrowed(response),
        Some(cut) => response
            .iter()
            .zip(cut)
            .map(|(r, h)| if h.norm_sqr() == 0.0 { *r } else { r / h })
            .collect(),
    }
}

struct ResponseAt<'a> {
    role: &'a str,
    position: u32,
}

fn applied_response(
    design: &Design,
    prepared: &super::prepared::Prepared<'_>,
    ideal: &MimoDesign,
    supports: &Supports,
    lp: &[C],
    at: ResponseAt<'_>,
    ctx: &Context,
) -> Result<(Vec<C>, Vec<C>), EngineError> {
    check(ctx)?;
    let f = design_freqs();
    let base = crate::stage_responses(design, at.role, &prepared.spectra, at.position)
        .ok_or_else(|| {
            EngineError::Input(format!(
                "missing MIMO final response {}/{}",
                at.position, at.role
            ))
        })?
        .output;
    let mut extra = vec![C::new(0.0, 0.0); f.len()];
    if let Some((_, sups)) = supports.iter().find(|(input, _)| input == at.role) {
        for (support, realization) in sups {
            check(ctx)?;
            let path = support_path(
                ideal,
                support,
                &prepared.spectra,
                at.position,
                &prepared.align_delay_ms,
            )?;
            let response = match realization {
                Support::Iir(support) => support.response(f),
                Support::Fir { taps, fs_low } => fir_response(taps, *fs_low, f),
            };
            for index in 0..f.len() {
                extra[index] += path[index] * lp[index] * response[index];
            }
        }
    }
    let applied = base
        .iter()
        .zip(extra)
        .map(|(base, extra)| base + extra)
        .collect();
    Ok((base, applied))
}

pub(crate) fn assemble_prepared(
    design: &Design,
    prepared: &super::prepared::Prepared<'_>,
    ideal: &MimoDesign,
    supports: Supports,
    key: MimoKey,
    ctx: &Context,
) -> Result<MimoRun, EngineError> {
    check(ctx)?;
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
                Support::Fir { taps, fs_low } => MimoFilter::FirLowRate {
                    taps_low: taps.clone(),
                    fs_low: *fs_low,
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
        let total: usize = paths.iter().map(|p| p.filter.fir_len_48k()).sum();
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
    let target = design.target.sample(
        &f.iter()
            .map(|frequency| frequency.max(1.0))
            .collect::<Vec<_>>(),
    );
    let input_cut = design
        .chain
        .input_hp_hz
        .map(|hz| secs_dsp::response::band_filter(f, hz, "HP", "BW48"))
        .transpose()?;
    let mut final_resp = Vec::new();
    let mut applied = Vec::new();
    let mut scorecard = Vec::new();
    let mut late_energy = Vec::new();
    let mut spl_mimo = Vec::new();
    for &position in &design.spectra.positions {
        for main in &design.chain.mains {
            let role = &main.role;
            let (base, sum) = applied_response(
                design,
                prepared,
                &ideal,
                &supports,
                &lp,
                ResponseAt { role, position },
                ctx,
            )?;
            late_energy.push(MimoRoleLateEnergy {
                role: role.clone(),
                position,
                after_mimo_db: crate::decay::late_db_20_150(&sum),
            });
            if position == design.spectra.main_position {
                scorecard.push(MimoRoleScore {
                    role: role.clone(),
                    after_mimo: crate::scorecard::metrics(
                        &without_input_cut(&sum, input_cut.as_deref()),
                        f,
                        &target,
                        (100.0, 400.0),
                    ),
                });
                spl_mimo.push((role.clone(), crate::design::spl_db(&sum, &spl_grid)));
                final_resp.push((role.clone(), base));
                applied.push((role.clone(), sum));
            }
        }
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
        scorecard,
        late_energy,
        spl_mimo,
        fir: key.fir,
        key,
    })
}
