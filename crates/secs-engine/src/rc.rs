//! Per-channel room correction, in upstream arithmetic and decision order.
//! Upstream: `_correction`, its masks, and `compute`'s per-main loop.

use crate::{
    C, EngineConfig, Tuning, decay,
    fit::{self, FitRequest, Fitter},
    grid::{BINS, N_FFT, SR_F, design_freqs},
    input::{Peq, PhasePositions},
    levels, measurement,
    notes::Note,
    setup::Setup,
};
use secs_dsp::{
    complex_div as divide, fft, interp, lowrate,
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
}
#[derive(Debug)]
pub struct RcOutput {
    pub fir: Vec<f64>,
    pub response: Vec<C>,
    pub peq: Vec<Peq>,
    pub phase_strength: f64,
    pub phase_band: Option<f64>,
    pub design_db: Vec<f64>,
    pub notes: Vec<Note>,
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
fn geomspace(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let start = lo.log10();
    let step = (hi.log10() - start) / (n - 1) as f64;
    let mut g: Vec<_> = (0..n)
        .map(|i| 10_f64.powf(i as f64 * step + start))
        .collect();
    g[0] = lo;
    g[n - 1] = hi;
    g
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
    let g = geomspace(input.setup.rc_lo.max(20.0), input.setup.rc_hi, 1200);
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
fn long_mode_mask(input: &RcInput<'_>, notes: &mut Vec<Note>) -> Vec<bool> {
    let f = design_freqs();
    let x = lowrate::to_low(main_response(input), f, 300).expect("low-rate grid");
    let modes: Vec<_> = decay::late_peaks(&[&x], 8)
        .into_iter()
        .filter(|&p| p >= input.setup.rc_lo && p <= input.setup.rc_hi.min(160.0))
        .filter_map(|p| {
            decay::mode_t60(&x, 600.0, p, 1.5)
                .filter(|t| *t != 0.0)
                .map(|t| (p, t))
        })
        .collect();
    let mut mask = vec![false; BINS];
    if modes.len() < 2 {
        return mask;
    }
    let threshold = (1.3 * decay::median(&modes.iter().map(|v| v.1).collect::<Vec<_>>())).max(0.3);
    let mut selected: Vec<_> = modes.into_iter().filter(|v| v.1 >= threshold).collect();
    let width = 2_f64.powf(1.0 / 24.0);
    for &(p, _) in &selected {
        for (m, &f) in mask.iter_mut().zip(f) {
            *m |= f >= p / width && f <= p * width;
        }
    }
    if mask.iter().any(|m| *m) {
        selected.sort_by(|a, b| a.0.total_cmp(&b.0));
        notes.push(Note::LongModeBoost {
            cap_db: input.tuning.long_boost_db,
            modes: selected,
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
    fitter(&FitRequest {
        g,
        corr_db: c,
        n,
        lo,
        hi,
        boost: input.config.max_boost_db,
        ring_ms: input.tuning.boost_ring_ms,
        f_max,
    })
}
fn unwrap(h: &[C]) -> Vec<f64> {
    let angle: Vec<_> = h
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
    let mut out = angle.clone();
    let mut correction = 0.0;
    for i in 1..out.len() {
        let d = angle[i] - angle[i - 1];
        let mut dm = (d + PI).rem_euclid(2.0 * PI) - PI;
        if dm == -PI && d > 0.0 {
            dm = PI;
        }
        correction += if d.abs() < PI { 0.0 } else { dm - d };
        out[i] += correction;
    }
    out
}
fn gradient(phase: &[f64]) -> Vec<f64> {
    let f = design_freqs();
    let x: Vec<_> = f.iter().map(|v| 2.0 * PI * v.max(f[1])).collect();
    let dx: Vec<_> = x.windows(2).map(|x| x[1] - x[0]).collect();
    let n = phase.len();
    let mut tau = vec![0.0; n];
    tau[0] = -(phase[1] - phase[0]) / dx[0];
    tau[n - 1] = -(phase[n - 1] - phase[n - 2]) / dx[n - 2];
    for i in 1..n - 1 {
        let (a, b) = (dx[i - 1], dx[i]);
        tau[i] = -((-b / (a * (a + b))) * phase[i - 1]
            + ((b - a) / (a * b)) * phase[i]
            + (a / (b * (a + b))) * phase[i + 1]);
    }
    for v in &mut tau {
        if !v.is_finite() {
            *v = 0.0;
        }
    }
    tau
}
fn truncate(spec: &[C], taps: usize) -> (Vec<f64>, Vec<C>) {
    let mut h = fft::irfft(spec, N_FFT);
    h.truncate(taps);
    let k = (taps as f64 * 0.1) as usize;
    for (i, v) in h[taps - k..].iter_mut().enumerate() {
        *v *= 0.5 * (1.0 + (PI * i as f64 / k as f64).cos());
    }
    let f = measurement::spectrum(&h);
    (h, f)
}
struct Realizer<'a> {
    excess: &'a [f64],
    band: &'a [bool],
    lift: f64,
    taps: usize,
}
impl Realizer<'_> {
    fn realize(&self, frac: f64, m: &[C]) -> (Vec<f64>, Vec<C>, f64, f64) {
        let mut sum = 0.0;
        let spec: Vec<_> = m
            .iter()
            .zip(self.excess)
            .map(|(&m, &e)| {
                sum += self.lift - e.clamp(-0.005, self.lift * frac);
                m * C::from_polar(1.0, -2.0 * PI * sum * design_freqs()[1])
            })
            .collect();
        let (h, f) = truncate(&spec, self.taps);
        let err: Vec<_> = f
            .iter()
            .zip(m)
            .zip(self.band)
            .filter(|(_, b)| **b)
            .map(|((&f, &m), _)| db(f) - db(m))
            .collect();
        let rms = rms(&err);
        let max = err.iter().map(|v| v.abs()).fold(0.0, f64::max);
        (h, f, rms, max)
    }
}
pub fn correction(input: &RcInput<'_>, phase_frac: f64, fitter: &mut Fitter<'_>) -> RcOutput {
    correction_traced(input, phase_frac, fitter, &mut Trace::default())
}
pub(crate) fn correction_traced(
    input: &RcInput<'_>,
    phase_frac: f64,
    fitter: &mut Fitter<'_>,
    trace: &mut Trace,
) -> RcOutput {
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
    let frac = secs_dsp::grid::frac_for(f);
    let sm = smooth::smooth_power(f, &power, Fraction::PerBin(&frac)).expect("design grid");
    let sm3 = smooth(&power, 3.0);
    let target = c
        .target
        .sample(&f.iter().map(|v| v.max(1.0)).collect::<Vec<_>>());
    let mut corr: Vec<_> = (0..BINS)
        .map(|i| {
            let sm = (10.0 * (sm[i] + 1e-30).log10()).max(10.0 * (sm3[i] + 1e-30).log10() - 3.0);
            (target[i] + input.level_db - sm).clamp(
                -25.0,
                if f[i] > 16000.0 {
                    c.max_boost_db.min(2.0)
                } else {
                    c.max_boost_db
                },
            )
        })
        .collect();
    let mut notes = Vec::new();
    let nb = no_boost_mask(input);
    let lm = long_mode_mask(input, &mut notes);
    trace.no_boost = nb.clone();
    trace.long_mode = lm.clone();
    let hi_edge = (s.rc_hi * 2_f64.sqrt()).min(SR_F / 2.0 * 0.99);
    for i in 0..BINS {
        if nb[i] {
            corr[i] = corr[i].min(0.0);
        }
        if lm[i] {
            corr[i] = corr[i].min(t.long_boost_db);
        }
        let lf = f[i].max(1e-3).log2();
        corr[i] *= ((lf - (s.rc_lo / 2_f64.sqrt()).log2()) / 0.5).clamp(0.0, 1.0)
            * ((hi_edge.log2() - lf) / (hi_edge / s.rc_hi).log2().max(1e-3)).clamp(0.0, 1.0);
    }
    let design_db = corr.clone();
    let mut peq = Vec::new();
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
            15.0_f64.max(s.rc_lo / 1.4),
            400.0,
            None,
            fitter,
        );
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
            s.rc_hi.min(20000.0),
            None,
            fitter,
        ));
        return RcOutput {
            fir: vec![1.0],
            response: response::peq_response(f, &peq),
            peq,
            phase_strength: 0.0,
            phase_band: None,
            design_db,
            notes,
        };
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
            20.0,
            500.0,
            Some(s.rc_hi),
            fitter,
        );
        p = response::peq_response(f, &peq);
        for i in 0..BINS {
            corr[i] =
                (corr[i] - db(p[i])) * ((f[i] - lo_fir / 2.0) / (lo_fir / 2.0)).clamp(0.0, 1.0);
        }
    }
    let mut m = minimum_phase(
        &corr
            .iter()
            .map(|v| 10_f64.powf(v / 20.0))
            .collect::<Vec<_>>(),
    )
    .expect("magnitude");
    let weights = levels::weights(input.positions, input.main_position);
    let mut u = vec![C::new(0.0, 0.0); BINS];
    let mut total = 0.0;
    for (pos, h) in input.comb {
        if t.phase_positions == PhasePositions::Main && *pos != input.main_position {
            continue;
        }
        let w = weights[input.positions.iter().position(|p| p == pos).unwrap()];
        total += w;
        let mut reference = multiply(&multiply(h, &p), &m);
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
            let ex = divide(reference[i], mp[i] + C::new(1e-30, 0.0));
            u[i] += divide(C::new(w, 0.0) * ex, C::new(magnitude(ex) + 1e-30, 0.0));
        }
    }
    for u in &mut u {
        *u = divide(*u, C::new(total, 0.0));
    }
    let phase =
        unwrap(&smooth::smooth_complex(f, &u, Fraction::Scalar(phase_frac)).expect("design grid"));
    let tau = smooth(&gradient(&phase), phase_frac);
    let hf: Vec<_> = f
        .iter()
        .zip(&tau)
        .filter(|(f, _)| **f >= 3000.0 && **f <= 6000.0)
        .map(|(_, t)| *t)
        .collect();
    let reference = decay::median(&hf);
    let fade_hz = s.rc_hi * 2_f64.sqrt();
    let excess: Vec<_> = f
        .iter()
        .zip(&tau)
        .map(|(&f, &tau)| {
            let fade = if f < 15.0 {
                (f / 15.0).clamp(0.0, 1.0)
            } else {
                ((fade_hz.log2() - f.max(1e-3).log2()) / 0.5).clamp(0.0, 1.0)
            };
            (tau - reference) * fade
        })
        .collect();
    let band: Vec<_> = f
        .iter()
        .map(|&f| f >= 20_f64.max(lo_fir) && f <= fade_hz)
        .collect();
    let realizer = Realizer {
        excess: &excess,
        band: &band,
        lift: s.lift_ms / 1000.0,
        taps: c.taps,
    };
    let (mut h, mut response, rms0, max0) = realizer.realize(0.0, &m);
    trace.ladder.push((0.0, rms0, max0));
    let mut strength = 0.0;
    if s.lift_ms <= 0.0 {
        (h, response) = truncate(&m, c.taps);
    } else {
        for fraction in [0.9, 0.7, 0.5, 0.3] {
            let (hh, ff, rms, max) = realizer.realize(fraction, &m);
            trace.ladder.push((fraction, rms, max));
            if rms <= 0.5_f64.max(rms0 + 0.3) && max <= 2_f64.max(max0 + 1.0) {
                h = hh;
                response = ff;
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
            &g,
            &cg,
            s.peq_bands,
            20.0,
            500.0,
            Some(s.rc_hi),
            fitter,
        );
        let nb_float: Vec<_> = nb.iter().map(|b| if *b { 1.0 } else { 0.0 }).collect();
        for (hz, _, gain) in &mut peq {
            if interp(*hz, f, &nb_float) > 0.5 {
                *gain = gain.min(0.0);
            }
        }
        p = response::peq_response(f, &peq);
        let rc: Vec<_> = f
            .iter()
            .map(|&f| f >= s.rc_lo.max(20.0) && f <= s.rc_hi)
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
            let residual: Vec<_> = (0..BINS)
                .map(|i| {
                    if rc[i] {
                        design_db[i] - db(response[i] * p[i])
                    } else {
                        0.0
                    }
                })
                .collect();
            let broad = smooth(&residual, 3.0);
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
                let (h, f, _, _) = realizer.realize(strength, &m2);
                (h, f)
            } else {
                truncate(&m2, c.taps)
            };
            if design_error(&f2) >= design_error(&response) {
                break;
            }
            h = h2;
            response = f2;
            m = m2;
        }
    }
    RcOutput {
        fir: h,
        response: multiply(&response, &p),
        peq,
        phase_strength: strength,
        phase_band: Some(strength),
        design_db,
        notes,
    }
}

pub(crate) fn phase_score(input: &RcInput<'_>, out: &RcOutput) -> f64 {
    let g = geomspace(input.setup.rc_lo.max(20.0), input.setup.rc_hi, 1200);
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

pub fn channel(input: &RcInput<'_>, fitter: &mut Fitter<'_>) -> ChannelRc {
    channel_with_cuts(input, fitter, &mut |h, lo, hi| {
        fit::fit_gd_cuts(design_freqs(), h, lo, hi, 6, 3.0)
    })
}
type GdFitter<'a> = dyn FnMut(&[C], f64, f64) -> Vec<Peq> + 'a;

pub(crate) fn channel_with_cuts(
    input: &RcInput<'_>,
    fitter: &mut Fitter<'_>,
    cuts: &mut GdFitter<'_>,
) -> ChannelRc {
    let requested = input.config.phase_frac;
    let mut out = correction(input, requested, fitter);
    let mut used = None;
    if requested > 6.0 {
        let other = correction(input, 6.0, fitter);
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
    let gd_cuts = if input.tuning.gd_cuts {
        cuts(
            &multiply(main_response(input), &out.response),
            input.setup.rc_lo.max(20.0),
            input.setup.rc_hi,
        )
    } else {
        Vec::new()
    };
    let response = if gd_cuts.is_empty() {
        out.response.clone()
    } else {
        out.notes.push(Note::GdCuts {
            role: input.role.to_owned(),
            cuts: gd_cuts.clone(),
        });
        multiply(
            &out.response,
            &response::peq_response(design_freqs(), &gd_cuts),
        )
    };
    ChannelRc {
        out,
        phase_frac_used: used,
        gd_cuts,
        response,
    }
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
