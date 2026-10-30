//! One immutable design, ported from `CorrectionEngine.compute` and
//! `calibration.compute`. Spectra stay on the internal axis; exports use the
//! shifted output delays in `SignalChain`.

use std::sync::Arc;

use secs_compute::{CancelToken, Context};
use secs_dsp::{
    interp, response,
    smooth::{self, Fraction},
};
use secs_types::{
    calibration::RcHiBasis,
    chain::{BandType, MainChannel, SignalChain, SubChannel},
    target::TargetCurve,
};

use crate::{
    C, EngineConfig, EngineError, MeasurementSet, Tuning,
    ba::{self, BaInput, LayoutInfo, ManualPlan},
    chain::BaRow,
    fit::{self, Fitter, PeqDerivatives},
    grid::{N_FFT, SR, design_freqs},
    levels,
    measurement::{self, Spectra},
    notes::Note,
    rc::{self, RcInput},
    scorecard::{self, Metrics},
    setup::Setup,
};

#[derive(Debug)]
pub enum FitStage<'a> {
    Ba,
    Top(&'a str),
    Rc(&'a str),
}
pub type FitterFactory<'f> = dyn Fn(FitStage<'_>) -> Box<Fitter<'f>> + Sync + 'f;
pub fn production_fitters(cancel: &CancelToken) -> Box<FitterFactory<'_>> {
    production_fitters_with(cancel, PeqDerivatives::TwoPoint)
}

pub fn production_fitters_with(
    cancel: &CancelToken,
    derivatives: PeqDerivatives,
) -> Box<FitterFactory<'_>> {
    Box::new(move |_| {
        Box::new(move |request| {
            if cancel.is_cancelled() {
                Vec::new()
            } else {
                fit::fit_peq_with(request, &|| cancel.is_cancelled(), derivatives)
            }
        })
    })
}

#[derive(Debug)]
pub struct DesignRequest<'a> {
    pub measurements: &'a MeasurementSet,
    pub config: &'a EngineConfig,
    pub tuning: &'a Tuning,
    pub subs_only: bool,
    pub adapter_notes: &'a [String],
}
#[derive(Debug)]
pub struct Design {
    pub replay: Arc<crate::DesignReplay>,
    pub chain: SignalChain,
    pub mains: Vec<MainDesign>,
    pub subs: Option<SubDesign>,
    pub ref_level: f64,
    pub rc_hi: f64,
    pub rc_auto: bool,
    pub schroeder_hz: Option<f64>,
    pub target_db: Arc<[f64]>,
    pub target_display: Vec<f64>,
    pub spectra: DesignSpectra,
    pub notes: Vec<Note>,
    pub adapter_notes: Vec<String>,
    pub scores: Vec<RoleScores>,
    /// The speaker groups the design was made for (`engine.groups`), in order.
    pub groups: Vec<(String, Vec<String>)>,
    /// The target curve the design was made for (`engine.target_db_fn`).
    pub target: TargetCurve,
    /// The crossover per group as configured (`engine.xo`).
    pub xo: Vec<(String, f64)>,
    #[cfg(test)]
    pub timings: Vec<(String, std::time::Duration)>,
}
#[derive(Debug)]
pub struct DesignSpectra {
    pub raw: Arc<Spectra>,
    pub used: Arc<Spectra>,
    pub main_position: u32,
    pub positions: Vec<u32>,
}
#[derive(Debug)]
pub struct MainDesign {
    pub role: String,
    pub gain_db: f64,
    pub xo_hz: Option<f64>,
    pub align_delay_ms: f64,
    pub top_response: Option<Arc<[C]>>,
    pub response: Arc<[C]>,
    pub phase_strength: f64,
    pub phase_band: Option<f64>,
    pub phase_frac_used: Option<f64>,
    pub before: Arc<[C]>,
    pub combined: Arc<[C]>,
    pub after: Arc<[C]>,
}
#[derive(Debug)]
pub struct SubDesign {
    pub rows: Vec<(String, BaRow)>,
    pub bus_gain: f64,
    pub bus_delay_ms: f64,
    pub bus_polarity: i8,
    pub bus_response: Option<Arc<[C]>>,
    pub sum_at_reference: Arc<[C]>,
    pub sub_level_db: Option<f64>,
    pub layout_info: Vec<(String, LayoutInfo)>,
    pub plan: Option<ManualPlan>,
}
#[derive(Debug)]
pub struct RoleScores {
    pub role: String,
    pub before: Metrics,
    pub after: Metrics,
    pub spl_before: Vec<f64>,
    pub spl_after: Vec<f64>,
    pub gd_before: Vec<f64>,
    pub gd_after: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoleLateEnergy {
    pub role: String,
    pub position: u32,
    pub before_db: f64,
    pub after_db: f64,
}

/// Late energy on the same per-position responses used by the scorecard at the
/// main position and by the mdat's before/after items elsewhere: the 2 s export
/// view, which MIMO also starts from. The fitting spectra (`spectra.used`) are
/// not used; their late tail differs from those items by up to about 1 dB.
/// Rows are position major, with main roles in chain order.
pub fn late_energy(design: &Design) -> Result<Vec<RoleLateEnergy>, EngineError> {
    let view = design.replay.export_view(design)?;
    let mut rows = Vec::new();
    for &position in &design.spectra.positions {
        for channel in &design.chain.mains {
            let (before_db, after_db) = if position == design.spectra.main_position {
                let Some(main) = design.mains.iter().find(|main| main.role == channel.role) else {
                    continue;
                };
                (
                    crate::decay::late_db_20_150(&main.before),
                    crate::decay::late_db_20_150(&main.after),
                )
            } else {
                let Some(before) = view.spectra.get(position, &channel.role) else {
                    continue;
                };
                let Some(after) =
                    crate::stage_responses(design, &channel.role, &view.spectra, position)
                        .map(|stages| stages.output)
                else {
                    continue;
                };
                (
                    crate::decay::late_db_20_150(before),
                    crate::decay::late_db_20_150(&after),
                )
            };
            rows.push(RoleLateEnergy {
                role: channel.role.clone(),
                position,
                before_db,
                after_db,
            });
        }
    }
    Ok(rows)
}

fn check(ctx: &Context) -> Result<(), EngineError> {
    ctx.cancel().check().map_err(|_| EngineError::Cancelled)
}

/// RC realizes each channel FIR on the `N_FFT`-point design grid and keeps
/// its first `taps` samples, so more taps than that cannot be designed
/// (TD-50); 0 selects the PEQ-only path. The modes use 0 and 2048–16384.
pub(crate) fn check_taps(config: &EngineConfig) -> Result<(), EngineError> {
    if config.taps > N_FFT {
        return Err(EngineError::Input(format!(
            "FIR taps {} outside 0..={N_FFT} (0 is PEQ only)",
            config.taps
        )));
    }
    Ok(())
}

/// Full-grid arrays one `rc::channel` keeps alive at once, in spectra (16
/// bytes per bin). `prepare_correction` holds `p`, `m`, `u`, `reference`, `mp`
/// and the minimum-phase buffers (`logm`, `cep`, FFT output) beside the f64
/// grids `power`, `total`, `frac`, `sm`, `sm3`, `target`, `corr`, `design_db`
/// and the masks; `finish_correction` holds that preparation and its clones
/// of `p`, `m`, `design_db` beside `phase`, `tau`, `excess`, `band`, the
/// realized `spec`/`h`/`response`, the residual grids and, for a second phase
/// candidate, the first `RcOutput`. About 350 bytes per bin, rounded up.
const RC_SCRATCH_SPECTRA: u64 = 24;

/// What one design reserves beyond the prepared spectra (TD-24). `retained`
/// is alive from BA to the end of the design; `channel` is one RC channel (or
/// one channel's scores, which need less) while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DesignBytes {
    pub retained: u64,
    pub channel: u64,
}

impl DesignBytes {
    /// Counted in spectra of `SPECTRUM_BYTES`, with P positions, S subs and M
    /// mains (none for a subs-only design):
    ///
    /// - retained: `used` cells replaced by BA's `alignment.h_subs` (each sub
    ///   cell once per layout) and the top PEQ (each main cell), BA's
    ///   `sub_sum_by_pos` and layout `by_pos` sums (2·P), `ba::sums`' gathered
    ///   cells, index and result at one position (S + 2), `top_responses` (M),
    ///   and per finished channel `MainDesign.response`, `after` and
    ///   `combined` (3·M). Without subs only the last term remains.
    /// - replay: owned input/config/Setup metadata and preparation handles;
    ///   IR samples and prepared spectrum buffers are shared, not copied.
    /// - channel: `comb` (P), `rc::combine`'s `delay`, `hp`, `lp` and bus
    ///   `delay` (4), and `rc::channel`'s scratch (`RC_SCRATCH_SPECTRA`).
    pub(crate) fn new(
        set: &MeasurementSet,
        config: &EngineConfig,
        setup: &Setup,
        subs_only: bool,
    ) -> Self {
        let positions = set.positions.len() as u64;
        let subs = config.subs.len() as u64;
        let mains = if subs_only
            && !(config.rc_range.1.is_none() && config.rc_hi_basis == RcHiBasis::Agreement)
        {
            0
        } else {
            config.mains.len() as u64
        };
        let ba = if config.subs.is_empty() {
            0
        } else {
            measurement::cells(set, Some(&config.subs))
                .saturating_mul(setup.layouts.len() as u64)
                .saturating_add(measurement::cells(set, Some(&config.mains)))
                .saturating_add(2 * positions)
                .saturating_add(subs + 2)
                .saturating_add(config.mains.len() as u64)
        };
        let spectra = |n: u64| n.saturating_mul(measurement::SPECTRUM_BYTES);
        Self {
            retained: spectra(ba.saturating_add(3 * mains))
                .saturating_add(crate::DesignReplay::bytes(set, config, setup)),
            channel: if mains == 0 {
                0
            } else {
                spectra(positions + 4 + RC_SCRATCH_SPECTRA)
            },
        }
    }
}
pub(crate) fn geomspace(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let start = lo.log10();
    let step = (hi.log10() - start) / (n - 1) as f64;
    let mut out: Vec<_> = (0..n)
        .map(|i| 10f64.powf(start + i as f64 * step))
        .collect();
    out[0] = lo;
    out[n - 1] = hi;
    out
}
pub(crate) fn spl_db(h: &[C], grid: &[f64]) -> Vec<f64> {
    let p: Vec<_> = h
        .iter()
        .map(|v| {
            let a = v.re.hypot(v.im);
            a * a
        })
        .collect();
    let db: Vec<_> = smooth::smooth_power(design_freqs(), &p, Fraction::Scalar(24.0))
        .expect("design grid")
        .iter()
        .map(|p| 10.0 * (p + 1e-30).log10())
        .collect();
    grid.iter()
        .map(|&g| interp(g, design_freqs(), &db))
        .collect()
}

pub fn design(
    req: &DesignRequest<'_>,
    ctx: &Context,
    fitters: &FitterFactory<'_>,
) -> Result<Design, EngineError> {
    design_observed(req, ctx, fitters, &|_, _| {})
}

/// Stage wall times for the release audit example; not part of the public API.
pub(crate) fn design_observed(
    req: &DesignRequest<'_>,
    ctx: &Context,
    fitters: &FitterFactory<'_>,
    observed: &(dyn Fn(&str, std::time::Duration) + Sync),
) -> Result<Design, EngineError> {
    check(ctx)?;
    check_taps(req.config)?;
    let prep_started = std::time::Instant::now();
    let config = req.config;
    let setup = Setup::new(config);
    // Reserve before `measurement::prepare` allocates (TD-24): its spectra
    // and the design's retained arrays are held to the end; RC channels and
    // their scores reserve `bytes.channel` each, in batches, out of the rest.
    let bytes = DesignBytes::new(req.measurements, config, &setup, req.subs_only);
    let held =
        measurement::preparation_bytes(req.measurements, &setup).saturating_add(bytes.retained);
    let _permit = crate::memory::reserve(ctx, held, bytes.channel)?;
    let prepared = measurement::prepare_until(req.measurements, config, &setup, &|| {
        ctx.cancel().is_cancelled()
    })?;
    observed("prepare", prep_started.elapsed());
    design_prepared_scored(
        req,
        ctx,
        fitters,
        &prepared,
        observed,
        true,
        ChannelBytes {
            held,
            channel: bytes.channel,
        },
    )
}

/// The caller must prepare these measurements with this config's non-XO fields.
/// Recommendation varies only XO: preparation reads the reference position,
/// main/sub membership and fixed sub filters, never XO or BA-derived filters.
/// Candidates borrow the spectra; all candidate-dependent work stays below.
/// Cloning Spectra at the output boundary copies Arc handles, not sample buffers.
pub(crate) fn design_prepared(
    req: &DesignRequest<'_>,
    ctx: &Context,
    fitters: &FitterFactory<'_>,
    prepared: &measurement::Prepared,
    observed: &(dyn Fn(&str, std::time::Duration) + Sync),
) -> Result<Design, EngineError> {
    let bytes = DesignBytes::new(
        req.measurements,
        req.config,
        &Setup::new(req.config),
        req.subs_only,
    );
    let _permit = crate::memory::reserve(ctx, bytes.retained, bytes.channel)?;
    design_prepared_scored(
        req,
        ctx,
        fitters,
        prepared,
        observed,
        true,
        ChannelBytes {
            held: bytes.retained,
            channel: bytes.channel,
        },
    )
}

/// Candidate-only design: the recommendation consumes the chain and spectra,
/// not the final report plots. A normal design always includes those reports.
/// The caller reserves the whole design (`DesignBytes` with one channel) for
/// this serial context, so the channel maps below reserve nothing more.
pub(crate) fn design_prepared_for_xo(
    req: &DesignRequest<'_>,
    ctx: &Context,
    fitters: &FitterFactory<'_>,
    prepared: &measurement::Prepared,
) -> Result<Design, EngineError> {
    design_prepared_scored(
        req,
        ctx,
        fitters,
        prepared,
        &|_, _| {},
        false,
        ChannelBytes::RESERVED,
    )
}

/// How the per-channel maps reserve: `held` bytes are already reserved by
/// this job on the calling thread, and each channel needs `channel` more.
#[derive(Debug, Clone, Copy)]
struct ChannelBytes {
    held: u64,
    channel: u64,
}

impl ChannelBytes {
    /// The caller reserved every channel already.
    const RESERVED: Self = Self {
        held: 0,
        channel: 0,
    };
}

fn design_prepared_scored(
    req: &DesignRequest<'_>,
    ctx: &Context,
    fitters: &FitterFactory<'_>,
    prepared: &measurement::Prepared,
    observed: &(dyn Fn(&str, std::time::Duration) + Sync),
    include_scores: bool,
    reserve: ChannelBytes,
) -> Result<Design, EngineError> {
    if req.config.rc_range.1.is_none() && req.config.rc_hi_basis == RcHiBasis::Agreement {
        let count = agreement_positions(req.measurements, req.config);
        if count < 3 {
            return Err(EngineError::Input(format!(
                "RC agreement requires at least 3 measured positions; found {count}"
            )));
        }
        let (probe, limits) = design_pass(
            req,
            ctx,
            fitters,
            prepared,
            observed,
            false,
            reserve,
            RcPass::Probe,
        )?;
        let limit = levels::median(limits).clamp(100.0, 2000.0);
        drop(probe);
        check(ctx)?;
        return design_pass(
            req,
            ctx,
            fitters,
            prepared,
            observed,
            include_scores,
            reserve,
            RcPass::Final(limit),
        )
        .map(|(design, _)| design);
    }
    design_pass(
        req,
        ctx,
        fitters,
        prepared,
        observed,
        include_scores,
        reserve,
        RcPass::Normal,
    )
    .map(|(design, _)| design)
}

pub(crate) fn agreement_positions(set: &MeasurementSet, config: &EngineConfig) -> usize {
    set.positions
        .iter()
        .filter(|(_, roles)| {
            roles
                .iter()
                .any(|(role, m)| config.mains.contains(role) && !m.samples.is_empty())
        })
        .count()
}

#[derive(Clone, Copy)]
enum RcPass {
    Normal,
    Probe,
    Final(f64),
}

#[allow(clippy::too_many_arguments)]
fn design_pass(
    req: &DesignRequest<'_>,
    ctx: &Context,
    fitters: &FitterFactory<'_>,
    prepared: &measurement::Prepared,
    observed: &(dyn Fn(&str, std::time::Duration) + Sync),
    include_scores: bool,
    reserve: ChannelBytes,
    pass: RcPass,
) -> Result<(Design, Vec<f64>), EngineError> {
    check(ctx)?;
    check_taps(req.config)?;
    let config = req.config;
    let mut setup = Setup::new(config);
    let positions = prepared.h.positions().to_vec();
    let progress = ctx.stage("engine.design", 1 + config.mains.len() as u64);
    let mut notes = Vec::new();
    if let Some(from_ms) = setup.lift_capped_from_ms {
        notes.push(Note::LiftCapped {
            from_ms,
            taps: config.taps,
            lift_ms: setup.lift_ms,
        });
    }
    if !setup.sub_filters.is_empty() {
        notes.push(Note::SubFilters(setup.sub_filters.clone()));
    }
    let rc_auto = setup.rc_hi_manual.is_none();
    let schroeder_hz = rc_auto
        .then(|| levels::schroeder_hz(req.measurements, config))
        .flatten();
    let lenient = (rc_auto && config.rc_hi_basis == RcHiBasis::Lenient)
        .then(|| levels::lenient_schroeder_hz(req.measurements, config))
        .flatten();
    if rc_auto {
        if !matches!(config.rc_hi_transition, 1 | 2 | 4) {
            return Err(EngineError::Input(
                "RC transition factor must be 1, 2 or 4".into(),
            ));
        }
        setup.rc_hi = match pass {
            RcPass::Probe => 2000.0,
            RcPass::Final(hz) => hz,
            RcPass::Normal => {
                levels::schroeder_limit(schroeder_hz, lenient.map(|v| v.0), config.rc_hi_transition)
            }
        };
    }
    notes.push(Note::RcLimit {
        hz: setup.rc_hi,
        auto: rc_auto,
        schroeder_hz,
        basis: config.rc_hi_basis,
        factor: config.rc_hi_transition,
        band_hz: lenient.map(|v| v.0),
        band_t60_s: lenient.map(|v| v.1),
        positions: (rc_auto && config.rc_hi_basis == RcHiBasis::Agreement)
            .then(|| agreement_positions(req.measurements, config)),
    });
    let xo_of: Vec<_> = config
        .groups
        .iter()
        .filter(|(g, _)| g != "sub")
        .flat_map(|(g, roles)| {
            roles.iter().filter_map(move |r| {
                config
                    .xo
                    .iter()
                    .find(|(name, x)| name == g && *x != 0.0 && !config.subs.is_empty())
                    .map(|(_, x)| (r.clone(), *x))
            })
        })
        .collect();
    let weights = levels::weights(&positions, config.main_position);
    let levels: Vec<_> = config
        .mains
        .iter()
        .filter(|r| prepared.h.get(config.main_position, r).is_some())
        .map(|r| {
            (
                r.clone(),
                levels::band_level(&prepared.h, r, &weights, &config.target, 300.0, 3000.0),
            )
        })
        .collect();
    let ref_level = levels
        .iter()
        .map(|(_, v)| *v)
        .reduce(f64::min)
        .ok_or_else(|| EngineError::Input("no measured main".into()))?;
    let input = BaInput {
        measurements: req.measurements,
        prepared,
        setup: &setup,
        config,
        tuning: req.tuning,
        xo_of: &xo_of,
        ref_level,
        cancel: ctx.cancel(),
    };
    let ba_started = std::time::Instant::now();
    let mut ba_fitter = fitters(FitStage::Ba);
    let alignment = ba::align(&input, &mut *ba_fitter)?;
    check(ctx)?;
    #[cfg(test)]
    let mut timings = vec![("BA".into(), ba_started.elapsed())];
    notes.extend(alignment.notes.clone());
    progress.advance(1);
    let ba_elapsed = ba_started.elapsed();
    let mut used = prepared.h.clone();
    for (p, r, h) in &alignment.h_subs {
        used.set(*p, r, h.clone());
    }
    let top_started = std::time::Instant::now();
    let mut top_peq = Vec::new();
    let mut top_responses = Vec::new();
    if !config.subs.is_empty() && req.tuning.xo_stage {
        for (role, xo) in &xo_of {
            check(ctx)?;
            let lo = if config.xo_peq_lo_hz == 0.0 {
                xo / 2.0
            } else {
                config.xo_peq_lo_hz
            };
            let hi = if config.xo_peq_hi_hz == 0.0 {
                xo * 4.0
            } else {
                config.xo_peq_hi_hz
            };
            if !config.xo_peq_on || hi <= lo {
                continue;
            }
            let level = levels
                .iter()
                .find(|(r, _)| r == role)
                .ok_or_else(|| EngineError::Input(format!("XO role {role} missing at reference")))?
                .1;
            let gain = C::new(10f64.powf((ref_level - level) / 20.0), 0.0);
            let by_pos: Vec<_> = positions
                .iter()
                .zip(&weights)
                .filter_map(|(&p, &w)| {
                    used.get(p, role)
                        .map(|h| (w, h.iter().map(|v| v * gain).collect::<Vec<_>>()))
                })
                .collect();
            let weighted: Vec<_> = by_pos.iter().map(|(w, h)| (*w, h.as_slice())).collect();
            let peq = fit::xo_target_peq(
                &weighted,
                &config.target,
                ref_level,
                lo,
                hi,
                6,
                config.max_boost_db.min(6.0),
                Some(hi),
                &mut *fitters(FitStage::Top(role)),
            );
            check(ctx)?;
            let response: Arc<[C]> = response::peq_response(design_freqs(), &peq).into();
            for &p in &positions {
                if let Some(h) = used.get(p, role) {
                    used.set(
                        p,
                        role,
                        h.iter().zip(response.iter()).map(|(h, p)| h * p).collect(),
                    );
                }
            }
            top_responses.push((role.clone(), response));
            top_peq.push((role.clone(), peq));
        }
    }
    #[cfg(test)]
    timings.push(("top PEQ".into(), top_started.elapsed()));
    observed("top", top_started.elapsed());
    let bus_started = std::time::Instant::now();
    let (bus_delay_ms, bus_polarity) = if config.subs.is_empty() {
        (0.0, 1)
    } else {
        let earliest = alignment
            .rows
            .iter()
            .map(|r| r.delay_ms)
            .fold(f64::INFINITY, f64::min);
        let lo = (-(setup.max_latency_ms - setup.lift_ms) - earliest) * 4.0;
        let mains: Vec<_> = config
            .mains
            .iter()
            .map(|r| {
                (
                    r.clone(),
                    positions
                        .iter()
                        .filter(|p| alignment.sub_sum_by_pos.iter().any(|(q, _)| q == *p))
                        .filter_map(|&p| used.get(p, r).map(|h| (p, h.clone())))
                        .collect(),
                )
            })
            .collect();
        ba::bus_delay(
            &alignment,
            &mains,
            &prepared.align_delay_ms,
            &xo_of,
            lo.ceil() / 4.0,
            &|| ctx.cancel().is_cancelled(),
        )?
    };
    check(ctx)?;
    let rows = ba::fold(
        &alignment.rows,
        alignment.bus_gain,
        bus_delay_ms,
        bus_polarity,
    );
    observed("ba", ba_elapsed + bus_started.elapsed());
    let shift = if alignment.plan.as_ref().is_some_and(|p| !p.fixed.is_empty()) {
        config.sub_main_delay_ms
    } else {
        (-rows
            .iter()
            .map(|r| r.delay_ms)
            .reduce(f64::min)
            .unwrap_or(0.0))
        .max(0.0)
    };
    let roles = if req.subs_only && !matches!(pass, RcPass::Probe) {
        vec![]
    } else {
        config.mains.iter().collect()
    };
    let rc_all_started = std::time::Instant::now();
    // Batches of channels reserve on this thread before entering the pool,
    // never inside a pool task (TD-23); outputs keep the channel order.
    let outputs = crate::memory::try_map(
        ctx,
        roles,
        reserve.held,
        |_| reserve.channel,
        |role| {
            check(ctx)?;
            let Some(before) = prepared.h0.get(config.main_position, role) else {
                progress.advance(1);
                return Ok(None);
            };
            let rc_started = std::time::Instant::now();
            let gain_db = ref_level - levels.iter().find(|(r, _)| r == role).unwrap().1;
            let xo_hz = xo_of.iter().find(|(r, _)| r == role).map(|(_, x)| *x);
            let align_delay_ms = prepared
                .align_delay_ms
                .iter()
                .find(|(r, _)| r == role)
                .unwrap()
                .1;
            let comb: Vec<(u32, Arc<[C]>)> = positions
                .iter()
                .filter_map(|&p| {
                    used.get(p, role).map(|h| {
                        let sub = alignment
                            .sub_sum_by_pos
                            .iter()
                            .find(|(q, _)| *q == p)
                            .map(|(_, h)| h.as_ref());
                        (
                            p,
                            rc::combine(
                                h,
                                align_delay_ms,
                                gain_db,
                                xo_hz,
                                sub,
                                bus_polarity,
                                bus_delay_ms,
                            )
                            .into(),
                        )
                    })
                })
                .collect();
            let mut fitter = fitters(FitStage::Rc(role));
            let rc_input = RcInput {
                role,
                comb: &comb,
                main_position: config.main_position,
                level_db: ref_level,
                setup: &setup,
                config,
                tuning: req.tuning,
                positions: &positions,
            };
            let result = rc::channel(&rc_input, &mut *fitter, &|| ctx.cancel().is_cancelled())?;
            check(ctx)?;
            let agreement = matches!(pass, RcPass::Probe)
                .then(|| rc::agreement_limit(&rc_input, &result.response));
            let combined = comb
                .iter()
                .find(|(p, _)| *p == config.main_position)
                .unwrap()
                .1
                .clone();
            let response: Arc<[C]> = result.response.into();
            let after = combined
                .iter()
                .zip(response.iter())
                .map(|(h, f)| h * f)
                .collect();
            let mut peq = result.out.peq;
            peq.extend(result.gd_cuts);
            let channel = MainChannel {
                role: role.clone(),
                input_peq: peq,
                fir: result.out.fir,
                gain_db,
                delay_ms: align_delay_ms + shift,
                xo_hz,
                align_delay_ms,
                top_peq: top_peq
                    .iter()
                    .find(|(r, _)| r == role)
                    .map_or_else(Vec::new, |(_, p)| p.clone()),
            };
            let main = MainDesign {
                role: role.clone(),
                gain_db,
                xo_hz,
                align_delay_ms,
                top_response: top_responses
                    .iter()
                    .find(|(r, _)| r == role)
                    .map(|(_, p)| p.clone()),
                response,
                phase_strength: result.out.phase_strength,
                phase_band: result.out.phase_band,
                phase_frac_used: result.phase_frac_used,
                before: before.clone(),
                combined,
                after,
            };
            observed(&format!("rc:{role}"), rc_started.elapsed());
            progress.advance(1);
            #[cfg(test)]
            let elapsed = rc_started.elapsed();
            #[cfg(not(test))]
            let elapsed = ();
            Ok(Some((channel, main, result.out.notes, elapsed, agreement)))
        },
    )?;
    observed("rc_all", rc_all_started.elapsed());
    let mut channels = Vec::new();
    let mut mains = Vec::new();
    let mut agreement_limits = Vec::new();
    for (channel, main, channel_notes, _elapsed, agreement) in outputs.into_iter().flatten() {
        if let Some(limit) = agreement {
            agreement_limits.push(limit);
        }
        #[cfg(test)]
        timings.push((format!("RC {}", main.role), _elapsed));
        channels.push(channel);
        mains.push(main);
        notes.extend(channel_notes);
    }
    let latency_ms = setup.lift_ms + shift;
    notes.push(Note::Latency {
        latency_ms,
        lift_ms: setup.lift_ms,
        main_extra_ms: shift,
    });
    let mut subs = Vec::new();
    for (role, row) in config.subs.iter().zip(&rows) {
        let mut sub = SubChannel {
            role: role.clone(),
            gain_db: row.gain_db,
            delay_ms: row.delay_ms + shift,
            polarity: row.polarity,
            allpass: row.allpass.clone(),
            hp_hz: 0.0,
            hp_type: BandType::Lr24,
            lp_hz: 0.0,
            lp_type: BandType::Lr24,
            peq: vec![],
            mode: ba::sub_mode_label(role, &setup, &alignment.layout_info),
        };
        if alignment
            .plan
            .as_ref()
            .is_some_and(|p| p.fixed.contains(role))
        {
            let m = &setup.sub_manual.iter().find(|(r, _)| r == role).unwrap().1;
            sub.gain_db = m.gain_db;
            sub.delay_ms = m.delay_ms;
            sub.polarity = if m.polarity < 0 { -1 } else { 1 };
            sub.allpass = m.allpass.iter().copied().take(2).collect();
            sub.mode = "fixed".into();
        }
        if let Some((_, f)) = alignment.sub_filters.iter().find(|(r, _)| r == role) {
            sub.hp_hz = f.hp_hz;
            sub.hp_type = f.hp_type;
            sub.lp_hz = f.lp_hz;
            sub.lp_type = f.lp_type;
            sub.peq = f.peq.clone();
        }
        subs.push(sub);
    }
    let sub = if config.subs.is_empty() {
        None
    } else {
        Some(SubDesign {
            rows: config.subs.iter().cloned().zip(rows).collect(),
            bus_gain: alignment.bus_gain,
            bus_delay_ms,
            bus_polarity,
            bus_response: (!alignment.bus_peq.is_empty())
                .then(|| response::peq_response(design_freqs(), &alignment.bus_peq).into()),
            sum_at_reference: alignment
                .sub_sum_by_pos
                .iter()
                .find(|(p, _)| *p == config.main_position)
                .unwrap()
                .1
                .clone(),
            sub_level_db: alignment.sub_level_db,
            layout_info: alignment.layout_info,
            plan: alignment.plan,
        })
    };
    let f = design_freqs();
    let target = config
        .target
        .sample(&f.iter().map(|f| f.max(1.0)).collect::<Vec<_>>());
    let target_db: Arc<[f64]> = target.iter().map(|v| v + ref_level).collect();
    let spl_grid = if include_scores {
        geomspace(5.0, 20000.0, 1390)
    } else {
        Vec::new()
    };
    let gd_grid = if include_scores {
        geomspace(5.0, 5000.0, 1070)
    } else {
        Vec::new()
    };
    let scores_started = std::time::Instant::now();
    let scores = if include_scores {
        // A channel's scores need less than its RC working set.
        crate::memory::try_map(
            ctx,
            mains.iter().collect(),
            reserve.held,
            |_| reserve.channel,
            |main| {
                check(ctx)?;
                Ok(RoleScores {
                    role: main.role.clone(),
                    before: scorecard::metrics(&main.before, f, &target, (100.0, 400.0)),
                    after: scorecard::metrics(&main.after, f, &target, (100.0, 400.0)),
                    spl_before: spl_db(&main.before, &spl_grid),
                    spl_after: spl_db(&main.after, &spl_grid),
                    gd_before: response::gd_curve(
                        &main.before,
                        f,
                        &gd_grid,
                        Fraction::Scalar(24.0),
                    )?,
                    gd_after: response::gd_curve(&main.after, f, &gd_grid, Fraction::Scalar(24.0))?,
                })
            },
        )?
    } else {
        Vec::new()
    };
    observed("scores", scores_started.elapsed());
    #[cfg(test)]
    timings.push(("scores".into(), scores_started.elapsed()));
    notes.push(Note::PhaseStrength(
        mains
            .iter()
            .map(|m| (m.role.clone(), m.phase_strength))
            .collect(),
    ));
    notes.push(Note::PhaseBand(
        mains
            .iter()
            .map(|m| (m.role.clone(), m.phase_band))
            .collect(),
    ));
    check(ctx)?;
    Ok((
        Design {
            chain: SignalChain {
                sample_rate: SR,
                mains: channels,
                subs,
                sub_bus_peq: alignment.bus_peq,
                main_extra_ms: shift,
                lift_ms: setup.lift_ms,
                latency_ms,
            },
            mains,
            subs: sub,
            ref_level,
            rc_hi: setup.rc_hi,
            rc_auto,
            schroeder_hz,
            target_display: spl_grid.iter().map(|&g| interp(g, f, &target_db)).collect(),
            target_db,
            spectra: DesignSpectra {
                raw: Arc::new(prepared.h0.clone()),
                used: Arc::new(used),
                main_position: config.main_position,
                positions,
            },
            notes,
            adapter_notes: req.adapter_notes.to_vec(),
            groups: config.groups.clone(),
            target: config.target.clone(),
            xo: config.xo.clone(),
            scores,
            replay: Arc::new(crate::DesignReplay::new(
                req.measurements,
                config,
                setup,
                prepared,
            )),
            #[cfg(test)]
            timings,
        },
        agreement_limits,
    ))
}
