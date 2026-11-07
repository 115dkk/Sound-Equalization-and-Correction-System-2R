//! One immutable design, ported from `CorrectionEngine.compute` and
//! `calibration.compute`. Spectra stay on the internal axis; exports use the
//! shifted output delays in `SignalChain`.

use std::sync::Arc;

use secs_compute::{Context, StageProgress};
use secs_dsp::{
    grid::geomspace,
    interp, response,
    smooth::{self, Fraction},
};
use secs_types::{
    calibration::RcHiBasis,
    chain::{BandType, MainChannel, MimoPaths, Peq, SignalChain, SubChannel},
    target::TargetCurve,
};

use crate::{
    C, EngineConfig, EngineError, MeasurementSet, Tuning,
    ba::{self, BaInput, LayoutInfo, ManualPlan},
    chain::BaRow,
    fit::{self, Fitter, PeqDerivatives},
    grid::{N_FFT, SR, design_freqs},
    input::RoleMap,
    levels,
    measurement::{self, Spectra},
    notes::{Band, Note, Peq as NotePeq, RoleBand, RoleNote, RoleValue, SubFilterNote},
    rc::{self, RcInput},
    scorecard::{self, Metrics},
    setup::Setup,
};

#[derive(Debug)]
pub enum FitStage<'a> {
    Ba,
    Top(#[cfg_attr(not(test), allow(dead_code))] &'a str),
    Rc(#[cfg_attr(not(test), allow(dead_code))] &'a str),
}
pub type FitterFactory<'f> = dyn Fn(FitStage<'_>) -> Box<Fitter<'f>> + Sync + 'f;
#[cfg(any(test, feature = "test-support"))]
pub fn production_fitters(ctx: &Context) -> Box<FitterFactory<'_>> {
    production_fitters_with(ctx, PeqDerivatives::TwoPoint)
}

pub fn production_fitters_with(
    ctx: &Context,
    derivatives: PeqDerivatives,
) -> Box<FitterFactory<'_>> {
    Box::new(move |_| {
        Box::new(move |request| {
            if ctx.cancel().is_cancelled() {
                Vec::new()
            } else {
                fit::fit_peq_with(request, ctx, derivatives)
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
    pub(crate) replay: Arc<crate::replay::DesignReplay>,
    pub chain: SignalChain,
    pub(crate) mains: Vec<MainDesign>,
    pub(crate) subs: Option<SubDesign>,
    pub ref_level: f64,
    pub rc_hi: f64,
    /// Merged correction bands; empty when no extra bands were requested.
    pub rc_bands: Vec<(f64, f64)>,
    pub rc_auto: bool,
    pub schroeder_hz: Option<f64>,
    pub target_db: Arc<[f64]>,
    pub target_display: Vec<f64>,
    pub(crate) spectra: DesignSpectra,
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

impl Design {
    pub fn config(&self) -> &EngineConfig {
        self.replay.config()
    }

    pub fn measurements(&self) -> &MeasurementSet {
        self.replay.measurements()
    }

    pub fn main_position(&self) -> u32 {
        self.config().main_position
    }

    pub fn latency_ms(&self) -> f64 {
        self.chain.latency_ms
    }

    pub fn rc_hi(&self) -> f64 {
        self.rc_hi
    }

    pub fn export_view(&self) -> Result<crate::ExportView, EngineError> {
        self.replay.export_view(self)
    }

    pub fn signal_chain(&self) -> &SignalChain {
        &self.chain
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn signal_chain_mut(&mut self) -> &mut SignalChain {
        &mut self.chain
    }

    pub fn main_channels(&self) -> impl Iterator<Item = &MainChannel> {
        self.chain.mains.iter()
    }

    pub fn sub_channels(&self) -> impl Iterator<Item = &SubChannel> {
        self.chain.subs.iter()
    }

    pub fn positions(&self) -> impl Iterator<Item = u32> + '_ {
        self.spectra.positions.iter().copied()
    }

    pub fn has_sub_design(&self) -> bool {
        self.subs.is_some()
    }

    pub fn layouts(&self) -> impl Iterator<Item = (&str, &LayoutInfo)> {
        self.subs.iter().flat_map(|subs| {
            subs.layout_info
                .iter()
                .map(|(name, info)| (name.as_str(), info))
        })
    }

    pub fn measured(&self, position: u32, role: &str) -> Option<&crate::Measurement> {
        self.replay.measurements().get(position, role)
    }

    pub fn main_roles(&self) -> impl Iterator<Item = &str> {
        self.main_channels().map(|main| main.role.as_str())
    }

    pub fn sub_roles(&self) -> impl Iterator<Item = &str> {
        self.sub_channels().map(|sub| sub.role.as_str())
    }

    pub fn fir(&self, role: &str) -> Option<&[f64]> {
        self.main_channels()
            .find(|main| main.role == role)
            .map(|main| main.fir.as_slice())
    }

    pub fn stage(&self, role: &str, position: u32) -> Result<crate::StageResponses, EngineError> {
        let view = self.export_view()?;
        crate::chain::stage_responses(self, role, &view.spectra, position)
            .ok_or_else(|| EngineError::Input(format!("missing stage response {position}/{role}")))
    }

    pub fn export_ir(&self, spectrum: &[C], axis: crate::ExportAxis) -> (Vec<f32>, f64) {
        let (samples, t0) = self.replay.export_ir(self, spectrum, axis);
        (
            samples.into_iter().map(|sample| sample as f32).collect(),
            t0,
        )
    }

    pub fn mimo_responses(&self, paths: &MimoPaths) -> Result<Vec<(String, Vec<C>)>, EngineError> {
        self.replay.mimo_responses(self, paths)
    }
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
    #[cfg_attr(not(test), allow(dead_code))]
    pub phase_frac_used: Option<f64>,
    pub(crate) before: Arc<[C]>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) combined: Arc<[C]>,
    pub(crate) after: Arc<[C]>,
}
#[derive(Debug)]
pub struct SubDesign {
    pub rows: Vec<(String, BaRow)>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub bus_gain: f64,
    #[cfg_attr(not(test), allow(dead_code))]
    pub bus_delay_ms: f64,
    #[cfg_attr(not(test), allow(dead_code))]
    pub bus_polarity: i8,
    pub bus_response: Option<Arc<[C]>>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub sum_at_reference: Arc<[C]>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub sub_level_db: Option<f64>,
    pub layout_info: Vec<(String, LayoutInfo)>,
    #[cfg_attr(not(test), allow(dead_code))]
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
            let (before_db, after_db) = if position == design.spectra.main_position
                && design.chain.input_hp_hz.is_none()
            {
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

/// The input cut's frequency stays inside the settings limits even when a
/// caller builds the config without the settings edit (ADR 0031): 0 would
/// make every export response non-finite.
pub(crate) fn check_input_hp(config: &EngineConfig) -> Result<(), EngineError> {
    let (lo, hi) = secs_types::rules::CALIBRATION_LIMITS.input_hp;
    match config.input_hp_hz {
        Some(hz) if !(hz.is_finite() && (lo..=hi).contains(&hz)) => Err(EngineError::Input(
            format!("input high-pass {hz} Hz outside {lo}..={hi} Hz"),
        )),
        _ => Ok(()),
    }
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
/// What `lf_align` and `mode_decay` (ADR 0025) keep beside that: the total
/// phase vector, the cut response, β and `τ_T` in the preparation, and the
/// smoothed total phase, its GD and the total-GD error in
/// `finish_correction`. About 90 bytes per bin, rounded up.
const RC_ADR0025_SPECTRA: u64 = 6;

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
                let extra = if setup.lf_align || setup.mode_decay {
                    RC_ADR0025_SPECTRA
                } else {
                    0
                };
                spectra(positions + 4 + RC_SCRATCH_SPECTRA + extra)
            },
        }
    }
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
    ctx.cancel().check()?;
    check_taps(req.config)?;
    check_input_hp(req.config)?;
    let prep_started = std::time::Instant::now();
    let config = req.config;
    let setup = Setup::new(config);
    // Reserve before `measurement::prepare` allocates (TD-24): its spectra
    // and the design's retained arrays are held to the end; RC channels and
    // their scores reserve `bytes.channel` each, in batches, out of the rest.
    let bytes = DesignBytes::new(req.measurements, config, &setup, req.subs_only);
    let held =
        measurement::preparation_bytes(req.measurements, &setup).saturating_add(bytes.retained);
    let schroeder = levels::Schroeder::analyze(req.measurements, config, ctx)?;
    let _permit = crate::memory::reserve(ctx, held, bytes.channel)?;
    let prepared = measurement::prepare_until(req.measurements, config, &setup, ctx)?;
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
        schroeder,
    )
}

/// The caller must prepare these measurements with this config's non-XO fields.
/// Recommendation varies only XO: preparation reads the reference position,
/// main/sub membership and fixed sub filters, never XO or BA-derived filters.
/// Candidates borrow the spectra; all candidate-dependent work stays below.
/// Cloning Spectra at the output boundary copies Arc handles, not sample buffers.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(test), allow(dead_code))]
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
    let schroeder = levels::Schroeder::analyze(req.measurements, req.config, ctx)?;
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
        schroeder,
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
    schroeder: levels::Schroeder,
) -> Result<Design, EngineError> {
    design_prepared_scored(
        req,
        ctx,
        fitters,
        prepared,
        &|_, _| {},
        false,
        ChannelBytes::RESERVED,
        schroeder,
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

#[allow(clippy::too_many_arguments)]
fn design_prepared_scored(
    req: &DesignRequest<'_>,
    ctx: &Context,
    fitters: &FitterFactory<'_>,
    prepared: &measurement::Prepared,
    observed: &(dyn Fn(&str, std::time::Duration) + Sync),
    include_scores: bool,
    reserve: ChannelBytes,
    schroeder: levels::Schroeder,
) -> Result<Design, EngineError> {
    if req.config.rc_range.1.is_none() && req.config.rc_hi_basis == RcHiBasis::Agreement {
        let count = agreement_positions(req.measurements, req.config);
        if count < 3 {
            return Err(EngineError::Input(format!(
                "RC agreement requires at least 3 measured positions; found {count}"
            )));
        }
        let (probe, limits) = design_pass(PassContext {
            req,
            ctx,
            fitters,
            prepared,
            observed,
            include_scores: false,
            reserve,
            schroeder,
            pass: RcPass::Probe,
        })?;
        let limit = levels::median(limits).clamp(100.0, 2000.0);
        drop(probe);
        ctx.cancel().check()?;
        return design_pass(PassContext {
            req,
            ctx,
            fitters,
            prepared,
            observed,
            include_scores,
            reserve,
            schroeder,
            pass: RcPass::Final(limit),
        })
        .map(|(design, _)| design);
    }
    design_pass(PassContext {
        req,
        ctx,
        fitters,
        prepared,
        observed,
        include_scores,
        reserve,
        schroeder,
        pass: RcPass::Normal,
    })
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

struct PassContext<'a, 'req, 'fit> {
    req: &'a DesignRequest<'req>,
    ctx: &'a Context,
    fitters: &'a FitterFactory<'fit>,
    prepared: &'a measurement::Prepared,
    observed: &'a (dyn Fn(&str, std::time::Duration) + Sync),
    include_scores: bool,
    reserve: ChannelBytes,
    schroeder: levels::Schroeder,
    pass: RcPass,
}

struct LevelStage {
    positions: Vec<u32>,
    rc_auto: bool,
    schroeder_hz: Option<f64>,
    rc_bands: Vec<(f64, f64)>,
    xo_of: Vec<(String, f64)>,
    weights: Vec<f64>,
    levels: Vec<(String, f64)>,
    ref_level: f64,
}

struct BaStage {
    alignment: ba::SubAlignment,
    used: Spectra,
    top_peq: Vec<(String, Vec<Peq>)>,
    top_responses: Vec<(String, Arc<[C]>)>,
    bus_delay_ms: f64,
    bus_polarity: i8,
    rows: Vec<BaRow>,
    shift: f64,
    #[cfg(test)]
    ba_elapsed: std::time::Duration,
    #[cfg(test)]
    top_elapsed: std::time::Duration,
}

struct RcStage {
    channels: Vec<MainChannel>,
    mains: Vec<MainDesign>,
    agreement_limits: Vec<f64>,
    #[cfg(test)]
    timings: Vec<(String, std::time::Duration)>,
}

struct SubStage {
    channels: Vec<SubChannel>,
    design: Option<SubDesign>,
    bus_peq: Vec<Peq>,
    used: Spectra,
    shift: f64,
    latency_ms: f64,
}

struct ScoreStage {
    scores: Vec<RoleScores>,
    target_db: Arc<[f64]>,
    spl_grid: Vec<f64>,
    #[cfg(test)]
    elapsed: std::time::Duration,
}

fn polarity_stage(
    pass: &PassContext<'_, '_, '_>,
    setup: &Setup,
    run_final_checks: bool,
    notes: &mut Vec<Note>,
) {
    let config = pass.req.config;
    if run_final_checks {
        notes.extend(crate::polarity::check(
            pass.req.measurements,
            &config.mains,
            config.main_position,
        ));
    }
    if let Some(from_ms) = setup.lift_capped_from_ms {
        notes.push(Note::LiftCapped {
            from_ms,
            taps: config.taps,
            lift_ms: setup.lift_ms,
        });
    }
    if !setup.sub_filters.is_empty() {
        notes.push(Note::SubFilters {
            filters: setup
                .sub_filters
                .iter()
                .map(|(role, filter)| RoleNote {
                    role: role.clone(),
                    value: SubFilterNote {
                        hp_hz: filter.hp_hz,
                        hp_type: filter.hp_type,
                        lp_hz: filter.lp_hz,
                        lp_type: filter.lp_type,
                        peq: filter
                            .peq
                            .iter()
                            .map(|&(hz, q, gain_db)| NotePeq { hz, q, gain_db })
                            .collect(),
                    },
                })
                .collect(),
        });
    }
}

fn levels_stage(
    pass: &PassContext<'_, '_, '_>,
    setup: &mut Setup,
    positions: Vec<u32>,
    notes: &mut Vec<Note>,
) -> Result<LevelStage, EngineError> {
    let config = pass.req.config;
    let rc_auto = setup.rc_hi_manual.is_none();
    let schroeder_hz = pass.schroeder.strict;
    let lenient = pass.schroeder.lenient;
    if rc_auto {
        if !matches!(config.rc_hi_transition, 1 | 2 | 4) {
            return Err(EngineError::Input(
                "RC transition factor must be 1, 2 or 4".into(),
            ));
        }
        setup.rc_hi = match pass.pass {
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
            .then(|| agreement_positions(pass.req.measurements, config)),
    });
    let rc_bands = if setup.rc_bands.is_empty() {
        Vec::new()
    } else {
        let bands = rc::band_set(setup.rc_lo, setup.rc_hi, &setup.rc_bands);
        notes.push(Note::RcBands {
            bands: bands
                .iter()
                .map(|&(lo_hz, hi_hz)| Band { lo_hz, hi_hz })
                .collect(),
        });
        bands
    };
    let xo_of: Vec<_> = config
        .groups
        .iter()
        .filter(|(group, _)| group != "sub")
        .flat_map(|(group, roles)| {
            roles.iter().filter_map(move |role| {
                (!config.subs.is_empty())
                    .then(|| RoleMap::get_from(&config.xo, group))
                    .flatten()
                    .copied()
                    .filter(|xo| *xo != 0.0)
                    .map(|xo| (role.clone(), xo))
            })
        })
        .collect();
    let weights = levels::weights(&positions, config.main_position);
    let channel_levels: Vec<_> = config
        .mains
        .iter()
        .filter(|role| pass.prepared.h.get(config.main_position, role).is_some())
        .map(|role| {
            (
                role.clone(),
                levels::band_level(
                    &pass.prepared.h,
                    role,
                    &weights,
                    &config.target,
                    300.0,
                    3000.0,
                ),
            )
        })
        .collect();
    let ref_level = channel_levels
        .iter()
        .map(|(_, value)| *value)
        .reduce(f64::min)
        .ok_or_else(|| EngineError::Input("no measured main".into()))?;
    Ok(LevelStage {
        positions,
        rc_auto,
        schroeder_hz,
        rc_bands,
        xo_of,
        weights,
        levels: channel_levels,
        ref_level,
    })
}

fn ba_stage(
    pass: &PassContext<'_, '_, '_>,
    setup: &Setup,
    level: &LevelStage,
    progress: &StageProgress<'_>,
    notes: &mut Vec<Note>,
) -> Result<BaStage, EngineError> {
    let config = pass.req.config;
    let input = BaInput {
        measurements: pass.req.measurements,
        prepared: pass.prepared,
        setup,
        config,
        tuning: pass.req.tuning,
        xo_of: &level.xo_of,
        ref_level: level.ref_level,
        cancel: pass.ctx.cancel(),
    };
    let ba_started = std::time::Instant::now();
    let mut ba_fitter = (pass.fitters)(FitStage::Ba);
    let alignment = ba::align(&input, &mut *ba_fitter)?;
    pass.ctx.cancel().check()?;
    #[cfg(test)]
    let ba_timing = ba_started.elapsed();
    notes.extend(alignment.notes.clone());
    progress.advance(1);
    let ba_elapsed = ba_started.elapsed();
    let mut used = pass.prepared.h.clone();
    for (position, role, response) in &alignment.h_subs {
        used.set(*position, role, response.clone());
    }
    let top_started = std::time::Instant::now();
    let mut top_peq = Vec::new();
    let mut top_responses = Vec::new();
    if !config.subs.is_empty() && pass.req.tuning.xo_stage {
        for (role, xo) in &level.xo_of {
            pass.ctx.cancel().check()?;
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
            let channel_level = *RoleMap::get_from(&level.levels, role).ok_or_else(|| {
                EngineError::Input(format!("XO role {role} missing at reference"))
            })?;
            let gain = C::new(10f64.powf((level.ref_level - channel_level) / 20.0), 0.0);
            let by_pos: Vec<_> = level
                .positions
                .iter()
                .zip(&level.weights)
                .filter_map(|(&position, &weight)| {
                    used.get(position, role).map(|response| {
                        (
                            weight,
                            response
                                .iter()
                                .map(|value| value * gain)
                                .collect::<Vec<_>>(),
                        )
                    })
                })
                .collect();
            let weighted: Vec<_> = by_pos
                .iter()
                .map(|(weight, response)| (*weight, response.as_slice()))
                .collect();
            let peq = fit::xo_target_peq(
                &weighted,
                &config.target,
                level.ref_level,
                lo,
                hi,
                6,
                config.max_boost_db.min(6.0),
                Some(hi),
                &mut *(pass.fitters)(FitStage::Top(role)),
            );
            pass.ctx.cancel().check()?;
            let response: Arc<[C]> = response::peq_response(design_freqs(), &peq).into();
            for &position in &level.positions {
                if let Some(measured) = used.get(position, role) {
                    used.set(
                        position,
                        role,
                        measured
                            .iter()
                            .zip(response.iter())
                            .map(|(measured, correction)| measured * correction)
                            .collect(),
                    );
                }
            }
            top_responses.push((role.clone(), response));
            top_peq.push((role.clone(), peq));
        }
    }
    #[cfg(test)]
    let top_timing = top_started.elapsed();
    (pass.observed)("top", top_started.elapsed());
    let bus_started = std::time::Instant::now();
    let (bus_delay_ms, bus_polarity) = if config.subs.is_empty() {
        (0.0, 1)
    } else {
        let earliest = alignment
            .rows
            .iter()
            .map(|row| row.delay_ms)
            .fold(f64::INFINITY, f64::min);
        let lo = (-(setup.max_latency_ms - setup.lift_ms) - earliest) * 4.0;
        let mains: Vec<_> = config
            .mains
            .iter()
            .map(|role| {
                (
                    role.clone(),
                    level
                        .positions
                        .iter()
                        .filter(|position| {
                            alignment
                                .sub_sum_by_pos
                                .iter()
                                .any(|(candidate, _)| candidate == *position)
                        })
                        .filter_map(|&position| {
                            used.get(position, role)
                                .map(|response| (position, response.clone()))
                        })
                        .collect(),
                )
            })
            .collect();
        ba::bus_delay(
            &alignment,
            &mains,
            &pass.prepared.align_delay_ms,
            &level.xo_of,
            lo.ceil() / 4.0,
            pass.ctx,
        )?
    };
    pass.ctx.cancel().check()?;
    let rows = ba::fold(
        &alignment.rows,
        alignment.bus_gain,
        bus_delay_ms,
        bus_polarity,
    );
    (pass.observed)("ba", ba_elapsed + bus_started.elapsed());
    let shift = if alignment
        .plan
        .as_ref()
        .is_some_and(|plan| !plan.fixed.is_empty())
    {
        config.sub_main_delay_ms
    } else {
        (-rows
            .iter()
            .map(|row| row.delay_ms)
            .reduce(f64::min)
            .unwrap_or(0.0))
        .max(0.0)
    };
    Ok(BaStage {
        alignment,
        used,
        top_peq,
        top_responses,
        bus_delay_ms,
        bus_polarity,
        rows,
        shift,
        #[cfg(test)]
        ba_elapsed: ba_timing,
        #[cfg(test)]
        top_elapsed: top_timing,
    })
}

fn rc_stage(
    pass: &PassContext<'_, '_, '_>,
    setup: &Setup,
    level: &LevelStage,
    ba: &BaStage,
    progress: &StageProgress<'_>,
    notes: &mut Vec<Note>,
) -> Result<RcStage, EngineError> {
    let config = pass.req.config;
    let roles = if pass.req.subs_only && !matches!(pass.pass, RcPass::Probe) {
        vec![]
    } else {
        config.mains.iter().collect()
    };
    let rc_all_started = std::time::Instant::now();
    // Batches of channels reserve on this thread before entering the pool,
    // never inside a pool task (TD-23); outputs keep the channel order.
    let outputs = crate::memory::try_map(
        pass.ctx,
        roles,
        pass.reserve.held,
        |_| pass.reserve.channel,
        |role| {
            pass.ctx.cancel().check()?;
            let Some(before) = pass.prepared.h0.get(config.main_position, role) else {
                progress.advance(1);
                return Ok(None);
            };
            let rc_started = std::time::Instant::now();
            let gain_db = level.ref_level
                - RoleMap::get_from(&level.levels, role).expect("role present: reference level");
            let xo_hz = RoleMap::get_from(&level.xo_of, role).copied();
            let align_delay_ms = *RoleMap::get_from(&pass.prepared.align_delay_ms, role)
                .expect("role present: alignment delay");
            let comb: Vec<(u32, Arc<[C]>)> = level
                .positions
                .iter()
                .filter_map(|&position| {
                    ba.used.get(position, role).map(|response| {
                        let sub = ba
                            .alignment
                            .sub_sum_by_pos
                            .iter()
                            .find(|(candidate, _)| *candidate == position)
                            .map(|(_, response)| response.as_ref());
                        (
                            position,
                            rc::combine(
                                response,
                                align_delay_ms,
                                gain_db,
                                xo_hz,
                                sub,
                                ba.bus_polarity,
                                ba.bus_delay_ms,
                            )
                            .into(),
                        )
                    })
                })
                .collect();
            let mut fitter = (pass.fitters)(FitStage::Rc(role));
            let rc_input = RcInput {
                role,
                comb: &comb,
                main_position: config.main_position,
                level_db: level.ref_level,
                setup,
                config,
                tuning: pass.req.tuning,
                positions: &level.positions,
                bus_rolloff_hz: if config.low_protect && xo_hz.is_some_and(|hz| hz != 0.0) {
                    ba.alignment.bus_rolloff_hz
                } else {
                    None
                },
            };
            let result = rc::channel(&rc_input, &mut *fitter, pass.ctx)?;
            pass.ctx.cancel().check()?;
            let agreement = matches!(pass.pass, RcPass::Probe)
                .then(|| rc::agreement_limit(&rc_input, &result.response));
            let combined = comb
                .iter()
                .find(|(position, _)| *position == config.main_position)
                .unwrap()
                .1
                .clone();
            let response: Arc<[C]> = result.response.into();
            let after = combined
                .iter()
                .zip(response.iter())
                .map(|(measured, correction)| measured * correction)
                .collect();
            let mut peq = result.out.peq;
            peq.extend(result.gd_cuts);
            let channel = MainChannel {
                role: role.clone(),
                input_peq: peq,
                fir: result.out.fir,
                gain_db,
                polarity: config.inverted_mains.contains(role).then_some(-1),
                delay_ms: align_delay_ms + ba.shift,
                xo_hz,
                align_delay_ms,
                top_peq: RoleMap::get_from(&ba.top_peq, role).map_or_else(Vec::new, Clone::clone),
            };
            let main = MainDesign {
                role: role.clone(),
                gain_db,
                xo_hz,
                align_delay_ms,
                top_response: RoleMap::get_from(&ba.top_responses, role).cloned(),
                response,
                phase_strength: result.out.phase_strength,
                phase_band: result.out.phase_band,
                phase_frac_used: result.phase_frac_used,
                before: before.clone(),
                combined,
                after,
            };
            (pass.observed)(&format!("rc:{role}"), rc_started.elapsed());
            progress.advance(1);
            #[cfg(test)]
            let elapsed = rc_started.elapsed();
            #[cfg(not(test))]
            let elapsed = ();
            Ok(Some((channel, main, result.out.notes, elapsed, agreement)))
        },
    )?;
    (pass.observed)("rc_all", rc_all_started.elapsed());
    let mut channels = Vec::new();
    let mut mains = Vec::new();
    let mut agreement_limits = Vec::new();
    #[cfg(test)]
    let mut timings = Vec::new();
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
    Ok(RcStage {
        channels,
        mains,
        agreement_limits,
        #[cfg(test)]
        timings,
    })
}

fn sub_stage(
    pass: &PassContext<'_, '_, '_>,
    setup: &Setup,
    level: &LevelStage,
    ba: BaStage,
    notes: &mut Vec<Note>,
) -> SubStage {
    let config = pass.req.config;
    let BaStage {
        alignment,
        used,
        bus_delay_ms,
        bus_polarity,
        rows,
        shift,
        ..
    } = ba;
    let latency_ms = setup.lift_ms + shift;
    notes.push(Note::Latency {
        latency_ms,
        lift_ms: setup.lift_ms,
        main_extra_ms: shift,
    });
    let mut channels = Vec::new();
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
            mode: ba::sub_mode_label(role, setup, &alignment.layout_info),
        };
        if alignment
            .plan
            .as_ref()
            .is_some_and(|plan| plan.fixed.contains(role))
        {
            let manual = RoleMap::get_from(&setup.sub_manual, role)
                .expect("role present: fixed sub settings");
            sub.gain_db = manual.gain_db;
            sub.delay_ms = manual.delay_ms;
            sub.polarity = if manual.polarity < 0 { -1 } else { 1 };
            sub.allpass = manual.allpass.iter().copied().take(2).collect();
            sub.mode = "fixed".into();
        }
        if let Some(filter) = RoleMap::get_from(&alignment.sub_filters, role) {
            sub.hp_hz = filter.hp_hz;
            sub.hp_type = filter.hp_type;
            sub.lp_hz = filter.lp_hz;
            sub.lp_type = filter.lp_type;
            sub.peq = filter.peq.clone();
        }
        if config.low_protect {
            let by_pos: Vec<_> = level
                .positions
                .iter()
                .filter_map(|&position| {
                    used.get(position, role)
                        .map(|response| (position, response.clone()))
                })
                .collect();
            if let Some(hz) = crate::low_protect::rolloff(
                &by_pos,
                config.main_position,
                Some((sub.hp_hz, sub.lp_hz)),
            ) {
                if sub.hp_hz == 0.0 {
                    sub.hp_hz = hz / 2.0_f64.sqrt();
                    sub.hp_type = BandType::Lr24;
                }
                notes.push(Note::LowProtect {
                    role: role.clone(),
                    rolloff_hz: hz,
                    hp_hz: Some(sub.hp_hz),
                    limited: true,
                    reduced: 0,
                    cuts: 0,
                });
            }
        }
        channels.push(sub);
    }
    let design = if config.subs.is_empty() {
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
                .find(|(position, _)| *position == config.main_position)
                .unwrap()
                .1
                .clone(),
            sub_level_db: alignment.sub_level_db,
            layout_info: alignment.layout_info,
            plan: alignment.plan,
        })
    };
    SubStage {
        channels,
        design,
        bus_peq: alignment.bus_peq,
        used,
        shift,
        latency_ms,
    }
}

fn score_stage(
    pass: &PassContext<'_, '_, '_>,
    setup: &Setup,
    level: &LevelStage,
    mains: &[MainDesign],
    notes: &mut Vec<Note>,
) -> Result<ScoreStage, EngineError> {
    let f = design_freqs();
    let target = pass.req.config.target.sample(
        &f.iter()
            .map(|frequency| frequency.max(1.0))
            .collect::<Vec<_>>(),
    );
    let target_db: Arc<[f64]> = target.iter().map(|value| value + level.ref_level).collect();
    let spl_grid = if pass.include_scores {
        geomspace(5.0, 20000.0, 1390)
    } else {
        Vec::new()
    };
    let gd_grid = if pass.include_scores {
        geomspace(5.0, 5000.0, 1070)
    } else {
        Vec::new()
    };
    let scores_started = std::time::Instant::now();
    let scores = if pass.include_scores {
        // A channel's scores need less than its RC working set.
        crate::memory::try_map(
            pass.ctx,
            mains.iter().collect(),
            pass.reserve.held,
            |_| pass.reserve.channel,
            |main| {
                pass.ctx.cancel().check()?;
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
    (pass.observed)("scores", scores_started.elapsed());
    #[cfg(test)]
    let elapsed = scores_started.elapsed();
    notes.push(Note::PhaseStrength {
        values: mains
            .iter()
            .map(|main| RoleValue {
                role: main.role.clone(),
                value: main.phase_strength,
            })
            .collect(),
    });
    notes.push(Note::PhaseBand {
        values: mains
            .iter()
            .map(|main| RoleBand {
                role: main.role.clone(),
                hz: main.phase_band,
            })
            .collect(),
    });
    if let Some(hz) = setup.input_hp_hz {
        notes.push(Note::InputHp { hz });
    }
    pass.ctx.cancel().check()?;
    Ok(ScoreStage {
        scores,
        target_db,
        spl_grid,
        #[cfg(test)]
        elapsed,
    })
}

fn pair_phase_stage(
    pass: &PassContext<'_, '_, '_>,
    run_final_checks: bool,
    design: &mut Design,
) -> Result<(), EngineError> {
    if !run_final_checks {
        return Ok(());
    }
    let pair_started = std::time::Instant::now();
    let f = design_freqs();
    // Only the inspection range's bins are kept, a few hundred per main,
    // so the check stays far below the reserved design budget (TD-24).
    let (lo, hi) = crate::pair_phase::BAND_HZ;
    let kept: Vec<usize> = f
        .iter()
        .enumerate()
        .filter(|&(_, &hz)| hz >= lo && hz <= hi)
        .map(|(index, _)| index)
        .collect();
    let band_f: Vec<f64> = kept.iter().map(|&index| f[index]).collect();
    let mut responses = Vec::with_capacity(design.mains.len());
    for main in &design.mains {
        pass.ctx.cancel().check()?;
        let Some(top) = crate::chain::stage_top(
            design,
            &main.role,
            &pass.prepared.h,
            pass.req.config.main_position,
            &kept,
        ) else {
            continue;
        };
        let response: Vec<C> = top
            .into_iter()
            .zip(&kept)
            .map(|(top, &index)| top * main.response[index])
            .collect();
        responses.push((main.role.clone(), response));
    }
    design
        .notes
        .extend(crate::pair_phase::check(&band_f, &responses));
    pass.ctx.cancel().check()?;
    (pass.observed)("pair_phase", pair_started.elapsed());
    Ok(())
}

fn design_pass(stage: PassContext<'_, '_, '_>) -> Result<(Design, Vec<f64>), EngineError> {
    let req = stage.req;
    let ctx = stage.ctx;
    let prepared = stage.prepared;
    let pass = stage.pass;
    ctx.cancel().check()?;
    check_taps(req.config)?;
    check_input_hp(req.config)?;
    let config = req.config;
    let mut setup = Setup::new(config);
    if matches!(pass, RcPass::Probe) {
        setup.rc_bands.clear();
        setup.lf_align = false;
        setup.mode_decay = false;
        setup.hf_phase = false;
    }
    let positions = prepared.h.positions().to_vec();
    let progress = ctx.stage("engine.design", 1 + config.mains.len() as u64);
    let mut notes = Vec::new();
    let run_final_checks = !req.subs_only && !matches!(pass, RcPass::Probe);
    polarity_stage(&stage, &setup, run_final_checks, &mut notes);
    let level = levels_stage(&stage, &mut setup, positions, &mut notes)?;
    let ba = ba_stage(&stage, &setup, &level, &progress, &mut notes)?;
    #[cfg(test)]
    let mut timings = vec![
        ("BA".into(), ba.ba_elapsed),
        ("top PEQ".into(), ba.top_elapsed),
    ];
    let rc = rc_stage(&stage, &setup, &level, &ba, &progress, &mut notes)?;
    #[cfg(test)]
    timings.extend(rc.timings.iter().cloned());
    let sub = sub_stage(&stage, &setup, &level, ba, &mut notes);
    let score = score_stage(&stage, &setup, &level, &rc.mains, &mut notes)?;
    #[cfg(test)]
    timings.push(("scores".into(), score.elapsed));
    let f = design_freqs();
    let mut design = Design {
        chain: SignalChain {
            input_hp_hz: setup.input_hp_hz,
            sample_rate: SR,
            mains: rc.channels,
            subs: sub.channels,
            sub_bus_peq: sub.bus_peq,
            main_extra_ms: sub.shift,
            lift_ms: setup.lift_ms,
            latency_ms: sub.latency_ms,
        },
        mains: rc.mains,
        subs: sub.design,
        ref_level: level.ref_level,
        rc_hi: setup.rc_hi,
        rc_bands: level.rc_bands,
        rc_auto: level.rc_auto,
        schroeder_hz: level.schroeder_hz,
        target_display: score
            .spl_grid
            .iter()
            .map(|&frequency| interp(frequency, f, &score.target_db))
            .collect(),
        target_db: score.target_db,
        spectra: DesignSpectra {
            raw: Arc::new(prepared.h0.clone()),
            used: Arc::new(sub.used),
            main_position: config.main_position,
            positions: level.positions,
        },
        notes,
        adapter_notes: req.adapter_notes.to_vec(),
        groups: config.groups.clone(),
        target: config.target.clone(),
        xo: config.xo.clone(),
        scores: score.scores,
        replay: Arc::new(crate::DesignReplay::new(
            req.measurements,
            config,
            setup,
            prepared,
        )),
        #[cfg(test)]
        timings,
    };
    pair_phase_stage(&stage, run_final_checks, &mut design)?;
    Ok((design, rc.agreement_limits))
}
