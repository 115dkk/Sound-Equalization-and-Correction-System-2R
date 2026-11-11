//! Per-channel room correction, in upstream arithmetic and decision order.
//! Upstream: `_correction`, its masks, and `compute`'s per-main loop.

use crate::{
    C, EngineConfig, EngineError, Tuning, decay,
    fit::{self, FitRequest, Fitter},
    grid::{BINS, N_FFT, SR_F, design_freqs},
    input::{Peq, PhasePositions},
    levels, measurement,
    notes::Note,
    setup::Setup,
};
use secs_dsp::{
    complex_div as divide, fft,
    grid::geomspace,
    interp, lowrate,
    minphase::minimum_phase,
    pairwise_sum,
    response::{self, magnitude},
    smooth::{self, Fraction},
};
use std::{f64::consts::PI, sync::Arc};

pub struct RcInput<'a> {
    pub role: &'a str,
    pub comb: &'a [(u32, Arc<[C]>)],
    pub main_position: u32,
    pub level_db: f64,
    pub setup: &'a Setup,
    pub config: &'a EngineConfig,
    pub tuning: &'a Tuning,
    pub positions: &'a [u32],
    pub bus_rolloff_hz: Option<f64>,
}

fn protection_rolloff(input: &RcInput<'_>, channel_rolloff: Option<f64>) -> Option<f64> {
    if !input.config.low_protect {
        return None;
    }
    channel_rolloff
        .into_iter()
        .chain(input.bus_rolloff_hz)
        .reduce(f64::max)
}
#[derive(Debug, Clone)]
pub struct LfAlignOutcome {
    pub f_lin: f64,
    pub strength: f64,
    pub arrival_ms: Option<f64>,
    pub limited: bool,
}
#[derive(Debug, Clone)]
pub struct ModeDecayOutcome {
    pub target_t60_s: f64,
    pub modes: Vec<(f64, f64, f64)>,
}
#[derive(Debug, Clone)]
pub struct HfPhaseOutcome {
    pub rotation_deg: Option<f64>,
    pub limited: bool,
}
#[derive(Debug, Clone)]
pub struct RcOutput {
    pub fir: Vec<f64>,
    pub response: Vec<C>,
    pub peq: Vec<Peq>,
    pub phase_strength: f64,
    pub phase_band: Option<f64>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub design_db: Vec<f64>,
    pub notes: Vec<Note>,
    pub lf_align: Option<LfAlignOutcome>,
    pub mode_decay: Option<ModeDecayOutcome>,
    pub hf_phase: Option<HfPhaseOutcome>,
    dip_limit: Option<Vec<(f64, f64, f64)>>,
    protection: crate::low_protect::LimitOutcome,
    rolloff: Option<f64>,
}
pub struct ChannelRc {
    pub out: RcOutput,
    pub phase_frac_used: Option<f64>,
    pub gd_cuts: Vec<Peq>,
    pub response: Vec<C>,
}

// The trace observes the same implementation; it never supplies decisions.
#[derive(Default, Debug)]
pub(crate) struct Trace {
    pub no_boost: Vec<bool>,
    pub long_mode: Vec<bool>,
    pub peaks: Vec<(u32, usize)>,
    pub ladder: Vec<(f64, f64, f64)>,
}
fn main_response<'a>(input: &'a RcInput<'_>) -> &'a [C] {
    &input
        .comb
        .iter()
        .find(|(p, _)| *p == input.main_position)
        .expect("measured main position")
        .1
}
fn smooth(x: &[f64], fraction: f64) -> Vec<f64> {
    smooth::smooth_power(design_freqs(), x, Fraction::Scalar(fraction)).expect("design grid")
}
/// The magnitude estimate used both by RC fitting and the agreement probe.
fn smoothed_db(power: &[f64]) -> Vec<f64> {
    let f = design_freqs();
    let frac = secs_dsp::grid::frac_for(f);
    let sm = smooth::smooth_power(f, power, Fraction::PerBin(&frac)).expect("design grid");
    let sm3 = smooth(power, 3.0);
    sm.iter()
        .zip(sm3)
        .map(|(sm, sm3)| (10.0 * (sm + 1e-30).log10()).max(10.0 * (sm3 + 1e-30).log10() - 3.0))
        .collect()
}

/// The lowest target correction, upstream's `-25` cut clamp; the dip cap
/// (ADR 0034) never lowers a cut below it.
pub(crate) const CUT_FLOOR_DB: f64 = -25.0;

pub(crate) fn limit_dip_fills(
    power: &[f64],
    sm: &[f64],
    target: &[f64],
    level_db: f64,
    corr: &mut [f64],
) -> Vec<(f64, f64)> {
    let env: Vec<_> = smooth(power, 1.0)
        .into_iter()
        .map(|value| 10.0 * (value + 1e-30).log10())
        .collect();
    (0..BINS)
        .map(|i| {
            let depth = (env[i] - sm[i]).max(0.0);
            let fill = if depth <= 2.0 {
                depth
            } else {
                2.0 + (depth - 2.0) / 2.0
            }
            .min(6.0);
            corr[i] = corr[i].min((target[i] + level_db - env[i] + fill).max(CUT_FLOOR_DB));
            (depth, fill)
        })
        .collect()
}

/// The dips the cap really limited (ADR 0034): bins at least 4 dB deep whose
/// final design target is 0.5 dB or more below what the other limits alone
/// would have left. `bins` is `limit_dip_fills`' per-bin (depth, fill);
/// `limited` and `unlimited` are the design target after every other limit,
/// with and without the cap. Contiguous bins form one dip, reported at its
/// deepest bin as (Hz, depth, fill); deepest first, six at most.
pub(crate) fn dip_notes(
    bins: &[(f64, f64)],
    limited: &[f64],
    unlimited: &[f64],
) -> Vec<(f64, f64, f64)> {
    let frequencies = design_freqs();
    let mut dips = Vec::new();
    let mut current = None;
    for (i, &(depth, fill)) in bins.iter().enumerate() {
        if depth >= 4.0 && unlimited[i] - limited[i] >= 0.5 {
            let candidate = (frequencies[i], depth, fill);
            if current.is_none_or(|(_, deepest, _)| depth > deepest) {
                current = Some(candidate);
            }
        } else if let Some(dip) = current.take() {
            dips.push(dip);
        }
    }
    if let Some(dip) = current {
        dips.push(dip);
    }
    dips.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.total_cmp(&b.0)));
    dips.truncate(6);
    dips
}

/// Evaluate the actual pass-1 RC response against each position's target error.
/// Position weights apply to gains, not to spectra averaged before scoring.
pub(crate) fn agreement_limit(input: &RcInput<'_>, correction: &[C]) -> f64 {
    let f = design_freqs();
    let width = 2.0_f64.powf(1.0 / 6.0);
    let bands: Vec<_> = levels::RC_BAND_CENTRES
        .iter()
        .map(|fc| f.partition_point(|f| *f < fc / width)..f.partition_point(|f| *f <= fc * width))
        .collect();
    let target = levels::target_db(
        &input.config.target,
        &f.iter().map(|v| v.max(1.0)).collect::<Vec<_>>(),
    );
    // Use the same power-to-dB floor as the measured curve. The fitting
    // helper `db` adds 1e-12 to magnitude, making unity look like a tiny
    // improvement for a negative error instead of the required zero gain.
    let correction: Vec<_> = correction
        .iter()
        .map(|&h| {
            let m = magnitude(h);
            10.0 * (m * m + 1e-30).log10()
        })
        .collect();
    let weights = levels::weights(input.positions, input.main_position);
    let mut gains = [0.0; 14];
    let mut total = 0.0;
    for (position, h) in input.comb {
        let weight = weights[input
            .positions
            .iter()
            .position(|p| p == position)
            .expect("engine position")];
        let power: Vec<_> = h
            .iter()
            .map(|v| {
                let m = magnitude(*v);
                m * m
            })
            .collect();
        let measured = smoothed_db(&power);
        for (gain, band) in gains.iter_mut().zip(&bands) {
            let mut before = 0.0;
            let mut after = 0.0;
            for i in band.clone() {
                let error = measured[i] - target[i] - input.level_db;
                before += error;
                after += error + correction[i];
            }
            *gain += weight * (before.abs() - after.abs()) / band.len() as f64;
        }
        total += weight;
    }
    let mut limit = 100.0;
    for (fc, gain) in levels::RC_BAND_CENTRES.iter().zip(gains) {
        if (gain / total).partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            break;
        }
        limit = fc * width;
    }
    // Clamp only after taking the median over mains. Clamping each main here
    // would lower the median of an even-sized set near the 2 kHz boundary.
    limit
}

/// Sorted union of the resolved primary band and the manually set extra bands.
/// Keep legacy primary bounds untouched when there are no extras.
pub(crate) fn band_set(lo: f64, hi: f64, extra: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut bands = vec![(lo, hi)];
    bands.extend_from_slice(extra);
    merge(bands)
}

/// Sort by lower edge and merge intervals that overlap or touch.
fn merge(mut bands: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    bands.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (lo, hi) in bands {
        if let Some(last) = merged.last_mut()
            && lo <= last.1
        {
            last.1 = last.1.max(hi);
        } else {
            merged.push((lo, hi));
        }
    }
    merged
}

pub(crate) fn in_bands(f: f64, bands: &[(f64, f64)]) -> bool {
    bands.iter().any(|&(lo, hi)| f >= lo && f <= hi)
}

/// A geometric grid of `n` points per band, from `max(lo, floor)` to `hi`.
/// A lone band is the legacy grid as it was, falling when an automatic upper
/// limit lies below `rc_lo`. With extra bands an interval that holds no
/// frequency (`max(lo, floor) >= hi`) gets no points, so every point lies in
/// a band.
pub(crate) fn band_grid(bands: &[(f64, f64)], floor: f64, n: usize) -> Vec<f64> {
    if let [(lo, hi)] = bands {
        return geomspace(lo.max(floor), *hi, n);
    }
    bands
        .iter()
        .filter(|&&(lo, hi)| lo.max(floor) < hi)
        .flat_map(|&(lo, hi)| geomspace(lo.max(floor), hi, n))
        .collect()
}

/// What a Zero-mode PEQ fit over `lo..hi` weighs (ADR 0024). Without an extra
/// band inside `lo..hi` the fit keeps its legacy weights (`None`), so an extra
/// band elsewhere changes nothing in it. With one, the fit weighs where the
/// magnitude ramps reach: the primary band from `lo` to `rc_hi·√2` and each
/// extra band from `lo/√2` to `hi·√2`; outside their union it seeds nothing
/// and weighs 0.15.
pub(crate) fn fit_reach(setup: &Setup, lo: f64, hi: f64) -> Option<Vec<(f64, f64)>> {
    if !setup
        .rc_bands
        .iter()
        .any(|&(band_lo, band_hi)| band_lo <= hi && band_hi >= lo)
    {
        return None;
    }
    let sqrt2 = 2_f64.sqrt();
    Some(merge(
        std::iter::once((lo, setup.rc_hi * sqrt2))
            .chain(
                setup
                    .rc_bands
                    .iter()
                    .map(|&(band_lo, band_hi)| (band_lo / sqrt2, band_hi * sqrt2)),
            )
            .filter(|&(lo, hi)| lo < hi)
            .collect(),
    ))
}

pub(crate) fn magnitude_mask(f: f64, bands: &[(f64, f64)]) -> f64 {
    let lf = f.max(1e-3).log2();
    bands
        .iter()
        .map(|&(lo, hi)| {
            let hi_edge = (hi * 2_f64.sqrt()).min(SR_F / 2.0 * 0.99);
            ((lf - (lo / 2_f64.sqrt()).log2()) / 0.5).clamp(0.0, 1.0)
                * ((hi_edge.log2() - lf) / (hi_edge / hi).log2().max(1e-3)).clamp(0.0, 1.0)
        })
        .fold(0.0, f64::max)
}

pub(crate) fn phase_fade(f: f64, setup: &Setup, bands: &[(f64, f64)]) -> f64 {
    let fade_hz = setup.rc_hi * 2_f64.sqrt();
    let primary = if f < 15.0 {
        (f / 15.0).clamp(0.0, 1.0)
    } else {
        ((fade_hz.log2() - f.max(1e-3).log2()) / 0.5).clamp(0.0, 1.0)
    };
    if setup.rc_bands.is_empty() {
        return primary;
    }
    let lf = f.max(1e-3).log2();
    bands
        .iter()
        .map(|&(lo, hi)| {
            ((lf - (lo / 2_f64.sqrt()).log2()) / 0.5).clamp(0.0, 1.0)
                * (((hi * 2_f64.sqrt()).log2() - lf) / 0.5).clamp(0.0, 1.0)
        })
        .fold(primary, f64::max)
}

/// Where the realizer measures the FIR's magnitude error when it picks the
/// phase strength: the primary band from `max(20, lo_fir)` to `rc_hi·√2`, as
/// without extra bands, and each extra band from its lower edge to its upper
/// edge ×√2 below Nyquist (ADR 0024).
pub(crate) fn phase_error_band(f: f64, lo_fir: f64, setup: &Setup) -> bool {
    f >= 20_f64.max(lo_fir)
        && (f <= setup.rc_hi * 2_f64.sqrt()
            || setup
                .rc_bands
                .iter()
                .any(|&(lo, hi)| f >= lo && f <= (hi * 2_f64.sqrt()).min(SR_F / 2.0 * 0.99)))
}

fn db(h: C) -> f64 {
    20.0 * (magnitude(h) + 1e-12).log10()
}
fn multiply(a: &[C], b: &[C]) -> Vec<C> {
    a.iter().zip(b).map(|(a, b)| a * b).collect()
}
fn rms(v: &[f64]) -> f64 {
    (pairwise_sum(&v.iter().map(|v| v * v).collect::<Vec<_>>()) / v.len() as f64).sqrt()
}

fn no_boost_mask(input: &RcInput<'_>) -> Vec<bool> {
    let f = design_freqs();
    let bands = band_set(input.setup.rc_lo, input.setup.rc_hi, &input.setup.rc_bands);
    let g = band_grid(&bands, 20.0, 1200);
    let gd =
        response::gd_curve(main_response(input), f, &g, Fraction::Scalar(24.0)).expect("GD grid");
    let mut mask = vec![false; BINS];
    let width = 2_f64.powf(1.0 / 12.0);
    for i in 0..g.len() {
        if gd[i] - decay::median(&gd[i.saturating_sub(60)..(i + 60).min(g.len())])
            > input.tuning.noboost_ms
        {
            for (m, &f) in mask.iter_mut().zip(f) {
                *m |= f >= g[i] / width && f <= g[i] * width;
            }
        }
    }
    mask
}
fn long_modes(input: &RcInput<'_>) -> Vec<(f64, f64)> {
    let f = design_freqs();
    let x = lowrate::to_low(main_response(input), f, 300).expect("low-rate grid");
    let bands = band_set(input.setup.rc_lo, input.setup.rc_hi, &input.setup.rc_bands);
    decay::late_peaks(&[&x], 8)
        .into_iter()
        .filter(|&p| p <= 160.0 && in_bands(p, &bands))
        .filter_map(|p| {
            decay::mode_t60(&x, 600.0, p, 1.5)
                .filter(|t| *t != 0.0)
                .map(|t| (p, t))
        })
        .collect()
}
fn long_mode_mask(modes: &[(f64, f64)], notes: &mut Vec<Note>, cap_db: f64) -> Vec<bool> {
    let f = design_freqs();
    let mut mask = vec![false; BINS];
    if modes.len() < 2 {
        return mask;
    }
    let threshold = (1.3 * decay::median(&modes.iter().map(|v| v.1).collect::<Vec<_>>())).max(0.3);
    let mut selected: Vec<_> = modes.iter().copied().filter(|v| v.1 >= threshold).collect();
    let width = 2_f64.powf(1.0 / 24.0);
    for &(p, _) in &selected {
        for (m, &f) in mask.iter_mut().zip(f) {
            *m |= f >= p / width && f <= p * width;
        }
    }
    if mask.iter().any(|m| *m) {
        selected.sort_by(|a, b| a.0.total_cmp(&b.0));
        notes.push(Note::LongModeBoost {
            cap_db,
            modes: selected
                .into_iter()
                .map(|(hz, t60_s)| secs_types::notes::LongMode { hz, t60_s })
                .collect(),
        });
    }
    mask
}
#[allow(clippy::too_many_arguments)]
fn fit_request(
    input: &RcInput<'_>,
    g: &[f64],
    c: &[f64],
    n: usize,
    lo: f64,
    hi: f64,
    f_max: Option<f64>,
    fitter: &mut Fitter<'_>,
) -> Vec<Peq> {
    let reach = (input.config.taps == 0)
        .then(|| fit_reach(input.setup, lo, hi))
        .flatten();
    fitter(&FitRequest {
        g,
        corr_db: c,
        n,
        lo,
        hi,
        boost: input.config.max_boost_db,
        ring_ms: input.tuning.boost_ring_ms,
        bands: reach.as_deref(),
        f_max,
    })
}
fn unwrap(h: &[C]) -> Vec<f64> {
    let mut phase: Vec<_> = h
        .iter()
        .map(|h| {
            let clean = |x: f64| {
                if x.is_nan() {
                    0.0
                } else {
                    x.clamp(-f64::MAX, f64::MAX)
                }
            };
            clean(h.im).atan2(clean(h.re))
        })
        .collect();
    secs_dsp::unwrap_phase(&mut phase);
    phase
}
fn gradient(phase: &[f64]) -> Vec<f64> {
    let f = design_freqs();
    let x: Vec<_> = f.iter().map(|v| 2.0 * PI * v.max(f[1])).collect();
    let mut tau = secs_dsp::gradient(phase, &x);
    for value in &mut tau {
        *value = -*value;
        if !value.is_finite() {
            *value = 0.0;
        }
    }
    tau
}
fn truncate(spec: &[C], taps: usize, protect_dc: bool) -> (Vec<f64>, Vec<C>) {
    let mut h = fft::irfft(spec, N_FFT);
    h.truncate(taps);
    let k = (taps as f64 * 0.1) as usize;
    for (i, v) in h[taps - k..].iter_mut().enumerate() {
        *v *= 0.5 * (1.0 + (PI * i as f64 / k as f64).cos());
    }
    let mut f = measurement::spectrum(&h);
    if protect_dc && correct_dc(&mut h) {
        f = measurement::spectrum(&h);
    }
    (h, f)
}

pub(crate) fn correct_dc(h: &mut [f64]) -> bool {
    // audit #155 Q8: every recorded low-protection case had positive DC sums
    // before and after this guard. For that RC polarity, sum(h) is the DC
    // magnitude, so signed sum(h) - 1 detects the boost in ADR 0022. This is
    // not a magnitude normalizer for arbitrary polarity-inverted FIRs.
    // Truncation can boost DC. Subtract its signed excess with a normalized
    // Hann. Its main lobe extends to about 2 * fs / taps (47 Hz at 2048 taps);
    // only above that range is the effect small, not identically zero.
    let excess = h.iter().sum::<f64>() - 1.0;
    if h.len() <= 2 || excess <= 10_f64.powf(0.5 / 20.0) - 1.0 || !excess.is_finite() {
        return false;
    }
    let window: Vec<_> = (0..h.len())
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (h.len() - 1) as f64).cos()))
        .collect();
    let norm = window.iter().sum::<f64>();
    for (sample, weight) in h.iter_mut().zip(window) {
        *sample -= excess * weight / norm;
    }
    true
}
/// ADR 0025 pole replacement in the engine's RBJ peaking form.
pub(crate) fn decay_cut(
    f0: f64,
    t60_m: f64,
    t60_t: f64,
    height_db: f64,
    long_boost_db: f64,
) -> Option<Peq> {
    if ![f0, t60_m, t60_t, height_db, long_boost_db]
        .iter()
        .all(|v| v.is_finite())
        || f0 <= 0.0
        || t60_t <= 0.0
        || t60_m <= 1.5 * t60_t
    {
        return None;
    }
    let depth = 12_f64
        .min(20.0 * (t60_m / t60_t).log10())
        .min(height_db.max(0.0) + long_boost_db);
    (depth >= 1.0).then(|| {
        (
            f0,
            10_f64.powf(-depth / 40.0) * PI * f0 * t60_m / 6.91,
            -depth,
        )
    })
}

/// Refine a low-rate peak without changing the legacy long-mode mask.
pub(crate) fn refine_mode(main: &[C], p: f64) -> Option<(f64, f64)> {
    let f = design_freqs();
    let lo = f.partition_point(|&f| f < p - 3.0);
    let hi = f.partition_point(|&f| f <= p + 3.0);
    if hi <= lo + 2 || main.len() != f.len() {
        return None;
    }
    let power: Vec<_> = main[lo..hi].iter().map(|&h| magnitude(h)).collect();
    let peak = lo + decay::first_max(&power);
    if peak == lo || peak + 1 == hi {
        return None;
    }
    let [left, middle, right] = [main[peak - 1], main[peak], main[peak + 1]].map(db);
    let curvature = left - 2.0 * middle + right;
    if !curvature.is_finite() || curvature >= 0.0 || middle <= left || middle <= right {
        return None;
    }
    let offset = 0.5 * (left - right) / curvature;
    let hz = f[peak] + offset * (f[peak + 1] - f[peak]);
    let low = lowrate::to_low(main, f, 1200).ok()?;
    let t60 = decay::mode_t60(&low, 600.0, hz, 1.5)?;
    (t60.is_finite() && t60 > 0.0).then_some((hz, t60))
}

fn mode_decay_cuts(
    input: &RcInput<'_>,
    modes: &[(f64, f64)],
    sm: &[f64],
    target: &[f64],
) -> (Vec<Peq>, ModeDecayOutcome) {
    let ir = fft::irfft(main_response(input), N_FFT);
    let times: Vec<_> = [200.0, 250.0, 315.0, 400.0, 500.0]
        .into_iter()
        .filter_map(|fc| decay::mode_t60(&ir, SR_F, fc, 0.115 * fc))
        .collect();
    let target_t60_s = if times.is_empty() {
        0.25
    } else {
        decay::median(&times).max(0.25)
    };
    let mut refined: Vec<_> = modes
        .iter()
        .filter_map(|&(hz, _)| refine_mode(main_response(input), hz))
        .collect();
    refined.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut modes: Vec<(f64, f64)> = Vec::new();
    let mut previous = f64::NEG_INFINITY;
    for mode in refined {
        if mode.0 - previous <= 1.5 {
            let last = modes.last_mut().expect("preceding refined mode");
            if mode.1 > last.1 {
                *last = mode;
            }
        } else {
            modes.push(mode);
        }
        previous = mode.0;
    }
    modes.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut selected = Vec::new();
    let cuts = modes
        .into_iter()
        .filter_map(|(hz, t60)| {
            let height = interp(hz, design_freqs(), sm)
                - interp(hz, design_freqs(), target)
                - input.level_db;
            let cut = decay_cut(hz, t60, target_t60_s, height, input.tuning.long_boost_db)?;
            selected.push((hz, t60, -cut.2));
            Some(cut)
        })
        .take(4)
        .collect();
    (
        cuts,
        ModeDecayOutcome {
            target_t60_s,
            modes: selected,
        },
    )
}

pub(crate) fn lf_pull(e: f64, lift: f64, frac: f64, f: f64, b: f64) -> f64 {
    let upper = if f > 0.0 {
        (1.0 - b) * lift * frac + b * (lift * frac).min(1.5 / f)
    } else {
        lift * frac
    };
    e.clamp(-0.005, upper)
}

fn lf_weight(f: f64, f_lin: f64) -> f64 {
    if f_lin >= 200.0 || f <= 0.0 {
        return 0.0;
    }
    let below = (2.0 * (f / f_lin).log2() + 1.0).clamp(0.0, 1.0);
    let above = (1.0 - 2.0 * (f / 200.0).log2()).clamp(0.0, 1.0);
    0.5 * (1.0 - (PI * below).cos()) * 0.5 * (1.0 - (PI * above).cos())
}

// Keep only scalar bounds and normalization; weights need no extra grid.
struct HfPhaseRegion {
    lo: f64,
    hi: f64,
    log_width: f64,
    weight_sum: f64,
}
impl HfPhaseRegion {
    fn new(setup: &Setup, excess: &[f64]) -> Self {
        let lo = setup
            .rc_bands
            .iter()
            .map(|&(_, hi)| hi * 2_f64.sqrt())
            .fold(setup.rc_hi * 2_f64.sqrt(), f64::max);
        debug_assert!(
            design_freqs()
                .iter()
                .zip(excess)
                .all(|(&f, &e)| f < lo || e == 0.0)
        );
        let hi = (8.0 * lo).min(0.45 * SR_F);
        let mut region = Self {
            lo,
            hi,
            log_width: (hi / lo).ln(),
            weight_sum: 0.0,
        };
        if hi >= 2.0 * lo {
            for &f in design_freqs() {
                region.weight_sum += region.weight(f);
            }
        }
        region
    }

    fn weight(&self, f: f64) -> f64 {
        if f < self.lo || f > self.hi {
            return 0.0;
        }
        let x = (f / self.lo).ln() / self.log_width;
        0.5 * (1.0 - (2.0 * PI * x).cos())
    }
}

struct Realizer<'a> {
    excess: &'a [f64],
    band: &'a [bool],
    lift: f64,
    taps: usize,
    beta: Option<&'a [f64]>,
    protect_dc: bool,
    hf_phase: Option<HfPhaseRegion>,
}
impl Realizer<'_> {
    fn realize(&self, frac: f64, m: &[C]) -> (Vec<f64>, Vec<C>, f64, f64, Option<HfPhaseOutcome>) {
        let mut outcome = self.hf_phase.as_ref().map(|_| HfPhaseOutcome {
            rotation_deg: None,
            limited: false,
        });
        let mut sum = 0.0;
        let spec: Vec<_> = if let Some(region) = &self.hf_phase
            && region.weight_sum > 0.0
        {
            let applied = |i: usize, e: f64| {
                if let Some(beta) = self.beta {
                    lf_pull(e, self.lift, frac, design_freqs()[i], beta[i])
                } else {
                    e.clamp(-0.005, self.lift * frac)
                }
            };
            let mut total = 0.0;
            for (i, &e) in self.excess.iter().enumerate() {
                total += applied(i, e);
            }
            let df = design_freqs()[1];
            // The phase integral includes the lift at bin zero.
            let rho = 2.0 * PI * df * (total - self.lift);
            let wrapped = rho.rem_euclid(2.0 * PI);
            let rotation = if wrapped > PI {
                wrapped - 2.0 * PI
            } else {
                wrapped
            };
            // Always add delay in the region; the note uses the signed angle.
            let correction = wrapped / (2.0 * PI * df);
            let mut limited = false;
            let spec = m
                .iter()
                .zip(self.excess)
                .enumerate()
                .map(|(i, (&m, &e))| {
                    let mut value = applied(i, e);
                    let f = design_freqs()[i];
                    if f >= region.lo && f <= region.hi {
                        let requested = value - correction * region.weight(f) / region.weight_sum;
                        value = requested.clamp(-0.005, self.lift);
                        limited |= value != requested;
                    }
                    sum += self.lift - value;
                    m * C::from_polar(1.0, -2.0 * PI * sum * df)
                })
                .collect();
            outcome = Some(HfPhaseOutcome {
                rotation_deg: Some(rotation.to_degrees()),
                limited,
            });
            spec
        } else {
            // Preserve the legacy loop and its arithmetic order when off or
            // when less than an octave remains above the phase fade.
            m.iter()
                .zip(self.excess)
                .enumerate()
                .map(|(i, (&m, &e))| {
                    let applied = if let Some(beta) = self.beta {
                        lf_pull(e, self.lift, frac, design_freqs()[i], beta[i])
                    } else {
                        e.clamp(-0.005, self.lift * frac)
                    };
                    sum += self.lift - applied;
                    m * C::from_polar(1.0, -2.0 * PI * sum * design_freqs()[1])
                })
                .collect()
        };
        let (h, f) = truncate(&spec, self.taps, self.protect_dc);
        let err: Vec<_> = f
            .iter()
            .zip(m)
            .zip(self.band)
            .filter(|(_, b)| **b)
            .map(|((&f, &m), _)| db(f) - db(m))
            .collect();
        let rms = rms(&err);
        let max = err.iter().map(|v| v.abs()).fold(0.0, f64::max);
        (h, f, rms, max, outcome)
    }
}
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(test), allow(dead_code))]
pub fn correction(input: &RcInput<'_>, phase_frac: f64, fitter: &mut Fitter<'_>) -> RcOutput {
    correction_traced(input, phase_frac, fitter, &mut Trace::default())
}

// The first PEQ fit and position-common excess phase do not depend on the
// candidate phase fraction. Each candidate owns only the values it modifies.
struct FirPreparation {
    peq: Vec<Peq>,
    p: Vec<C>,
    m: Vec<C>,
    u: Vec<C>,
    nb: Vec<bool>,
    g: Vec<f64>,
    lo_fir: f64,
    rolloff: Option<f64>,
    protection: crate::low_protect::LimitOutcome,
    design_db: Vec<f64>,
    notes: Vec<Note>,
    lf: Option<LfPreparation>,
    cuts: Vec<Peq>,
    dr: Option<Vec<C>>,
    mode_decay: Option<ModeDecayOutcome>,
    dip_limit: Option<Vec<(f64, f64, f64)>>,
}

struct LfPreparation {
    ut: Vec<C>,
    lm: Vec<bool>,
    beta: Vec<f64>,
    target: Vec<f64>,
    f_lin: f64,
}

enum RcPreparation {
    Complete(RcOutput),
    Fir(FirPreparation),
}

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn correction_traced(
    input: &RcInput<'_>,
    phase_frac: f64,
    fitter: &mut Fitter<'_>,
    trace: &mut Trace,
) -> RcOutput {
    let ctx = secs_compute::Context::serial();
    let prepared = prepare_correction(input, fitter, trace, &ctx).expect("serial context");
    finish_correction(input, phase_frac, &prepared, fitter, trace, &ctx).expect("serial context")
}

fn prepare_correction(
    input: &RcInput<'_>,
    fitter: &mut Fitter<'_>,
    trace: &mut Trace,
    ctx: &secs_compute::Context,
) -> Result<RcPreparation, EngineError> {
    ctx.cancel().check()?;
    let f = design_freqs();
    let (s, c, t) = (input.setup, input.config, input.tuning);
    let weights = levels::freq_weights(input.positions, input.main_position, t.main_weight_lo);
    let mut power = vec![0.0; BINS];
    let mut total = vec![0.0; BINS];
    for (pos, h) in input.comb {
        let w = &weights[input
            .positions
            .iter()
            .position(|p| p == pos)
            .expect("engine position")];
        for i in 0..BINS {
            let m = magnitude(h[i]);
            power[i] += w[i] * (m * m);
            total[i] += w[i];
        }
    }
    for (p, w) in power.iter_mut().zip(total) {
        *p /= w;
    }
    let mut sm = smoothed_db(&power);
    let modes = long_modes(input);
    let target = c
        .target
        .sample(&f.iter().map(|v| v.max(1.0)).collect::<Vec<_>>());
    let (cuts, mode_decay) = if s.mode_decay {
        let (cuts, outcome) = mode_decay_cuts(input, &modes, &sm, &target);
        (cuts, Some(outcome))
    } else {
        (Vec::new(), None)
    };
    let dr = s.mode_decay.then(|| response::peq_response(f, &cuts));
    if let Some(dr) = &dr {
        // Apply the cut before each position's weighted power sum.
        power.fill(0.0);
        let mut total = vec![0.0; BINS];
        for (pos, h) in input.comb {
            let w = &weights[input
                .positions
                .iter()
                .position(|p| p == pos)
                .expect("engine position")];
            for i in 0..BINS {
                let m = magnitude(h[i] * dr[i]);
                power[i] += w[i] * (m * m);
                total[i] += w[i];
            }
        }
        for (p, w) in power.iter_mut().zip(total) {
            *p /= w;
        }
        sm = smoothed_db(&power);
    }
    let mut corr: Vec<_> = (0..BINS)
        .map(|i| {
            let sm = sm[i];
            (target[i] + input.level_db - sm).clamp(
                CUT_FLOOR_DB,
                if f[i] > 16000.0 {
                    c.max_boost_db.min(2.0)
                } else {
                    c.max_boost_db
                },
            )
        })
        .collect();
    // The dip cap (ADR 0034) and the target it would have left alone: the
    // other limits below apply to both, so the notes name only the dips the
    // cap really reduced.
    let mut dip_bins = s.dip_limit.then(|| {
        let unlimited = corr.clone();
        (
            limit_dip_fills(&power, &sm, &target, input.level_db, &mut corr),
            unlimited,
        )
    });
    let mut notes = Vec::new();
    let mut nb = no_boost_mask(input);
    let channel_rolloff = (c.low_protect || s.lf_align)
        .then(|| crate::low_protect::rolloff(input.comb, input.main_position, None))
        .flatten();
    let rolloff = protection_rolloff(input, channel_rolloff);
    if let Some(hz) = rolloff {
        for (masked, &f) in nb.iter_mut().zip(f) {
            *masked |= f < hz;
        }
    }
    let lm = long_mode_mask(&modes, &mut notes, t.long_boost_db);
    ctx.cancel().check()?;
    trace.no_boost = nb.clone();
    trace.long_mode = lm.clone();
    let bands = band_set(s.rc_lo, s.rc_hi, &s.rc_bands);
    for i in 0..BINS {
        let limit = |mut value: f64| {
            if nb[i] {
                value = value.min(0.0);
            }
            if let Some(hz) = rolloff
                && f[i] >= hz
                && f[i] < 2.0 * hz
            {
                value = value.min(c.max_boost_db * (f[i] / hz).log2());
            }
            if lm[i]
                || cuts.iter().any(|&(hz, _, _)| {
                    let width = 2_f64.powf(1.0 / 24.0);
                    f[i] >= hz / width && f[i] <= hz * width
                })
            {
                value = value.min(t.long_boost_db);
            }
            value * magnitude_mask(f[i], &bands)
        };
        corr[i] = limit(corr[i]);
        if let Some((_, unlimited)) = dip_bins.as_mut() {
            unlimited[i] = limit(unlimited[i]);
        }
    }
    let design_db = corr.clone();
    let dip_limit = dip_bins.map(|(bins, unlimited)| dip_notes(&bins, &corr, &unlimited));
    let mut lf = (s.lf_align && c.taps > 0).then(|| {
        let f_lin = channel_rolloff.map_or(s.rc_lo, |hz| (1.5 * hz).max(s.rc_lo));
        let beta: Vec<_> = f
            .iter()
            .enumerate()
            .map(|(i, &f)| {
                if nb[i] || lm[i] {
                    0.0
                } else {
                    lf_weight(f, f_lin)
                }
            })
            .collect();
        let beta = smooth(&beta, 6.0)
            .into_iter()
            .map(|b| b.clamp(0.0, 1.0))
            .collect();
        let predicted = minimum_phase(
            &sm.iter()
                .zip(&corr)
                .map(|(s, c)| 10_f64.powf((s + c) / 20.0))
                .collect::<Vec<_>>(),
        )
        .expect("magnitude");
        let tau = smooth(&gradient(&unwrap(&predicted)), 3.0);
        let at_lin = interp(f_lin, f, &tau);
        let target = f
            .iter()
            .zip(tau)
            .map(|(&f, tau)| {
                if f < f_lin {
                    (tau - at_lin).max(0.0)
                } else {
                    0.0
                }
            })
            .collect();
        LfPreparation {
            ut: vec![C::new(0.0, 0.0); BINS],
            lm: lm.clone(),
            beta,
            target,
            f_lin,
        }
    });
    let mut peq = Vec::new();
    let mut protection = crate::low_protect::LimitOutcome::default();
    let mut p = vec![C::new(1.0, 0.0); BINS];
    if c.taps == 0 {
        let g = geomspace(15.0, 20000.0, 900);
        let cg: Vec<_> = g.iter().map(|&g| interp(g, f, &corr)).collect();
        let low: Vec<_> = g
            .iter()
            .zip(&cg)
            .map(|(&g, &v)| if g <= 500.0 { v } else { 0.0 })
            .collect();
        peq = fit_request(
            input,
            &g,
            &low,
            s.peq_bands / 2,
            rolloff.map_or(15.0_f64.max(s.rc_lo / 1.4), |hz| {
                hz.max(15.0).max(s.rc_lo / 1.4)
            }),
            400.0,
            None,
            fitter,
        );
        ctx.cancel().check()?;
        let rest: Vec<_> = g
            .iter()
            .zip(&cg)
            .zip(response::peq_response(&g, &peq))
            .map(|((&g, &v), p)| if g >= 300.0 { v - db(p) } else { 0.0 })
            .collect();
        peq.extend(fit_request(
            input,
            &g,
            &rest,
            s.peq_bands - s.peq_bands / 2,
            400.0,
            bands.last().expect("primary band").1.min(20000.0),
            None,
            fitter,
        ));
        ctx.cancel().check()?;
        if let Some(hz) = rolloff {
            crate::low_protect::clamp_centres(&mut peq, hz);
        }
        let mut design_db = design_db;
        if let Some(dr) = &dr {
            for (d, &h) in design_db.iter_mut().zip(dr) {
                *d += db(h);
            }
            peq.splice(0..0, cuts);
        }
        return Ok(RcPreparation::Complete(RcOutput {
            fir: vec![1.0],
            response: response::peq_response(f, &peq),
            peq,
            phase_strength: 0.0,
            phase_band: None,
            design_db,
            notes,
            lf_align: None,
            mode_decay,
            hf_phase: None,
            dip_limit,
            protection,
            rolloff,
        }));
    }
    let g = geomspace(15.0, 20000.0, 700);
    let lo_fir = 1.5 * SR_F / c.taps as f64;
    if s.peq_bands > 0 {
        let cg: Vec<_> = g.iter().map(|&g| interp(g, f, &corr)).collect();
        peq = fit_request(
            input,
            &g,
            &cg,
            s.peq_bands,
            rolloff.map_or(20.0, |hz| hz.max(20.0)),
            500.0,
            Some(s.rc_hi),
            fitter,
        );
        ctx.cancel().check()?;
        if let Some(hz) = rolloff {
            crate::low_protect::clamp_centres(&mut peq, hz);
            protection = crate::low_protect::limit(
                &mut peq,
                &vec![C::new(1.0, 0.0); BINS],
                hz,
                Default::default(),
            );
        }
        p = response::peq_response(f, &peq);
        for i in 0..BINS {
            corr[i] =
                (corr[i] - db(p[i])) * ((f[i] - lo_fir / 2.0) / (lo_fir / 2.0)).clamp(0.0, 1.0);
        }
    }
    let m = minimum_phase(
        &corr
            .iter()
            .map(|v| 10_f64.powf(v / 20.0))
            .collect::<Vec<_>>(),
    )
    .expect("magnitude");
    let weights = levels::weights(input.positions, input.main_position);
    let frac = secs_dsp::grid::frac_for(f);
    let mut u = vec![C::new(0.0, 0.0); BINS];
    let mut total = 0.0;
    for (pos, h) in input.comb {
        if t.phase_positions == PhasePositions::Main && *pos != input.main_position {
            continue;
        }
        ctx.cancel().check()?;
        let w = weights[input.positions.iter().position(|p| p == pos).unwrap()];
        total += w;
        let mut reference = if let Some(dr) = &dr {
            multiply(&multiply(&multiply(h, dr), &p), &m)
        } else {
            multiply(&multiply(h, &p), &m)
        };
        let ir = fft::irfft(&reference, N_FFT);
        let peak = decay::first_max(&ir.iter().map(|v| v.abs()).collect::<Vec<_>>());
        trace.peaks.push((*pos, peak));
        let time = peak as f64 / SR_F;
        for (r, &f) in reference.iter_mut().zip(f) {
            *r *= C::from_polar(1.0, 0.0 + (2.0 * PI * f) * time);
        }
        let power: Vec<_> = reference
            .iter()
            .map(|&h| {
                let m = magnitude(h);
                m * m
            })
            .collect();
        let sm = smooth::smooth_power(f, &power, Fraction::PerBin(&frac)).expect("design grid");
        let mp = minimum_phase(&sm.iter().map(|v| v.sqrt() + 1e-12).collect::<Vec<_>>())
            .expect("magnitude");
        for i in 0..BINS {
            if let Some(lf) = &mut lf {
                lf.ut[i] += divide(
                    C::new(w, 0.0) * reference[i],
                    C::new(magnitude(reference[i]) + 1e-30, 0.0),
                );
            }
            let ex = divide(reference[i], mp[i] + C::new(1e-30, 0.0));
            u[i] += divide(C::new(w, 0.0) * ex, C::new(magnitude(ex) + 1e-30, 0.0));
        }
    }
    for u in &mut u {
        *u = divide(*u, C::new(total, 0.0));
    }
    if let Some(lf) = &mut lf {
        for u in &mut lf.ut {
            *u = divide(*u, C::new(total, 0.0));
        }
    }
    Ok(RcPreparation::Fir(FirPreparation {
        peq,
        p,
        m,
        u,
        nb,
        g,
        lo_fir,
        rolloff,
        protection,
        design_db,
        notes,
        lf,
        cuts,
        dr,
        mode_decay,
        dip_limit,
    }))
}

fn finish_correction(
    input: &RcInput<'_>,
    phase_frac: f64,
    prepared: &RcPreparation,
    fitter: &mut Fitter<'_>,
    trace: &mut Trace,
    ctx: &secs_compute::Context,
) -> Result<RcOutput, EngineError> {
    ctx.cancel().check()?;
    let RcPreparation::Fir(prepared) = prepared else {
        let RcPreparation::Complete(out) = prepared else {
            unreachable!()
        };
        return Ok(out.clone());
    };
    let f = design_freqs();
    let (s, c) = (input.setup, input.config);
    let mut protection = prepared.protection.clone();
    let mut peq = prepared.peq.clone();
    let mut p = prepared.p.clone();
    let mut m = prepared.m.clone();
    let u = &prepared.u;
    let nb = &prepared.nb;
    let g = &prepared.g;
    let lo_fir = prepared.lo_fir;
    let design_db = prepared.design_db.clone();
    let notes = prepared.notes.clone();
    let phase =
        unwrap(&smooth::smooth_complex(f, u, Fraction::Scalar(phase_frac)).expect("design grid"));
    let tau = smooth(&gradient(&phase), phase_frac);
    let hf: Vec<_> = f
        .iter()
        .zip(&tau)
        .filter(|(f, _)| **f >= 3000.0 && **f <= 6000.0)
        .map(|(_, t)| *t)
        .collect();
    let reference = decay::median(&hf);
    let bands = band_set(s.rc_lo, s.rc_hi, &s.rc_bands);
    let total_error = prepared.lf.as_ref().map(|lf| {
        debug_assert_eq!(lf.lm.len(), BINS);
        let total =
            smooth::smooth_complex(f, &lf.ut, Fraction::Scalar(phase_frac)).expect("design grid");
        let phase = unwrap(&total);
        let tau = smooth(&gradient(&phase), 3.0);
        let reference = decay::median(
            &f.iter()
                .zip(&tau)
                .filter(|(f, _)| (3000.0..=6000.0).contains(*f))
                .map(|(_, &t)| t)
                .collect::<Vec<_>>(),
        );
        tau.iter()
            .zip(&lf.target)
            .enumerate()
            .map(|(i, (t, target))| {
                // Legacy unwrap cleans non-finite complex inputs. Preserve
                // their invalidity here so the new term falls back instead.
                if [lf.ut[i].re, lf.ut[i].im, total[i].re, total[i].im]
                    .iter()
                    .all(|v| v.is_finite())
                {
                    t - reference - target
                } else {
                    f64::NAN
                }
            })
            .collect::<Vec<_>>()
    });
    let excess: Vec<_> = f
        .iter()
        .zip(&tau)
        .enumerate()
        .map(|(i, (&f, &tau))| {
            let fade = phase_fade(f, s, &bands);
            if let (Some(lf), Some(total)) = (&prepared.lf, &total_error) {
                let b = lf.beta[i];
                let e = fade * ((1.0 - b) * (tau - reference) + b * total[i]);
                if total[i].is_finite() && b.is_finite() && e.is_finite() {
                    return e;
                }
            }
            (tau - reference) * fade
        })
        .collect();
    let band: Vec<_> = f.iter().map(|&f| phase_error_band(f, lo_fir, s)).collect();
    let realizer = Realizer {
        excess: &excess,
        band: &band,
        lift: s.lift_ms / 1000.0,
        taps: c.taps,
        beta: prepared.lf.as_ref().map(|lf| lf.beta.as_slice()),
        protect_dc: c.low_protect,
        hf_phase: (s.hf_phase && s.lift_ms > 0.0).then(|| HfPhaseRegion::new(s, &excess)),
    };
    let (mut h, mut response, rms0, max0, mut hf_phase) = realizer.realize(0.0, &m);
    trace.ladder.push((0.0, rms0, max0));
    let mut strength = 0.0;
    if s.lift_ms <= 0.0 {
        (h, response) = truncate(&m, c.taps, c.low_protect);
    } else {
        for fraction in [0.9, 0.7, 0.5, 0.3] {
            ctx.cancel().check()?;
            let (hh, ff, rms, max, outcome) = realizer.realize(fraction, &m);
            trace.ladder.push((fraction, rms, max));
            if rms <= 0.5_f64.max(rms0 + 0.3) && max <= 2_f64.max(max0 + 1.0) {
                h = hh;
                response = ff;
                hf_phase = outcome;
                strength = fraction;
                break;
            }
        }
    }
    if s.peq_bands > 0 {
        let resid: Vec<_> = (0..BINS)
            .map(|i| {
                let r = design_db[i] - db(response[i]);
                if nb[i] { r.min(0.0) } else { r }
            })
            .collect();
        let cg: Vec<_> = g.iter().map(|&g| interp(g, f, &resid)).collect();
        peq = fit_request(
            input,
            g,
            &cg,
            s.peq_bands,
            prepared.rolloff.map_or(20.0, |hz| hz.max(20.0)),
            500.0,
            Some(s.rc_hi),
            fitter,
        );
        ctx.cancel().check()?;
        let nb_float: Vec<_> = nb.iter().map(|b| if *b { 1.0 } else { 0.0 }).collect();
        for (hz, _, gain) in &mut peq {
            if interp(*hz, f, &nb_float) > 0.5 {
                *gain = gain.min(0.0);
            }
        }
        if let Some(hz) = prepared.rolloff {
            protection = crate::low_protect::limit(
                &mut peq,
                &vec![C::new(1.0, 0.0); BINS],
                hz,
                Default::default(),
            );
        }
        p = response::peq_response(f, &peq);
        let rc: Vec<_> = f
            .iter()
            .map(|&f| f >= 20.0 && in_bands(f, &bands))
            .collect();
        let design_error = |fx: &[C]| {
            let power: Vec<_> = multiply(fx, &p)
                .iter()
                .map(|&h| {
                    let m = magnitude(h);
                    m * m
                })
                .collect();
            let error: Vec<_> = smooth(&power, 24.0)
                .iter()
                .enumerate()
                .filter(|(i, _)| rc[*i])
                .map(|(i, v)| 10.0 * (v + 1e-30).log10() - design_db[i])
                .collect();
            let med = decay::median(&error);
            rms(&error.iter().map(|e| e - med).collect::<Vec<_>>())
        };
        for _ in 0..2 {
            ctx.cancel().check()?;
            let residual: Vec<_> = (0..BINS)
                .map(|i| {
                    if rc[i] {
                        design_db[i] - db(response[i] * p[i])
                    } else {
                        0.0
                    }
                })
                .collect();
            let mut broad = smooth(&residual, 3.0);
            if let Some(hz) = prepared.rolloff {
                for (value, &f) in broad.iter_mut().zip(f) {
                    if f < hz {
                        *value = value.min(0.0);
                    }
                }
            }
            let mag: Vec<_> = (0..BINS)
                .map(|i| {
                    let fade = if rc[i] {
                        ((f[i] - lo_fir / 2.0) / (lo_fir / 2.0)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    magnitude(m[i]) * 10_f64.powf(broad[i] * fade / 20.0)
                })
                .collect();
            let m2 = minimum_phase(&mag).expect("magnitude");
            let (h2, f2) = if s.lift_ms > 0.0 {
                // The outcome depends on excess and strength, not magnitude.
                let (h, f, _, _, _) = realizer.realize(strength, &m2);
                (h, f)
            } else {
                truncate(&m2, c.taps, c.low_protect)
            };
            if design_error(&f2) >= design_error(&response) {
                break;
            }
            h = h2;
            response = f2;
            m = m2;
        }
    }
    let lf_align = prepared.lf.as_ref().map(|lf| {
        let mut limited = lf.f_lin >= 200.0 || strength == 0.0;
        for i in 0..BINS {
            if lf.beta[i] >= 0.5 {
                let applied = lf_pull(excess[i], realizer.lift, strength, f[i], lf.beta[i]);
                limited |= excess[i] - applied > 0.001;
            }
        }
        LfAlignOutcome {
            f_lin: lf.f_lin,
            strength,
            // The selected channel response supplies the measured arrival.
            arrival_ms: None,
            limited,
        }
    });
    let mut response = multiply(&response, &p);
    let mut design_db = design_db;
    if let Some(dr) = &prepared.dr {
        response = multiply(&response, dr);
        for (d, &h) in design_db.iter_mut().zip(dr) {
            *d += db(h);
        }
        peq.splice(0..0, prepared.cuts.iter().copied());
        protection.reduced = protection
            .reduced
            .into_iter()
            .map(|index| index + prepared.cuts.len())
            .collect();
    }
    Ok(RcOutput {
        fir: h,
        response,
        peq,
        phase_strength: strength,
        phase_band: Some(strength),
        design_db,
        notes,
        lf_align,
        mode_decay: prepared.mode_decay.clone(),
        hf_phase,
        dip_limit: prepared.dip_limit.clone(),
        protection,
        rolloff: prepared.rolloff,
    })
}

pub(crate) fn phase_score(input: &RcInput<'_>, out: &RcOutput) -> f64 {
    let bands = band_set(input.setup.rc_lo, input.setup.rc_hi, &input.setup.rc_bands);
    let g = band_grid(&bands, 20.0, 1200);
    let gd = response::gd_curve(
        &multiply(main_response(input), &out.response),
        design_freqs(),
        &g,
        Fraction::Scalar(24.0),
    )
    .expect("GD grid");
    let med = decay::median(&gd);
    gd.iter().map(|v| v - med).fold(f64::NEG_INFINITY, f64::max)
}

/// Compare phase candidates while sharing their phase-independent preparation.
/// The supplied fitter must return the same result for the same request: an
/// identical preparation is evaluated once, including the PEQ-only path.
/// Sequential replay clients must account for the shared preparation call.
/// Returns `Err(Cancelled)` when the context token is cancelled; the PEQ fitter is
/// expected to watch the same token.
#[cfg(test)]
pub fn channel(
    input: &RcInput<'_>,
    fitter: &mut Fitter<'_>,
    ctx: &secs_compute::Context,
) -> Result<ChannelRc, EngineError> {
    channel_progress(input, fitter, ctx, &|| {})
}

/// Preparation, each requested phase realization, and final protection/cuts.
pub(crate) fn channel_steps(config: &EngineConfig) -> u64 {
    3 + u64::from(config.phase_frac > 6.0)
}

pub(crate) fn channel_progress(
    input: &RcInput<'_>,
    fitter: &mut Fitter<'_>,
    ctx: &secs_compute::Context,
    advance: &dyn Fn(),
) -> Result<ChannelRc, EngineError> {
    let bands = band_set(input.setup.rc_lo, input.setup.rc_hi, &input.setup.rc_bands);
    channel_with_cuts_progress(
        input,
        fitter,
        &mut |h, lo, hi| {
            if input.setup.rc_bands.is_empty() {
                fit::fit_gd_cuts_until(design_freqs(), h, lo, hi, 6, 3.0, ctx)
            } else {
                fit::fit_gd_cuts_bands_until(design_freqs(), h, &bands, 6, 3.0, ctx)
            }
        },
        ctx,
        advance,
    )
}
type GdFitter<'a> = dyn FnMut(&[C], f64, f64) -> Vec<Peq> + 'a;

#[cfg(test)]
pub(crate) fn channel_with_cuts(
    input: &RcInput<'_>,
    fitter: &mut Fitter<'_>,
    cuts: &mut GdFitter<'_>,
    ctx: &secs_compute::Context,
) -> Result<ChannelRc, EngineError> {
    channel_with_cuts_progress(input, fitter, cuts, ctx, &|| {})
}

fn channel_with_cuts_progress(
    input: &RcInput<'_>,
    fitter: &mut Fitter<'_>,
    cuts: &mut GdFitter<'_>,
    ctx: &secs_compute::Context,
    advance: &dyn Fn(),
) -> Result<ChannelRc, EngineError> {
    let requested = input.config.phase_frac;
    let prepared = prepare_correction(input, fitter, &mut Trace::default(), ctx)?;
    advance();
    let mut out = finish_correction(
        input,
        requested,
        &prepared,
        fitter,
        &mut Trace::default(),
        ctx,
    )?;
    advance();
    let mut used = None;
    if requested > 6.0 {
        let other = finish_correction(input, 6.0, &prepared, fitter, &mut Trace::default(), ctx)?;
        advance();
        let mut notes = out.notes.clone();
        notes.extend(other.notes.clone());
        if phase_score(input, &other) < phase_score(input, &out) {
            out = other;
            used = Some(6.0);
        } else {
            used = Some(requested);
        }
        out.notes = notes;
    }
    if let Some(mode) = &out.mode_decay {
        out.notes.push(Note::ModeDecay {
            role: input.role.to_owned(),
            target_t60_s: mode.target_t60_s,
            modes: mode
                .modes
                .iter()
                .map(|&(hz, t60_s, cut_db)| secs_types::notes::Mode { hz, t60_s, cut_db })
                .collect(),
        });
    }
    if let Some(hf) = &out.hf_phase {
        out.notes.push(Note::HfPhase {
            role: input.role.to_owned(),
            rotation_deg: hf.rotation_deg,
            limited: hf.limited,
        });
    }
    if let Some(dips) = &out.dip_limit {
        out.notes.push(Note::DipLimit {
            role: input.role.to_owned(),
            dips: dips
                .iter()
                .map(|&(hz, depth_db, fill_db)| secs_types::notes::Dip {
                    hz,
                    depth_db,
                    fill_db,
                })
                .collect(),
        });
    }
    let gd_cuts = if input.tuning.gd_cuts {
        let cuts = cuts(
            &multiply(main_response(input), &out.response),
            input.setup.rc_lo.max(20.0),
            input.setup.rc_hi,
        );
        // A cancelled GD fit returns meaningless cuts; discard them here.
        ctx.cancel().check()?;
        cuts
    } else {
        Vec::new()
    };
    if let Some(hz) = out.rolloff {
        let mut padded = out.fir.clone();
        padded.resize(N_FFT, 0.0);
        let fir = fft::rfft(&padded);
        let fixed = multiply(&fir, &response::peq_response(design_freqs(), &gd_cuts));
        let outcome = crate::low_protect::limit(
            &mut out.peq,
            &fixed,
            hz,
            std::mem::take(&mut out.protection),
        );
        let limited = crate::low_protect::limited(&out.peq, &fixed, hz);
        out.response = multiply(&fir, &response::peq_response(design_freqs(), &out.peq));
        out.notes.push(Note::LowProtect {
            role: input.role.to_owned(),
            rolloff_hz: hz,
            hp_hz: None,
            limited,
            reduced: outcome.reduced.len(),
            cuts: outcome.cuts,
        });
        out.protection = outcome;
    }
    if let Some(lf) = &mut out.lf_align {
        let tau = smooth(
            &gradient(&unwrap(&multiply(main_response(input), &out.response))),
            3.0,
        );
        let median = |lo: f64, hi: f64| {
            let values: Vec<_> = design_freqs()
                .iter()
                .zip(&tau)
                .filter(|(f, t)| **f >= lo && **f <= hi && t.is_finite())
                .map(|(_, &t)| t)
                .collect();
            (!values.is_empty()).then(|| decay::median(&values))
        };
        lf.arrival_ms = median(lf.f_lin.max(40.0), 150.0)
            .zip(median(3000.0, 6000.0))
            .map(|(low, high)| 1000.0 * (low - high));
        out.notes.push(Note::LfAlign {
            role: input.role.to_owned(),
            strength: lf.strength,
            arrival_ms: lf.arrival_ms,
            limited: lf.limited,
        });
    }
    let response = if gd_cuts.is_empty() {
        out.response.clone()
    } else {
        out.notes.push(Note::GdCuts {
            role: input.role.to_owned(),
            cuts: gd_cuts
                .iter()
                .map(|&(hz, q, gain_db)| secs_types::notes::Peq { hz, q, gain_db })
                .collect(),
        });
        multiply(
            &out.response,
            &response::peq_response(design_freqs(), &gd_cuts),
        )
    };
    advance();
    Ok(ChannelRc {
        out,
        phase_frac_used: used,
        gd_cuts,
        response,
    })
}

pub fn combine(
    main: &[C],
    align_delay_ms: f64,
    gain_db: f64,
    xo_hz: Option<f64>,
    sub_sum: Option<&[C]>,
    bus_polarity: i8,
    bus_delay_ms: f64,
) -> Vec<C> {
    assert_eq!(main.len(), BINS);
    let delay = measurement::delay_response(align_delay_ms);
    let gain = C::new(10_f64.powf(gain_db / 20.0), 0.0);
    let mut out: Vec<_> = main.iter().zip(delay).map(|(&h, d)| h * d * gain).collect();
    if let (Some(xo), Some(sub)) = (xo_hz.filter(|v| *v != 0.0), sub_sum) {
        assert_eq!(sub.len(), BINS);
        let hp = response::lr4(design_freqs(), xo, "HP");
        let lp = response::lr4(design_freqs(), xo, "LP");
        let delay = measurement::delay_response(bus_delay_ms);
        for i in 0..BINS {
            out[i] =
                out[i] * hp[i] + sub[i] * C::new(f64::from(bus_polarity), 0.0) * delay[i] * lp[i];
        }
    }
    out
}
