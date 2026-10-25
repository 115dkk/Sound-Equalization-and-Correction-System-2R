//! Port of SECS 2 `src/secs/mimo_iir.py` (GPL-3.0).
//! Default joint TRF accumulates position blocks; the opt-in solver applies
//! Jv/Jᵀv directly. Neither stacks a dense residual Jacobian (ADR 0007).

use std::{cell::RefCell, f64::consts::PI};

use faer::{Accum, Mat, Par, linalg::matmul::matmul};
use secs_compute::Context;
use secs_dsp::{
    DspError, fft, optimize,
    optimize::{JacobianOperator, NormalEquations, NormalOptions, OperatorOptions, XScale},
    pairwise_sum, response,
};

use super::{MimoDesign, MimoRequest, design::third_octave_bands, support_path};
use crate::{
    C, Design, EngineConfig, EngineError, MeasurementSet, Note, Tuning, decay,
    fit::{FitRequest, fit_peq_until},
    grid::design_freqs,
};

const FS: f64 = 600.0;
/// Residuals of one support's seed fit: 160 frequencies, real and imaginary.
const SEED_RESIDUALS: usize = 320;
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
        self.response_z(f, &frequency_z(f))
    }

    fn response_z(&self, f: &[f64], z: &[C]) -> Vec<C> {
        let gain = 10f64.powf(self.gain_db / 20.0) * f64::from(self.polarity);
        let mut out: Vec<_> = f
            .iter()
            .map(|&f| C::from_polar(1.0, -2.0 * PI * f * self.delay_ms / 1000.0) * gain)
            .collect();
        for &(f0, q, g) in &self.peq {
            for (h, p) in out
                .iter_mut()
                .zip(response::peaking_response_z(z, f0, q, g, 48000.0))
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

fn frequency_z(f: &[f64]) -> Vec<C> {
    f.iter()
        .map(|&f| C::from_polar(1.0, (-2.0 * PI * f) * (1.0 / 48000.0)))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MimoIir {
    pub supports: Vec<(String, Vec<(String, IirSupport)>)>,
    pub removed: Vec<(String, Vec<String>)>,
    pub notes: Vec<Note>,
}

/// Joint IIR optimizer. The matrix-free path is explicit because an iterative
/// trust-region subproblem follows a different numerical trajectory.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum IirSolver {
    #[default]
    Dense,
    MatrixFree {
        max_cg_iterations: usize,
        cg_rtol: f64,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct IirSolveOptions {
    pub solver: IirSolver,
    /// Retry the original dense Jac/Unit solvers after an operator error. The
    /// memory permit also includes dense workspaces when this is enabled.
    pub dense_fallback: bool,
}

impl Default for IirSolveOptions {
    fn default() -> Self {
        Self {
            solver: IirSolver::Dense,
            dense_fallback: true,
        }
    }
}

impl IirSolveOptions {
    fn validate(&self) -> Result<(), EngineError> {
        if let IirSolver::MatrixFree {
            max_cg_iterations,
            cg_rtol,
        } = self.solver
            && (max_cg_iterations == 0 || !cg_rtol.is_finite() || cg_rtol <= 0.0 || cg_rtol >= 1.0)
        {
            return Err(EngineError::Input("invalid matrix-free IIR options".into()));
        }
        Ok(())
    }
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

pub(crate) fn late_modes(xs: &[Vec<f64>], count: usize, lo: f64, hi: f64) -> Vec<f64> {
    let mut spec = vec![0.0; 2049];
    for x in xs {
        let mut late = vec![0.0; 4096];
        let begin = (decay::onset(x) + 90).min(x.len().min(4096));
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
    let z = frequency_z(&f);
    let cache = RefCell::new(Parts::default());
    let residual = |x: &[f64]| {
        if ctx.cancel().is_cancelled() {
            return vec![0.0; SEED_RESIDUALS];
        }
        let mut cache = cache.borrow_mut();
        cache.response(x, polarity, &f, &z, t);
        let r: Vec<_> = cache
            .s
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

#[derive(Default)]
struct Parts {
    key: Vec<u64>,
    s: Vec<C>,
    d: Vec<Vec<C>>,
}

impl Parts {
    fn response(&mut self, v: &[f64], pol: i8, f: &[f64], z: &[C], t: &Tuning) {
        if self.key.len() == v.len() && self.key.iter().zip(v).all(|(&a, &b)| a == b.to_bits()) {
            return;
        }
        self.s = unpack(v, &[pol], t).remove(0).response_z(f, z);
        self.key.clear();
        self.key.extend(v.iter().map(|v| v.to_bits()));
        self.d.clear();
    }

    fn derivatives(&mut self, v: &[f64], pol: i8, f: &[f64], z: &[C], t: &Tuning) {
        self.response(v, pol, f, z, t);
        if !self.d.is_empty() {
            return;
        }
        self.d = finite_difference_derivatives(v, &self.s, f, t);
    }
}

/// Upstream's central differences (step 1e-4) for the support factors. The
/// gain and delay columns are exact. Analytic factor derivatives moved the
/// joint fit outside SECS 2's spread (ADR 0012), so the port keeps these.
fn finite_difference_derivatives(v: &[f64], s: &[C], f: &[f64], t: &Tuning) -> Vec<Vec<C>> {
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
                d[base + c][i] = secs_dsp::complex_div(s[i] * (u[i] - l[i]) / (2.0 * h), factor[i]);
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
                d[base + c][i] = secs_dsp::complex_div(s[i] * (u[i] - l[i]) / (2.0 * h), factor[i]);
            }
        }
    }
    d
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
    z: Vec<C>,
    cache: Vec<Parts>,
    operator: Option<OperatorState>,
}

struct OperatorState {
    /// Per support, parameter and frequency. Retained independently of trial
    /// response caches, so a rejected residual cannot change Jv or Jᵀv.
    derivatives: Vec<Vec<Vec<C>>>,
    level: Vec<f64>,
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
            z: frequency_z(&pb.f),
            cache: (0..pb.roles.len()).map(|_| Parts::default()).collect(),
            operator: None,
        }
    }

    fn evaluate(&mut self, x: &[f64]) {
        for ((part, v), &pol) in self
            .cache
            .iter_mut()
            .zip(x.chunks_exact(per(self.t)))
            .zip(self.pols)
        {
            part.response(v, pol, &self.pb.f, &self.z, self.t);
        }
    }

    fn derivatives(&mut self, x: &[f64]) {
        for ((part, v), &pol) in self
            .cache
            .iter_mut()
            .zip(x.chunks_exact(per(self.t)))
            .zip(self.pols)
        {
            part.derivatives(v, pol, &self.pb.f, &self.z, self.t);
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
    solve_joint_with_options(pb, t, ctx, x0, pols, &IirSolveOptions::default())
}

fn solve_joint_with_options(
    pb: &Problem,
    t: &Tuning,
    ctx: &Context,
    x0: &[f64],
    pols: &[i8],
    options: &IirSolveOptions,
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
    if let IirSolver::MatrixFree {
        max_cg_iterations,
        cg_rtol,
    } = options.solver
    {
        let result = optimize::least_squares_operator(
            &mut objective,
            &x0,
            &lb,
            &ub,
            &OperatorOptions {
                max_nfev: t.mimo_max_nfev,
                max_cg_iterations,
                cg_rtol,
                ..OperatorOptions::default()
            },
        );
        check(ctx)?;
        match result {
            Ok(solution) => return Ok(solution),
            Err(error) if !options.dense_fallback => return Err(error.into()),
            Err(_) => objective.operator = None,
        }
    }
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
    let cost = 0.5 * decay::energy(&NormalEquations::residual(&mut objective, &x0));
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
    solve_problem_with_options(pb, t, ctx, &IirSolveOptions::default())
}

fn solve_problem_with_options(
    pb: &Problem,
    t: &Tuning,
    ctx: &Context,
    options: &IirSolveOptions,
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
    Ok((
        solve_joint_with_options(pb, t, ctx, &starts, &pols, options)?.x,
        pols,
    ))
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

struct BandGuard {
    bounds: Vec<(usize, usize)>,
    before: Vec<f64>,
}

impl BandGuard {
    fn new(pb: &Problem) -> Self {
        let mut bands = third_octave_bands(pb.fsiso);
        bands.extend(
            late_modes(&pb.xs, 6, 20.0, 160.0)
                .iter()
                .map(|f| (f - 1.5, f + 1.5)),
        );
        let bounds = bands
            .into_iter()
            .map(|(lo, hi)| {
                (
                    pb.f.partition_point(|&f| f < lo),
                    pb.f.partition_point(|&f| f < hi),
                )
            })
            .collect::<Vec<_>>();
        let before = Self::power(pb, &pb.xs, &bounds);
        Self { bounds, before }
    }

    fn power(pb: &Problem, ys: &[Vec<f64>], bounds: &[(usize, usize)]) -> Vec<f64> {
        let mut total = vec![0.0; bounds.len()];
        for (p, y) in ys.iter().enumerate() {
            let mut windowed: Vec<_> = y.iter().zip(&pb.wl[p]).map(|(y, w)| y * w).collect();
            windowed.resize(pb.nf, 0.0);
            let power: Vec<_> = fft::rfft(&windowed)
                .iter()
                .map(|&h| {
                    let a = response::magnitude(h);
                    a * a
                })
                .collect();
            for (total, &(a, b)) in total.iter_mut().zip(bounds) {
                *total += pairwise_sum(&power[a..b]);
            }
        }
        total
    }

    fn evaluate(&self, pb: &Problem, ys: &[Vec<f64>]) -> f64 {
        self.before
            .iter()
            .zip(Self::power(pb, ys, &self.bounds))
            .filter(|(b, _)| **b > 0.0)
            .map(|(b, a)| 10.0 * ((a + 1e-30) / b).log10())
            .reduce(f64::max)
            .unwrap_or(-99.0)
    }
}

pub(crate) fn worst_band_db(pb: &Problem, supports: &[(usize, IirSupport)]) -> f64 {
    BandGuard::new(pb).evaluate(pb, &applied(pb, supports))
}

struct OutcomeGuard<'a> {
    pb: &'a Problem,
    band: BandGuard,
    tail: super::design::T60Guard<'a>,
}

impl<'a> OutcomeGuard<'a> {
    fn new(pb: &'a Problem) -> Self {
        Self {
            pb,
            band: BandGuard::new(pb),
            tail: super::design::T60Guard::new(&pb.xs, 6),
        }
    }

    fn evaluate(&mut self, supports: &[(usize, IirSupport)]) -> Result<(f64, f64), EngineError> {
        // Both guards consume the same candidate waveform, while baseline
        // windows, bands, modal filters and original tails survive all trials.
        let ys = applied(self.pb, supports);
        Ok((
            if supports.is_empty() {
                -99.0
            } else {
                self.band.evaluate(self.pb, &ys)
            },
            self.tail.evaluate(&ys)?,
        ))
    }
}

pub(crate) fn outcome(
    pb: &Problem,
    supports: &[(usize, IirSupport)],
) -> Result<(f64, f64), EngineError> {
    OutcomeGuard::new(pb).evaluate(supports)
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
    let mut guard = OutcomeGuard::new(pb);
    let mut violation = |s: &[(usize, IirSupport)]| -> Result<f64, EngineError> {
        check(ctx)?;
        let (band, tail) = guard.evaluate(s)?;
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
        let onset = decay::onset(&x) + 36;
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
    // Each support's seed fit solves `per(t)` = 2 + 3·K + 2·A parameters from
    // its SEED_RESIDUALS residuals; more unknowns than that is not a fit the
    // design supports, and the count must not overflow (TD-104).
    let parameters = t
        .mimo_k_bands
        .checked_mul(3)
        .zip(t.mimo_a_bands.checked_mul(2))
        .and_then(|(k, a)| k.checked_add(a)?.checked_add(2));
    if parameters.is_none_or(|n| n > SEED_RESIDUALS) {
        return Err(EngineError::Input(format!(
            "invalid MIMO IIR tuning: {} peaking and {} all-pass bands exceed {SEED_RESIDUALS} parameters per support",
            t.mimo_k_bands, t.mimo_a_bands
        )));
    }
    // The seed and joint fits clamp their start into [lb + 1e-6, ub - 1e-6],
    // which needs every interval wider than 2e-6. The tuning sets the gain, Q
    // and band-gain intervals; the frequency ones are at least ln 1.5 wide
    // for any band `band_of` returns, so one representative band suffices.
    // Every bound is finite after the checks above.
    let (lb, ub) = bounds((15.0, 300.0), t);
    if lb
        .iter()
        .zip(&ub)
        .any(|(&l, &u)| u - l <= 2e-6 || l + 1e-6 > u - 1e-6)
    {
        return Err(EngineError::Input(
            "invalid MIMO IIR tuning: a parameter range is not wider than 2e-6".into(),
        ));
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
    design_mimo_iir_with_options(
        design,
        measurements,
        config,
        ideal,
        req,
        t,
        &IirSolveOptions::default(),
        ctx,
    )
}

/// Designs IIR supports with an explicit joint-solver choice. Existing callers
/// use `design_mimo_iir`, which retains the dense solver. Setting
/// `IirSolver::MatrixFree { max_cg_iterations: 64, cg_rtol: 1e-3 }` enables the
/// f64 operator solver without changing project files or IPC tuning values.
#[allow(clippy::too_many_arguments)]
pub fn design_mimo_iir_with_options(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    ideal: &MimoDesign,
    req: &MimoRequest,
    t: &Tuning,
    options: &IirSolveOptions,
    ctx: &Context,
) -> Result<MimoIir, EngineError> {
    check(ctx)?;
    validate_request(req, t)?;
    options.validate()?;
    if ideal.supports.iter().all(|(_, s)| s.is_empty()) {
        let _progress = ctx.stage("engine.mimo_iir", 0);
        return Ok(MimoIir::default());
    }
    let prepared = super::prepared::Prepared::new(design, measurements, config, ctx)?;
    design_mimo_iir_prepared_with_options(design, &prepared, ideal, req, t, options, ctx)
}

pub(crate) fn design_mimo_iir_prepared(
    design: &Design,
    prepared: &super::prepared::Prepared<'_>,
    ideal: &MimoDesign,
    req: &MimoRequest,
    t: &Tuning,
    ctx: &Context,
) -> Result<MimoIir, EngineError> {
    design_mimo_iir_prepared_with_options(
        design,
        prepared,
        ideal,
        req,
        t,
        &IirSolveOptions::default(),
        ctx,
    )
}

#[allow(clippy::too_many_arguments)]
fn design_mimo_iir_prepared_with_options(
    design: &Design,
    prepared: &super::prepared::Prepared<'_>,
    ideal: &MimoDesign,
    req: &MimoRequest,
    t: &Tuning,
    options: &IirSolveOptions,
    ctx: &Context,
) -> Result<MimoIir, EngineError> {
    check(ctx)?;
    validate_request(req, t)?;
    options.validate()?;
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
    let results = crate::memory::try_map(
        ctx,
        inputs,
        prepared.reserved,
        |(_, supports)| {
            let r = supports.len();
            let dim = r * per(t);
            let rows = 2 * positions.len() * N + 1 + 2 * r * (N + 1) + r * t.mimo_k_bands;
            let solver_bytes = if options.solver == IirSolver::Dense || options.dense_fallback {
                4 * N * dim + 12 * dim * dim
            } else {
                8 * rows + 24 * dim + 8 * r * (N + 1)
            };
            // Problem X/WL/WE/G, derivative cache, one position's time columns,
            // weighted columns, normal/eigen/TRF workspaces, seed Jacobian and
            // design-grid response temporaries. Reserve before per-input allocation.
            let bytes = 8
                * (positions.len() * N * (3 + 4 * r)
                    + 4 * dim * (N + 1)
                    + solver_bytes
                    + 320 * per(t) * 4
                    + 32 * design_freqs().len()
                    + 32768);
            bytes as u64
        },
        |(input, supports)| {
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
            let (x, pols) = solve_problem_with_options(&pb, t, ctx, options)?;
            let post = postprocess(&pb, &x, &pols, t, ctx)?;
            let paths = post
                .supports
                .into_iter()
                .map(|(r, s)| (pb.roles[r].clone(), s))
                .collect();
            progress.advance(1);
            Ok((input.clone(), paths, post.removed))
        },
    )?;
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
        self.derivatives(x);
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
                    level[r * width + a] = self.scale * 10.0 / (10f64.ln() * (lvl + 1e-30))
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

/// Adjoint of the normalized real inverse transform after truncation to n.
/// Interior bins carry both conjugate frequencies (2/nf); DC and an even
/// Nyquist bin carry one (1/nf), with imaginary endpoint derivatives ignored.
fn inverse_adjoint(time: &[f64], nf: usize) -> Vec<C> {
    let mut padded = time.to_vec();
    padded.resize(nf, 0.0);
    let mut spectrum = fft::rfft(&padded);
    for (k, h) in spectrum.iter_mut().enumerate() {
        let endpoint = k == 0 || (nf.is_multiple_of(2) && k == nf / 2);
        *h *= if endpoint {
            1.0 / nf as f64
        } else {
            2.0 / nf as f64
        };
        if endpoint {
            h.im = 0.0;
        }
    }
    spectrum
}

impl JacobianOperator for Objective<'_> {
    fn residual(&mut self, x: &[f64]) -> Vec<f64> {
        NormalEquations::residual(self, x)
    }

    fn linearize(&mut self, x: &[f64]) -> Result<Vec<f64>, DspError> {
        let width = per(self.t);
        let dim = x.len();
        if self.ctx.cancel().is_cancelled() {
            self.operator = Some(OperatorState {
                derivatives: Vec::new(),
                level: vec![0.0; dim],
            });
            return Ok(vec![0.0; dim]);
        }
        self.derivatives(x);
        let pb = self.pb;
        let mut diagonal = vec![0.0; dim];
        // Exact column norms need one time column at a time, never an n×dim
        // block and never a Gram matrix. They supply scaling/preconditioning.
        for p in 0..pb.xs.len() {
            for (r, part) in self.cache.iter().enumerate() {
                for (a, d) in part.d.iter().enumerate() {
                    let column = time_product(&pb.g[p][r], d, pb.nf, pb.n);
                    for (i, value) in column.into_iter().enumerate() {
                        diagonal[r * width + a] += value
                            * value
                            * (pb.wl[p][i] * pb.wl[p][i] + MU * pb.we[p][i] * pb.we[p][i]);
                    }
                }
            }
        }
        let (st, lvl, over) = self.level();
        let mut level = vec![0.0; dim];
        for (r, part) in self.cache.iter().enumerate() {
            for (a, derivative) in part.d.iter().enumerate() {
                let j = r * width + a;
                for component in [false, true] {
                    for (k, d) in derivative.iter().enumerate() {
                        let v = self.weff[r][k] * if component { d.im } else { d.re };
                        diagonal[j] += v * v;
                    }
                }
                if over > 0.0 {
                    let ct = time_product(derivative, &pb.lp, pb.nf, pb.n);
                    let dot = ct.iter().zip(&st[r]).map(|(a, b)| a * b).sum::<f64>();
                    level[j] = self.scale * 10.0 / (10f64.ln() * (lvl + 1e-30))
                        * (2.0 * dot / pb.direct.max(1e-30));
                    diagonal[j] += level[j] * level[j];
                }
            }
        }
        if self.t.mimo_band_reg > 0.0 {
            let factor = self.t.mimo_band_reg * self.scale / 30.0;
            for r in 0..pb.roles.len() {
                for k in 0..self.t.mimo_k_bands {
                    diagonal[r * width + 4 + 3 * k] += factor * factor;
                }
            }
        }
        // Moving derivatives into the retained base point avoids a second full
        // cache and lets every rejected trial remain response-only.
        self.operator = Some(OperatorState {
            derivatives: self
                .cache
                .iter_mut()
                .map(|part| std::mem::take(&mut part.d))
                .collect(),
            level,
        });
        Ok(diagonal)
    }

    fn jvp(&self, direction: &[f64], out: &mut [f64]) -> Result<(), DspError> {
        let pb = self.pb;
        let width = per(self.t);
        if direction.len() != width * pb.roles.len() || out.len() != self.rows() {
            return Err(DspError::InvalidArgument(
                "invalid IIR Jacobian product shape".into(),
            ));
        }
        out.fill(0.0);
        if self.ctx.cancel().is_cancelled() {
            return Ok(());
        }
        let state = self
            .operator
            .as_ref()
            .ok_or_else(|| DspError::InvalidArgument("IIR operator has no linearization".into()))?;
        let mut responses = vec![vec![C::new(0.0, 0.0); pb.f.len()]; pb.roles.len()];
        for (r, derivatives) in state.derivatives.iter().enumerate() {
            for (a, derivative) in derivatives.iter().enumerate() {
                for (k, d) in derivative.iter().enumerate() {
                    responses[r][k] += d * direction[r * width + a];
                }
            }
        }
        for p in 0..pb.xs.len() {
            let mut sum = vec![C::new(0.0, 0.0); pb.f.len()];
            for (r, response) in responses.iter().enumerate() {
                for (k, value) in sum.iter_mut().enumerate() {
                    *value += pb.g[p][r][k] * response[k];
                }
            }
            let time = fft::irfft(&sum, pb.nf);
            for i in 0..pb.n {
                out[2 * p * pb.n + i] = pb.wl[p][i] * time[i];
                out[(2 * p + 1) * pb.n + i] = MU.sqrt() * pb.we[p][i] * time[i];
            }
        }
        let level_row = 2 * pb.xs.len() * pb.n;
        out[level_row] = state.level.iter().zip(direction).map(|(g, v)| g * v).sum();
        for (r, response) in responses.iter().enumerate() {
            let base = level_row + 1 + 2 * r * pb.f.len();
            for (k, h) in response.iter().enumerate() {
                out[base + k] = self.weff[r][k] * h.re;
                out[base + pb.f.len() + k] = self.weff[r][k] * h.im;
            }
        }
        if self.t.mimo_band_reg > 0.0 {
            let base = level_row + 1 + 2 * pb.roles.len() * pb.f.len();
            let factor = self.t.mimo_band_reg * self.scale / 30.0;
            for r in 0..pb.roles.len() {
                for k in 0..self.t.mimo_k_bands {
                    out[base + r * self.t.mimo_k_bands + k] =
                        factor * direction[r * width + 4 + 3 * k];
                }
            }
        }
        Ok(())
    }

    fn vjp(&self, residual_direction: &[f64], out: &mut [f64]) -> Result<(), DspError> {
        let pb = self.pb;
        let width = per(self.t);
        if out.len() != width * pb.roles.len() || residual_direction.len() != self.rows() {
            return Err(DspError::InvalidArgument(
                "invalid IIR transpose product shape".into(),
            ));
        }
        out.fill(0.0);
        if self.ctx.cancel().is_cancelled() {
            return Ok(());
        }
        let state = self
            .operator
            .as_ref()
            .ok_or_else(|| DspError::InvalidArgument("IIR operator has no linearization".into()))?;
        let mut adjoints = vec![vec![C::new(0.0, 0.0); pb.f.len()]; pb.roles.len()];
        for p in 0..pb.xs.len() {
            let time: Vec<_> = (0..pb.n)
                .map(|i| {
                    pb.wl[p][i] * residual_direction[2 * p * pb.n + i]
                        + MU.sqrt() * pb.we[p][i] * residual_direction[(2 * p + 1) * pb.n + i]
                })
                .collect();
            let spectrum = inverse_adjoint(&time, pb.nf);
            for (r, adjoint) in adjoints.iter_mut().enumerate() {
                for (k, value) in adjoint.iter_mut().enumerate() {
                    *value += pb.g[p][r][k].conj() * spectrum[k];
                }
            }
        }
        let level_row = 2 * pb.xs.len() * pb.n;
        for (r, adjoint) in adjoints.iter_mut().enumerate() {
            let base = level_row + 1 + 2 * r * pb.f.len();
            for (k, value) in adjoint.iter_mut().enumerate() {
                *value += C::new(
                    self.weff[r][k] * residual_direction[base + k],
                    self.weff[r][k] * residual_direction[base + pb.f.len() + k],
                );
            }
            for (a, derivative) in state.derivatives[r].iter().enumerate() {
                let j = r * width + a;
                out[j] = derivative
                    .iter()
                    .zip(&*adjoint)
                    .map(|(d, a)| d.re * a.re + d.im * a.im)
                    .sum::<f64>()
                    + state.level[j] * residual_direction[level_row];
            }
        }
        if self.t.mimo_band_reg > 0.0 {
            let base = level_row + 1 + 2 * pb.roles.len() * pb.f.len();
            let factor = self.t.mimo_band_reg * self.scale / 30.0;
            for r in 0..pb.roles.len() {
                for k in 0..self.t.mimo_k_bands {
                    out[r * width + 4 + 3 * k] +=
                        factor * residual_direction[base + r * self.t.mimo_k_bands + k];
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod operator_tests {
    use super::*;

    fn fixture(level_db: f64) -> (Problem, Tuning, Vec<f64>, Vec<i8>) {
        let (n, nf) = (64, 128);
        let f = secs_dsp::grid::freqs(nf, FS).unwrap();
        let t = Tuning {
            mimo_k_bands: 2,
            mimo_a_bands: 1,
            mimo_max_nfev: 25,
            ..Tuning::default()
        };
        let lp = response::lr4(&f, 100.0, "LP");
        let xs: Vec<Vec<f64>> = (0..2)
            .map(|p| {
                (0..n)
                    .map(|i| {
                        let time = i as f64;
                        (time * 0.6 + p as f64 * 0.1).cos() * (-time / 40.0).exp()
                    })
                    .collect()
            })
            .collect();
        let wl: Vec<Vec<f64>> = (0..2)
            .map(|p| (0..n).map(|i| if i >= 8 + p { 1.0 } else { 0.0 }).collect())
            .collect();
        let we = wl
            .iter()
            .map(|w| w.iter().map(|w| 1.0 - w).collect())
            .collect();
        let g = (0..2)
            .map(|p| {
                (0..2)
                    .map(|r| {
                        let mut ir = vec![0.0; nf];
                        for (i, value) in ir.iter_mut().enumerate().take(n).skip(2 + p + r) {
                            *value = (-((i - 2 - p - r) as f64) / 12.0).exp()
                                * (i as f64 * (0.4 + r as f64 * 0.1)).cos();
                        }
                        fft::rfft(&ir).iter().zip(&lp).map(|(a, b)| a * b).collect()
                    })
                    .collect()
            })
            .collect();
        let x = (0..2)
            .flat_map(|r| {
                [
                    -6.0 - r as f64,
                    12.0 + r as f64,
                    50.0_f64.ln(),
                    2.0_f64.ln(),
                    3.0,
                    110.0_f64.ln(),
                    1.3_f64.ln(),
                    -4.0,
                    70.0_f64.ln(),
                    1.5_f64.ln(),
                ]
            })
            .collect();
        (
            Problem {
                roles: vec!["SW1".into(), "SW2".into()],
                f,
                lp,
                n,
                nf,
                xs,
                wl,
                we,
                g,
                fir: vec![vec![0.0; n]; 2],
                ranges: vec![(20.0, 150.0); 2],
                modes: vec![50.0, 80.0],
                fsiso: 100.0,
                direct: 1.0,
                level_db,
            },
            t,
            x,
            vec![1, -1],
        )
    }

    fn dot(a: &[f64], b: &[f64]) -> f64 {
        a.iter().zip(b).map(|(a, b)| a * b).sum()
    }

    #[test]
    fn real_inverse_adjoint_handles_dc_nyquist_and_truncation() {
        for nf in [63, 64] {
            let spectrum: Vec<_> = (0..nf / 2 + 1)
                .map(|k| C::new((k as f64 * 0.3).sin(), (k as f64 * 0.7).cos()))
                .collect();
            let time: Vec<_> = (0..17).map(|i| (i as f64 * 0.5).cos()).collect();
            let transformed = fft::irfft(&spectrum, nf);
            let lhs = dot(&transformed[..time.len()], &time);
            let adjoint = inverse_adjoint(&time, nf);
            let rhs: f64 = spectrum
                .iter()
                .zip(&adjoint)
                .map(|(a, b)| a.re * b.re + a.im * b.im)
                .sum();
            assert!((lhs - rhs).abs() < 1e-12 * lhs.abs().max(rhs.abs()).max(1.0));
        }
    }

    #[test]
    fn iir_operator_matches_derivatives_normal_equations_and_adjoint() {
        for level_db in [-80.0, 80.0] {
            let (pb, tuning, x, pols) = fixture(level_db);
            let ctx = Context::serial();
            let mut objective = Objective::new(&pb, &tuning, &ctx, &pols);
            let f = NormalEquations::residual(&mut objective, &x);
            assert!(
                objective.cache.iter().all(|part| part.d.is_empty()),
                "residual calculated unused derivatives"
            );
            let (normal, gradient) = objective.normal(&x, &f).unwrap();
            let diagonal = objective.linearize(&x).unwrap();
            let direction: Vec<_> = (0..x.len()).map(|i| (i as f64 * 0.4).sin()).collect();
            let row_direction: Vec<_> = (0..f.len()).map(|i| (i as f64 * 0.13).cos()).collect();
            let mut jv = vec![0.0; f.len()];
            objective.jvp(&direction, &mut jv).unwrap();
            let mut jtu = vec![0.0; x.len()];
            objective.vjp(&row_direction, &mut jtu).unwrap();
            let lhs = dot(&jv, &row_direction);
            let rhs = dot(&direction, &jtu);
            assert!((lhs - rhs).abs() < 1e-10 * lhs.abs().max(rhs.abs()).max(1.0));
            let mut jt_jv = vec![0.0; x.len()];
            objective.vjp(&jv, &mut jt_jv).unwrap();
            let mut jt_f = vec![0.0; x.len()];
            objective.vjp(&f, &mut jt_f).unwrap();
            for j in 0..x.len() {
                let expected: f64 = (0..x.len()).map(|k| normal[(j, k)] * direction[k]).sum();
                assert!((jt_jv[j] - expected).abs() < 2e-10 * expected.abs().max(1.0));
                assert!((jt_f[j] - gradient[j]).abs() < 2e-10 * gradient[j].abs().max(1.0));
                assert!(
                    (diagonal[j] - normal[(j, j)]).abs() < 2e-10 * normal[(j, j)].abs().max(1.0)
                );
            }
            let up: Vec<_> = x
                .iter()
                .zip(&direction)
                .map(|(x, v)| x + 1e-5 * v)
                .collect();
            let down: Vec<_> = x
                .iter()
                .zip(&direction)
                .map(|(x, v)| x - 1e-5 * v)
                .collect();
            let up = NormalEquations::residual(&mut objective, &up);
            let down = NormalEquations::residual(&mut objective, &down);
            let error = jv
                .iter()
                .zip(up.iter().zip(down))
                .map(|(d, (u, l))| (d - (u - l) / 2e-5).abs())
                .fold(0.0_f64, f64::max);
            let scale = jv.iter().map(|v| v.abs()).fold(1.0_f64, f64::max);
            assert!(
                error < 5e-6 * scale,
                "analytic directional derivative {error}/{scale}"
            );
            let mut unchanged = vec![0.0; f.len()];
            objective.jvp(&direction, &mut unchanged).unwrap();
            assert!(
                unchanged
                    .iter()
                    .zip(&jv)
                    .all(|(a, b)| a.to_bits() == b.to_bits()),
                "trial residual invalidated the retained operator"
            );
        }
    }

    #[test]
    fn iir_matrix_free_solve_is_reachable_and_reduces_the_same_objective() {
        let (pb, tuning, x, pols) = fixture(80.0);
        let ctx = Context::serial();
        let mut objective = Objective::new(&pb, &tuning, &ctx, &pols);
        let initial = 0.5 * decay::energy(&NormalEquations::residual(&mut objective, &x));
        let options = IirSolveOptions {
            solver: IirSolver::MatrixFree {
                max_cg_iterations: 20,
                cg_rtol: 1e-4,
            },
            dense_fallback: false,
        };
        let result = solve_joint_with_options(&pb, &tuning, &ctx, &x, &pols, &options).unwrap();
        assert!(result.cost < initial * 0.95, "{initial} -> {}", result.cost);
        assert!(result.nfev > 1 && result.nfev <= tuning.mimo_max_nfev);
        for (r, &(lo, hi)) in pb.ranges.iter().enumerate() {
            let (lb, ub) = bounds(band_of(lo, hi, pb.fsiso, &tuning), &tuning);
            assert!(
                result.x[r * per(&tuning)..(r + 1) * per(&tuning)]
                    .iter()
                    .zip(lb)
                    .zip(ub)
                    .all(|((&x, l), u)| x >= l && x <= u)
            );
        }
    }

    #[test]
    fn iir_level_derivative_respects_the_additive_energy_floor() {
        let (mut pb, tuning, x, pols) = fixture(-400.0);
        for h in &mut pb.lp {
            *h *= 1e-15;
        }
        let ctx = Context::serial();
        let mut objective = Objective::new(&pb, &tuning, &ctx, &pols);
        let _ = NormalEquations::residual(&mut objective, &x);
        objective.linearize(&x).unwrap();
        let mut direction = vec![0.0; x.len()];
        direction[0] = 1.0;
        let mut jv = vec![0.0; objective.rows()];
        objective.jvp(&direction, &mut jv).unwrap();
        let (mut up, mut down) = (x.clone(), x);
        up[0] += 1e-3;
        down[0] -= 1e-3;
        let up = NormalEquations::residual(&mut objective, &up);
        let down = NormalEquations::residual(&mut objective, &down);
        let row = 2 * pb.xs.len() * pb.n;
        let central = (up[row] - down[row]) / 2e-3;
        assert!((jv[row] - central).abs() < 1e-7 * central.abs().max(1.0));
    }

    #[test]
    fn iir_operator_error_preserves_dense_fallback() {
        let (pb, tuning, x, pols) = fixture(80.0);
        let ctx = Context::serial();
        // Public API validation rejects this option before any design work.
        // Passing it internally injects an operator error to exercise recovery.
        let options = IirSolveOptions {
            solver: IirSolver::MatrixFree {
                max_cg_iterations: 0,
                cg_rtol: 1e-3,
            },
            dense_fallback: true,
        };
        assert!(options.validate().is_err());
        let recovered = solve_joint_with_options(&pb, &tuning, &ctx, &x, &pols, &options).unwrap();
        let dense = solve_joint(&pb, &tuning, &ctx, &x, &pols).unwrap();
        assert_eq!(recovered.cost.to_bits(), dense.cost.to_bits());
        assert_eq!(recovered.nfev, dense.nfev);
        assert!(
            recovered
                .x
                .iter()
                .zip(dense.x)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
        assert!(
            solve_joint_with_options(
                &pb,
                &tuning,
                &ctx,
                &x,
                &pols,
                &IirSolveOptions {
                    dense_fallback: false,
                    ..options
                }
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod tuning_tests {
    use super::*;

    #[test]
    fn iir_tuning_ranges_are_checked_at_entry() {
        // TD-104: band counts beyond the seed fit's residuals, and bounds too
        // narrow for the 1e-6 start margins (f64::clamp panics on min > max).
        let req = MimoRequest::default();
        let ok = |t: Tuning| validate_request(&req, &t).is_ok();
        assert!(ok(Tuning::default()));
        // 2 + 3·106 = 320 parameters per support: as many as residuals.
        assert!(ok(Tuning {
            mimo_k_bands: 106,
            mimo_a_bands: 0,
            ..Tuning::default()
        }));
        for (k, a) in [(106, 1), (0, 160), (usize::MAX, 0), (0, usize::MAX)] {
            let t = Tuning {
                mimo_k_bands: k,
                mimo_a_bands: a,
                ..Tuning::default()
            };
            assert!(
                matches!(validate_request(&req, &t), Err(EngineError::Input(_))),
                "{k} peaking and {a} all-pass bands"
            );
        }
        assert!(ok(Tuning {
            mimo_band_db: 2e-6,
            ..Tuning::default()
        }));
        for t in [
            Tuning {
                mimo_band_db: 1e-6,
                ..Tuning::default()
            },
            Tuning {
                mimo_q_max: 0.5 * 1e-6f64.exp(),
                ..Tuning::default()
            },
            Tuning {
                mimo_gain_max_db: -80.0 + 1e-6,
                ..Tuning::default()
            },
        ] {
            assert!(
                matches!(validate_request(&req, &t), Err(EngineError::Input(_))),
                "{t:?}"
            );
        }
    }
}
