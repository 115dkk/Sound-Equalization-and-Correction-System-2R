//! Port of SECS 2 `src/secs/mimo_iir.py` (GPL-3.0).
//! Joint TRF accumulates position blocks, never a dense Jacobian (ADR 0007).

use std::f64::consts::PI;

use faer::{Accum, Mat, Par, linalg::matmul::matmul};
use secs_compute::{Context, MapError, MemoryError};
use secs_dsp::{
    DspError, complex_div, fft, optimize,
    optimize::{NormalEquations, NormalOptions, XScale},
    pairwise_sum, response,
};

use super::{MimoDesign, MimoRequest, support_path, worst_t60_change};
use crate::{
    C, Design, EngineConfig, EngineError, MeasurementSet, Note, Tuning, decay,
    fit::{FitRequest, fit_peq_until},
    grid::design_freqs,
};

const FS: f64 = 600.0;
const N: usize = 720;
const MU: f64 = 10.0;

#[derive(Debug, Clone, PartialEq)]
pub struct IirSupport {
    pub gain_db: f64,
    pub delay_ms: f64,
    pub polarity: i8,
    pub peq: Vec<(f64, f64, f64)>,
    pub allpass: Vec<(f64, f64)>,
}

impl IirSupport {
    pub fn response(&self, f: &[f64]) -> Vec<C> {
        let gain = 10f64.powf(self.gain_db / 20.0) * f64::from(self.polarity);
        let mut out: Vec<_> = f
            .iter()
            .map(|&f| C::from_polar(1.0, -2.0 * PI * f * self.delay_ms / 1000.0) * gain)
            .collect();
        for &(f0, q, g) in &self.peq {
            for (h, p) in out
                .iter_mut()
                .zip(response::peaking_response(f, f0, q, g, 48000.0))
            {
                *h *= p;
            }
        }
        for &(f0, q) in &self.allpass {
            for (h, p) in out.iter_mut().zip(response::allpass2(f, f0, q)) {
                *h *= p;
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MimoIir {
    pub supports: Vec<(String, Vec<(String, IirSupport)>)>,
    pub removed: Vec<(String, Vec<String>)>,
    pub notes: Vec<Note>,
}

fn check(ctx: &Context) -> Result<(), EngineError> {
    ctx.cancel().check().map_err(|_| EngineError::Cancelled)
}

fn per(t: &Tuning) -> usize {
    2 + 3 * t.mimo_k_bands + 2 * t.mimo_a_bands
}

pub(crate) fn band_of(lo: f64, hi: f64, fsiso: f64, t: &Tuning) -> (f64, f64) {
    let low = 15f64.max(lo * 0.9);
    let high = 300f64.min(hi.min(fsiso) * 1.1);
    if t.mimo_in_band && high > low * 1.5 {
        (low, high)
    } else {
        (15.0, 300.0)
    }
}

pub(crate) fn bounds(band: (f64, f64), t: &Tuning) -> (Vec<f64>, Vec<f64>) {
    let mut lb = vec![-80.0, 0.0];
    let mut ub = vec![t.mimo_gain_max_db, 250.0];
    for _ in 0..t.mimo_k_bands {
        lb.extend([band.0.ln(), 0.5f64.ln(), -t.mimo_band_db]);
        ub.extend([band.1.ln(), t.mimo_q_max.ln(), t.mimo_band_db]);
    }
    for _ in 0..t.mimo_a_bands {
        lb.extend([band.0.ln(), 0.3f64.ln()]);
        ub.extend([band.1.ln(), 20f64.ln()]);
    }
    (lb, ub)
}

pub(crate) fn unpack(x: &[f64], pols: &[i8], t: &Tuning) -> Vec<IirSupport> {
    x.chunks_exact(per(t))
        .zip(pols)
        .map(|(v, &polarity)| IirSupport {
            gain_db: v[0],
            delay_ms: v[1],
            polarity,
            peq: (0..t.mimo_k_bands)
                .map(|k| (v[2 + 3 * k].exp(), v[3 + 3 * k].exp(), v[4 + 3 * k]))
                .collect(),
            allpass: (0..t.mimo_a_bands)
                .map(|a| {
                    let b = 2 + 3 * t.mimo_k_bands + 2 * a;
                    (v[b].exp(), v[b + 1].exp())
                })
                .collect(),
        })
        .collect()
}

fn onset(x: &[f64]) -> usize {
    let top = x.iter().map(|v| v.abs()).fold(0.0, f64::max);
    x.iter().position(|v| v.abs() > 0.1 * top).unwrap_or(0)
}

pub(crate) fn late_modes(xs: &[Vec<f64>], count: usize, lo: f64, hi: f64) -> Vec<f64> {
    let mut spec = vec![0.0; 2049];
    for x in xs {
        let mut late = vec![0.0; 4096];
        let begin = (onset(x) + 90).min(x.len().min(4096));
        let end = x.len().min(4096);
        late[begin..end].copy_from_slice(&x[begin..end]);
        for (s, h) in spec.iter_mut().zip(fft::rfft(&late)) {
            let a = response::magnitude(h);
            *s += a * a;
        }
    }
    let f = secs_dsp::grid::freqs(4096, FS).expect("MIMO grid");
    let masked: Vec<_> = spec
        .iter()
        .zip(&f)
        .map(|(&s, &f)| if (lo..=hi).contains(&f) { s } else { 0.0 })
        .collect();
    let mut indices = secs_dsp::peaks::find_peaks(&masked, None);
    indices.sort_by(|&a, &b| spec[b].total_cmp(&spec[a]));
    indices.into_iter().take(count).map(|i| f[i]).collect()
}

pub(crate) struct SeedResult {
    pub x: Vec<f64>,
    pub polarity: i8,
    pub cost: f64,
    pub nfev: usize,
    pub status: i32,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn fit_one(
    s: &[f64],
    lo: f64,
    hi: f64,
    modes: &[f64],
    band: (f64, f64),
    t: &Tuning,
    ctx: &Context,
) -> Result<SeedResult, EngineError> {
    check(ctx)?;
    let lo = lo.max(15.0);
    let step = (hi.log10() - lo.log10()) / 159.0;
    let mut f: Vec<_> = (0..160)
        .map(|k| 10f64.powf(lo.log10() + k as f64 * step))
        .collect();
    f[0] = lo;
    f[159] = hi;
    let target: Vec<C> = f
        .iter()
        .map(|&f| {
            s.iter()
                .enumerate()
                .map(|(k, &s)| C::from_polar(1.0, -2.0 * PI * f * k as f64 / FS) * s)
                .sum()
        })
        .collect();
    let k = decay::first_max(&s.iter().map(|v| v.abs()).collect::<Vec<_>>());
    let polarity = if s[k] >= 0.0 { 1 } else { -1 };
    let db: Vec<_> = target
        .iter()
        .map(|&h| 20.0 * (response::magnitude(h) + 1e-12).log10())
        .collect();
    let base = decay::median(&db);
    let corr: Vec<_> = db.iter().map(|v| v - base).collect();
    let mut peq = fit_peq_until(
        &FitRequest {
            g: &f,
            corr_db: &corr,
            n: t.mimo_k_bands,
            lo,
            hi,
            boost: t.mimo_band_db,
            ring_ms: None,
            f_max: None,
        },
        &|| ctx.cancel().is_cancelled(),
    );
    check(ctx)?;
    let middle = (lo * hi).sqrt();
    peq.resize(t.mimo_k_bands, (middle, 1.0, 0.0));
    let mut x0 = vec![base, 1000.0 * k as f64 / FS];
    for (f, q, g) in peq {
        x0.extend([f.ln(), q.ln(), g]);
    }
    for a in 0..t.mimo_a_bands {
        x0.extend([
            modes
                .get(a)
                .copied()
                .unwrap_or(middle)
                .clamp(band.0 * 1.03, band.1 * 0.97)
                .ln(),
            2f64.ln(),
        ]);
    }
    let (lb, ub) = bounds(band, t);
    for (k, v) in x0.iter_mut().enumerate() {
        *v = v.clamp(lb[k] + 1e-6, ub[k] - 1e-6);
    }
    let norm = pairwise_sum(
        &target
            .iter()
            .map(|&h| response::magnitude(h))
            .collect::<Vec<_>>(),
    ) / 160.0
        + 1e-12;
    let residual = |x: &[f64]| {
        if ctx.cancel().is_cancelled() {
            return vec![0.0; 320];
        }
        let h = unpack(x, &[polarity], t).remove(0).response(&f);
        let r: Vec<_> = h
            .iter()
            .zip(&target)
            .map(|(&h, &v)| (h - v) / norm)
            .collect();
        r.iter()
            .map(|v| v.re)
            .chain(r.iter().map(|v| v.im))
            .collect::<Vec<_>>()
    };
    let result = optimize::least_squares(residual, &x0, &lb, &ub, 300);
    check(ctx)?;
    Ok(match result {
        Ok(sol) => SeedResult {
            x: sol.x,
            polarity,
            cost: sol.cost,
            nfev: sol.nfev,
            status: sol.status,
        },
        Err(_) => SeedResult {
            cost: decay::energy(&residual(&x0)) * 0.5,
            x: x0,
            polarity,
            nfev: 0,
            status: -1,
        },
    })
}

#[derive(Clone)]
pub(crate) struct Problem {
    pub roles: Vec<String>,
    pub f: Vec<f64>,
    pub lp: Vec<C>,
    pub n: usize,
    pub nf: usize,
    pub xs: Vec<Vec<f64>>,
    pub wl: Vec<Vec<f64>>,
    pub we: Vec<Vec<f64>>,
    pub g: Vec<Vec<Vec<C>>>,
    pub fir: Vec<Vec<f64>>,
    pub ranges: Vec<(f64, f64)>,
    pub modes: Vec<f64>,
    pub fsiso: f64,
    pub direct: f64,
    pub level_db: f64,
}

struct Parts {
    s: Vec<C>,
    d: Vec<Vec<C>>,
}

fn parts(v: &[f64], pol: i8, f: &[f64], t: &Tuning) -> Parts {
    let support = unpack(v, &[pol], t).remove(0);
    let s = support.response(f);
    let mut d = vec![vec![C::new(0.0, 0.0); f.len()]; per(t)];
    for (k, &f) in f.iter().enumerate() {
        d[0][k] = s[k] * 10f64.ln() / 20.0;
        d[1][k] = s[k] * C::new(0.0, -2.0 * PI * f / 1000.0);
    }
    let h = 1e-4;
    for k in 0..t.mimo_k_bands {
        let base = 2 + 3 * k;
        let factor =
            response::peaking_response(f, v[base].exp(), v[base + 1].exp(), v[base + 2], 48000.0);
        for c in 0..3 {
            let mut up = [v[base], v[base + 1], v[base + 2]];
            let mut dn = up;
            up[c] += h;
            dn[c] -= h;
            let u = response::peaking_response(f, up[0].exp(), up[1].exp(), up[2], 48000.0);
            let l = response::peaking_response(f, dn[0].exp(), dn[1].exp(), dn[2], 48000.0);
            for i in 0..f.len() {
                d[base + c][i] = complex_div(s[i] * (u[i] - l[i]) / (2.0 * h), factor[i]);
            }
        }
    }
    for a in 0..t.mimo_a_bands {
        let base = 2 + 3 * t.mimo_k_bands + 2 * a;
        let factor = response::allpass2(f, v[base].exp(), v[base + 1].exp());
        for c in 0..2 {
            let mut up = [v[base], v[base + 1]];
            let mut dn = up;
            up[c] += h;
            dn[c] -= h;
            let u = response::allpass2(f, up[0].exp(), up[1].exp());
            let l = response::allpass2(f, dn[0].exp(), dn[1].exp());
            for i in 0..f.len() {
                d[base + c][i] = complex_div(s[i] * (u[i] - l[i]) / (2.0 * h), factor[i]);
            }
        }
    }
    Parts { s, d }
}

fn time_product(a: &[C], b: &[C], nf: usize, n: usize) -> Vec<f64> {
    let mut out = fft::irfft(&a.iter().zip(b).map(|(a, b)| a * b).collect::<Vec<_>>(), nf);
    out.truncate(n);
    out
}

pub(crate) struct Objective<'a> {
    pb: &'a Problem,
    t: &'a Tuning,
    ctx: &'a Context,
    pols: &'a [i8],
    weff: Vec<Vec<f64>>,
    scale: f64,
    key: Vec<u64>,
    cache: Vec<Parts>,
}

impl<'a> Objective<'a> {
    pub(crate) fn new(pb: &'a Problem, t: &'a Tuning, ctx: &'a Context, pols: &'a [i8]) -> Self {
        let band: Vec<_> =
            pb.f.iter()
                .map(|&f| f >= 15.0 && f <= pb.fsiso * 1.2)
                .collect();
        let count = band.iter().filter(|&&b| b).count().max(1);
        let energy = decay::energy(&pb.xs[0]);
        let weff = (0..pb.roles.len())
            .map(|r| {
                let mut gbar = vec![0.0; pb.f.len()];
                for g in &pb.g {
                    for (sum, &h) in gbar.iter_mut().zip(&g[r]) {
                        *sum += response::magnitude(h);
                    }
                }
                for v in &mut gbar {
                    *v /= pb.xs.len() as f64;
                }
                let reference = decay::median(
                    &gbar
                        .iter()
                        .zip(&band)
                        .filter(|(_, b)| **b)
                        .map(|(&v, _)| v)
                        .collect::<Vec<_>>(),
                );
                let factor = t.mimo_effort * (energy / count as f64).sqrt();
                gbar.iter()
                    .zip(&band)
                    .map(|(&g, &b)| {
                        factor
                            * if b {
                                reference / (g + 0.05 * reference)
                            } else {
                                0.0
                            }
                    })
                    .collect()
            })
            .collect();
        Self {
            pb,
            t,
            ctx,
            pols,
            weff,
            scale: energy.sqrt(),
            key: Vec::new(),
            cache: Vec::new(),
        }
    }

    fn evaluate(&mut self, x: &[f64]) {
        let key: Vec<_> = x.iter().map(|v| v.to_bits()).collect();
        if key != self.key {
            self.cache = x
                .chunks_exact(per(self.t))
                .zip(self.pols)
                .map(|(v, &pol)| parts(v, pol, &self.pb.f, self.t))
                .collect();
            self.key = key;
        }
    }

    fn level(&self) -> (Vec<Vec<f64>>, f64, f64) {
        let st: Vec<_> = self
            .cache
            .iter()
            .map(|p| time_product(&p.s, &self.pb.lp, self.pb.nf, self.pb.n))
            .collect();
        let lvl = st.iter().map(|s| decay::energy(s)).sum::<f64>() / self.pb.direct.max(1e-30);
        let over = 10.0 * (lvl + 1e-30).log10() - self.pb.level_db;
        (st, lvl, over)
    }

    fn rows(&self) -> usize {
        2 * self.pb.xs.len() * self.pb.n
            + 1
            + 2 * self.pb.roles.len() * self.pb.f.len()
            + if self.t.mimo_band_reg > 0.0 {
                self.pb.roles.len() * self.t.mimo_k_bands
            } else {
                0
            }
    }
}

pub(crate) fn solve_joint(
    pb: &Problem,
    t: &Tuning,
    ctx: &Context,
    x0: &[f64],
    pols: &[i8],
) -> Result<optimize::LeastSquaresResult, EngineError> {
    check(ctx)?;
    let mut lb = Vec::new();
    let mut ub = Vec::new();
    for &(lo, hi) in &pb.ranges {
        let (l, u) = bounds(band_of(lo, hi, pb.fsiso, t), t);
        lb.extend(l);
        ub.extend(u);
    }
    let x0: Vec<_> = x0
        .iter()
        .enumerate()
        .map(|(k, &v)| v.clamp(lb[k] + 1e-6, ub[k] - 1e-6))
        .collect();
    let mut objective = Objective::new(pb, t, ctx, pols);
    for x_scale in [XScale::Jac, XScale::Unit] {
        let result = optimize::least_squares_normal(
            &mut objective,
            &x0,
            &lb,
            &ub,
            &NormalOptions {
                max_nfev: t.mimo_max_nfev,
                x_scale,
                ..NormalOptions::default()
            },
        );
        check(ctx)?;
        if let Ok(sol) = result {
            return Ok(sol);
        }
    }
    let cost = 0.5 * decay::energy(&objective.residual(&x0));
    Ok(optimize::LeastSquaresResult {
        x: x0,
        cost,
        nfev: 0,
        njev: 0,
        status: -1,
        optimality: f64::NAN,
    })
}

pub(crate) fn solve_problem(
    pb: &Problem,
    t: &Tuning,
    ctx: &Context,
) -> Result<(Vec<f64>, Vec<i8>), EngineError> {
    let mut starts = Vec::new();
    let mut pols = Vec::new();
    for (r, &(lo, hi)) in pb.ranges.iter().enumerate() {
        let modes: Vec<_> = pb
            .modes
            .iter()
            .copied()
            .filter(|m| (lo..=hi).contains(m))
            .collect();
        let modes = if modes.is_empty() { &pb.modes } else { &modes };
        let seed = fit_one(
            &pb.fir[r],
            lo,
            hi.min(pb.fsiso),
            modes,
            band_of(lo, hi, pb.fsiso, t),
            t,
            ctx,
        )?;
        starts.extend(seed.x);
        pols.push(seed.polarity);
    }
    Ok((solve_joint(pb, t, ctx, &starts, &pols)?.x, pols))
}

pub(crate) fn applied(pb: &Problem, supports: &[(usize, IirSupport)]) -> Vec<Vec<f64>> {
    let ss: Vec<_> = supports.iter().map(|(_, s)| s.response(&pb.f)).collect();
    pb.xs
        .iter()
        .enumerate()
        .map(|(p, x)| {
            let mut sum = vec![C::new(0.0, 0.0); pb.f.len()];
            for ((r, _), s) in supports.iter().zip(&ss) {
                for (k, v) in sum.iter_mut().enumerate() {
                    *v += pb.g[p][*r][k] * s[k];
                }
            }
            let add = fft::irfft(&sum, pb.nf);
            x.iter().zip(add).map(|(x, a)| x + a).collect()
        })
        .collect()
}

pub(crate) fn worst_band_db(pb: &Problem, supports: &[(usize, IirSupport)]) -> f64 {
    let stop = (pb.fsiso / 20.0).log2() + 1e-9;
    let edges: Vec<_> = (0..)
        .map(|k| k as f64 / 3.0)
        .take_while(|&v| v < stop)
        .map(|v| 20.0 * 2f64.powf(v))
        .collect();
    let mut bands: Vec<_> = edges.windows(2).map(|v| (v[0], v[1])).collect();
    bands.extend(
        late_modes(&pb.xs, 6, 20.0, 160.0)
            .iter()
            .map(|f| (f - 1.5, f + 1.5)),
    );
    let mut before = vec![0.0; bands.len()];
    let mut after = before.clone();
    let ys = applied(pb, supports);
    for (p, y) in ys.iter().enumerate() {
        for (acc, y) in [(&mut before, &pb.xs[p]), (&mut after, y)] {
            let mut windowed: Vec<_> = y.iter().zip(&pb.wl[p]).map(|(y, w)| y * w).collect();
            windowed.resize(pb.nf, 0.0);
            let power: Vec<_> = fft::rfft(&windowed)
                .iter()
                .map(|&h| {
                    let a = response::magnitude(h);
                    a * a
                })
                .collect();
            for (total, &(lo, hi)) in acc.iter_mut().zip(&bands) {
                let a = pb.f.partition_point(|&f| f < lo);
                let b = pb.f.partition_point(|&f| f < hi);
                *total += pairwise_sum(&power[a..b]);
            }
        }
    }
    before
        .iter()
        .zip(after)
        .filter(|(b, _)| **b > 0.0)
        .map(|(b, a)| 10.0 * ((a + 1e-30) / b).log10())
        .reduce(f64::max)
        .unwrap_or(-99.0)
}

pub(crate) fn outcome(
    pb: &Problem,
    supports: &[(usize, IirSupport)],
) -> Result<(f64, f64), EngineError> {
    Ok((
        if supports.is_empty() {
            -99.0
        } else {
            worst_band_db(pb, supports)
        },
        worst_t60_change(&pb.xs, &applied(pb, supports), 6)?,
    ))
}

#[derive(Debug)]
pub(crate) struct PostResult {
    pub supports: Vec<(usize, IirSupport)>,
    pub removed: Vec<String>,
    pub level_cut: f64,
    pub violations: Vec<f64>,
    pub gain_steps: usize,
}

pub(crate) fn postprocess(
    pb: &Problem,
    x: &[f64],
    pols: &[i8],
    t: &Tuning,
    ctx: &Context,
) -> Result<PostResult, EngineError> {
    let mut sups = unpack(x, pols, t);
    let lvl = sups
        .iter()
        .map(|s| decay::energy(&time_product(&s.response(&pb.f), &pb.lp, pb.nf, pb.n)))
        .sum::<f64>()
        / pb.direct.max(1e-30);
    let target = 10f64.powf(pb.level_db / 10.0);
    let level_cut = if lvl > target {
        10.0 * (lvl / target).log10()
    } else {
        0.0
    };
    for s in &mut sups {
        s.gain_db -= level_cut;
    }
    let mut cur: Vec<_> = sups.into_iter().enumerate().collect();
    let mut violations = Vec::new();
    let mut violation = |s: &[(usize, IirSupport)]| -> Result<f64, EngineError> {
        check(ctx)?;
        let (band, tail) = outcome(pb, s)?;
        let v = (band - t.mimo_worse_db).max(tail - 0.15 * t.mimo_worse_db);
        violations.push(v);
        Ok(v)
    };
    let mut v = violation(&cur)?;
    let mut removed = Vec::new();
    while v > 0.0 && removed.len() < (pb.roles.len() / 2).max(1) && cur.len() > 1 {
        let mut best = (0, f64::INFINITY);
        for index in 0..cur.len() {
            let trial: Vec<_> = cur
                .iter()
                .enumerate()
                .filter(|(k, _)| *k != index)
                .map(|(_, s)| s.clone())
                .collect();
            let value = violation(&trial)?;
            if value < best.1 {
                best = (index, value);
            }
        }
        if best.1 >= v {
            break;
        }
        removed.push(pb.roles[cur.remove(best.0).0].clone());
        v = best.1;
    }
    let mut gain_steps = 0;
    for step in 0..5 {
        if v <= 0.0 {
            break;
        }
        if step == 4 {
            cur.clear();
            break;
        }
        for (_, s) in &mut cur {
            s.gain_db -= 3.0;
        }
        gain_steps += 1;
        v = violation(&cur)?;
    }
    Ok(PostResult {
        supports: cur,
        removed,
        level_cut,
        violations,
        gain_steps,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_problem(
    design: &Design,
    spectra: &crate::Spectra,
    align_delay_ms: &[(String, f64)],
    ideal: &MimoDesign,
    input: &str,
    supports: &[(String, Vec<f64>)],
    req: &MimoRequest,
    t: &Tuning,
    ctx: &Context,
) -> Result<Problem, EngineError> {
    let f = secs_dsp::grid::freqs(2 * N, FS)?;
    let lp = response::lr4(&f, req.fsiso, "LP");
    let roles: Vec<_> = supports.iter().map(|(r, _)| r.clone()).collect();
    let mut pb = Problem {
        roles,
        f,
        lp,
        n: N,
        nf: 2 * N,
        xs: Vec::new(),
        wl: Vec::new(),
        we: Vec::new(),
        g: Vec::new(),
        fir: supports.iter().map(|(_, s)| s.clone()).collect(),
        ranges: Vec::new(),
        modes: Vec::new(),
        fsiso: req.fsiso,
        direct: 0.0,
        level_db: req.level_db,
    };
    for role in &pb.roles {
        pb.ranges.push(
            ideal
                .ranges
                .iter()
                .find(|(r, _)| r == role)
                .and_then(|(_, v)| *v)
                .ok_or_else(|| EngineError::Input(format!("missing MIMO range {role}")))?,
        );
    }
    for &p in &design.spectra.positions {
        check(ctx)?;
        let h = crate::input_response(design, input, spectra, p)
            .ok_or_else(|| EngineError::Input(format!("missing MIMO input {p}/{input}")))?;
        let x = secs_dsp::lowrate::to_low(&h, design_freqs(), N)?;
        let onset = onset(&x) + 36;
        pb.wl
            .push((0..N).map(|i| if i >= onset { 1.0 } else { 0.0 }).collect());
        pb.we
            .push(pb.wl.last().unwrap().iter().map(|w| 1.0 - w).collect());
        pb.xs.push(x);
        let mut paths = Vec::new();
        for role in &pb.roles {
            let h = support_path(ideal, role, spectra, p, align_delay_ms)?;
            let mut low = secs_dsp::lowrate::to_low(&h, design_freqs(), N)?;
            low.resize(2 * N, 0.0);
            paths.push(
                fft::rfft(&low)
                    .iter()
                    .zip(&pb.lp)
                    .map(|(h, l)| h * l)
                    .collect(),
            );
        }
        pb.g.push(paths);
    }
    let main = design
        .mains
        .iter()
        .find(|m| m.role == input)
        .ok_or_else(|| EngineError::Input(format!("missing MIMO main {input}")))?;
    pb.direct = decay::energy(&secs_dsp::lowrate::to_low(
        &main.response,
        design_freqs(),
        N,
    )?);
    pb.modes = late_modes(&pb.xs, t.mimo_a_bands, 20.0, req.fsiso);
    Ok(pb)
}

fn validate_request(req: &MimoRequest, t: &Tuning) -> Result<(), EngineError> {
    if !(0.0..300.0).contains(&req.fsiso)
        || req.fsiso == 0.0
        || !req.level_db.is_finite()
        || !t.mimo_band_db.is_finite()
        || t.mimo_band_db <= 0.0
        || !t.mimo_q_max.is_finite()
        || t.mimo_q_max <= 0.5
        || !t.mimo_gain_max_db.is_finite()
        || t.mimo_gain_max_db <= -80.0
        || t.mimo_max_nfev == 0
        || [t.mimo_band_reg, t.mimo_effort, t.mimo_worse_db]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err(EngineError::Input("invalid MIMO IIR tuning".into()));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn design_mimo_iir(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    ideal: &MimoDesign,
    req: &MimoRequest,
    t: &Tuning,
    ctx: &Context,
) -> Result<MimoIir, EngineError> {
    check(ctx)?;
    validate_request(req, t)?;
    if ideal.supports.iter().all(|(_, s)| s.is_empty()) {
        let _progress = ctx.stage("engine.mimo_iir", 0);
        return Ok(MimoIir::default());
    }
    let prepared = super::prepared::Prepared::new(design, measurements, config)?;
    design_mimo_iir_prepared(design, &prepared, ideal, req, t, ctx)
}

pub(crate) fn design_mimo_iir_prepared(
    design: &Design,
    prepared: &super::prepared::Prepared,
    ideal: &MimoDesign,
    req: &MimoRequest,
    t: &Tuning,
    ctx: &Context,
) -> Result<MimoIir, EngineError> {
    check(ctx)?;
    validate_request(req, t)?;
    let inputs: Vec<_> = ideal
        .supports
        .iter()
        .filter(|(_, s)| !s.is_empty())
        .collect();
    let progress = ctx.stage("engine.mimo_iir", inputs.len() as u64);
    if inputs.is_empty() {
        return Ok(MimoIir::default());
    }
    let spectra = &prepared.spectra;
    let positions = &design.spectra.positions;
    let results = ctx
        .try_map(inputs, |(input, supports)| {
            let r = supports.len();
            let dim = r * per(t);
            // Problem X/WL/WE/G, derivative cache, one position's time columns,
            // weighted columns, normal/eigen/TRF workspaces, seed Jacobian and
            // design-grid response temporaries. Reserve before per-input allocation.
            let bytes = 8
                * (positions.len() * N * (3 + 4 * r)
                    + 4 * dim * (N + 1)
                    + 4 * N * dim
                    + 12 * dim * dim
                    + 320 * per(t) * 4
                    + 32 * design_freqs().len()
                    + 32768);
            let _permit =
                ctx.budget()
                    .acquire(bytes as u64, ctx.cancel())
                    .map_err(|e| match e {
                        MemoryError::Cancelled => EngineError::Cancelled,
                        other => EngineError::Input(other.to_string()),
                    })?;
            let pb = build_problem(
                design,
                spectra,
                &prepared.align_delay_ms,
                ideal,
                input,
                supports,
                req,
                t,
                ctx,
            )?;
            let (x, pols) = solve_problem(&pb, t, ctx)?;
            let post = postprocess(&pb, &x, &pols, t, ctx)?;
            let paths = post
                .supports
                .into_iter()
                .map(|(r, s)| (pb.roles[r].clone(), s))
                .collect();
            progress.advance(1);
            Ok((input.clone(), paths, post.removed))
        })
        .map_err(|e| match e {
            MapError::Cancelled => EngineError::Cancelled,
            MapError::Item { error, .. } => error,
        })?;
    let mut out = MimoIir::default();
    for (input, paths, removed) in results {
        out.supports.push((input.clone(), paths));
        out.removed.push((input, removed));
    }
    Ok(out)
}

impl NormalEquations for Objective<'_> {
    fn residual(&mut self, x: &[f64]) -> Vec<f64> {
        if self.ctx.cancel().is_cancelled() {
            return vec![0.0; self.rows()];
        }
        self.evaluate(x);
        let pb = self.pb;
        let mut out = Vec::with_capacity(self.rows());
        for p in 0..pb.xs.len() {
            let mut sum = vec![C::new(0.0, 0.0); pb.f.len()];
            for (r, part) in self.cache.iter().enumerate() {
                for (k, v) in sum.iter_mut().enumerate() {
                    *v += pb.g[p][r][k] * part.s[k];
                }
            }
            let add = fft::irfft(&sum, pb.nf);
            out.extend((0..pb.n).map(|i| pb.wl[p][i] * (pb.xs[p][i] + add[i])));
            out.extend((0..pb.n).map(|i| MU.sqrt() * pb.we[p][i] * add[i]));
        }
        out.push(self.level().2.max(0.0) * self.scale);
        for (r, part) in self.cache.iter().enumerate() {
            out.extend(part.s.iter().zip(&self.weff[r]).map(|(h, w)| w * h.re));
            out.extend(part.s.iter().zip(&self.weff[r]).map(|(h, w)| w * h.im));
        }
        if self.t.mimo_band_reg > 0.0 {
            for r in 0..pb.roles.len() {
                for k in 0..self.t.mimo_k_bands {
                    out.push(
                        self.t.mimo_band_reg * self.scale * x[r * per(self.t) + 4 + 3 * k] / 30.0,
                    );
                }
            }
        }
        out
    }

    fn normal(&mut self, x: &[f64], f: &[f64]) -> Result<(Mat<f64>, Vec<f64>), DspError> {
        let dim = x.len();
        if self.ctx.cancel().is_cancelled() {
            return Ok((Mat::zeros(dim, dim), vec![0.0; dim]));
        }
        self.evaluate(x);
        let pb = self.pb;
        let width = per(self.t);
        let mut h = Mat::zeros(dim, dim);
        let mut grad = vec![0.0; dim];
        // One position's unweighted time columns, not the stacked residual Jacobian.
        // Each position is accumulated in input order; faer never spawns threads.
        for p in 0..pb.xs.len() {
            let mut c = Mat::zeros(pb.n, dim);
            for (r, part) in self.cache.iter().enumerate() {
                for (a, d) in part.d.iter().enumerate() {
                    let column = time_product(&pb.g[p][r], d, pb.nf, pb.n);
                    for (i, &v) in column.iter().enumerate() {
                        c[(i, r * width + a)] = v;
                    }
                }
            }
            let weighted = Mat::from_fn(pb.n, dim, |i, j| {
                c[(i, j)] * (pb.wl[p][i] * pb.wl[p][i] + MU * pb.we[p][i] * pb.we[p][i])
            });
            matmul(
                h.as_mut(),
                Accum::Add,
                c.transpose(),
                &weighted,
                1.0,
                Par::Seq,
            );
            for j in 0..dim {
                for i in 0..pb.n {
                    grad[j] += c[(i, j)]
                        * (pb.wl[p][i] * f[2 * p * pb.n + i]
                            + MU.sqrt() * pb.we[p][i] * f[(2 * p + 1) * pb.n + i]);
                }
            }
        }
        let (st, lvl, over) = self.level();
        let mut level = vec![0.0; dim];
        for (r, part) in self.cache.iter().enumerate() {
            let effort = Mat::from_fn(2 * pb.f.len(), width, |i, j| {
                let k = i % pb.f.len();
                self.weff[r][k]
                    * if i < pb.f.len() {
                        part.d[j][k].re
                    } else {
                        part.d[j][k].im
                    }
            });
            let e0 = 2 * pb.xs.len() * pb.n + 1 + 2 * r * pb.f.len();
            let mut block = Mat::zeros(width, width);
            matmul(
                &mut block,
                Accum::Replace,
                effort.transpose(),
                &effort,
                1.0,
                Par::Seq,
            );
            for a in 0..width {
                for b in 0..width {
                    h[(r * width + a, r * width + b)] += block[(a, b)];
                }
                for i in 0..2 * pb.f.len() {
                    grad[r * width + a] += effort[(i, a)] * f[e0 + i];
                }
                if over > 0.0 {
                    let ct = time_product(&part.d[a], &pb.lp, pb.nf, pb.n);
                    let dot = ct.iter().zip(&st[r]).map(|(a, b)| a * b).sum::<f64>();
                    level[r * width + a] = self.scale * 10.0 / (10f64.ln() * lvl.max(1e-30))
                        * (2.0 * dot / pb.direct.max(1e-30));
                }
            }
        }
        if over > 0.0 {
            for a in 0..dim {
                grad[a] += level[a] * f[2 * pb.xs.len() * pb.n];
                for b in 0..dim {
                    h[(a, b)] += level[a] * level[b];
                }
            }
        }
        if self.t.mimo_band_reg > 0.0 {
            let factor = self.t.mimo_band_reg * self.scale / 30.0;
            let row = 2 * pb.xs.len() * pb.n + 1 + 2 * pb.roles.len() * pb.f.len();
            for r in 0..pb.roles.len() {
                for k in 0..self.t.mimo_k_bands {
                    let a = r * width + 4 + 3 * k;
                    h[(a, a)] += factor * factor;
                    grad[a] += factor * f[row + r * self.t.mimo_k_bands + k];
                }
            }
        }
        Ok((h, grad))
    }
}
