//! Adapted from Impulcifer-py313 `crates/impulcifer-dsp/src/optimize.rs` (MIT, Copyright (c) 2024- 115dkk).
//! See docs/THIRD_PARTY.md for attribution and licenses.
//! Bounded TRF, exact SVD solver, linear loss and bounded two-point differences.
//!
//! Port of SciPy 1.18.1 optimize/_lsq/{trf,common}.py and _numdiff.py,
//! Copyright SciPy Developers, BSD-3-Clause (see docs/THIRD_PARTY.md).
//! The dense exact-SVD solver supports x_scale=1; the normal-equation and opt-in
//! matrix-free solvers also support x_scale='jac'. All require linear loss and
//! at least one finite bound. The operator solver uses truncated CG for its
//! subproblem and intentionally does not promise the exact solver's trajectory.
//! Default ftol=xtol=gtol=1e-8; custom/disabled tolerances are explicit options. Jacobian evaluations
//! are deliberately excluded from nfev, as in scipy.optimize.least_squares.
mod de;
mod normal;
mod operator;
pub use de::{DifferentialEvolutionOptions, DifferentialEvolutionResult, differential_evolution};
pub use normal::{
    NormalEquations, NormalOptions, NormalTrace, NormalTrial, XScale, least_squares_normal,
    least_squares_normal_with_trace, trf_normal_one_step,
};
pub use operator::{JacobianOperator, OperatorOptions, least_squares_operator};

use crate::DspError;
use faer::Mat;

#[derive(Debug)]
pub struct LeastSquaresResult {
    pub x: Vec<f64>,
    pub cost: f64,
    pub nfev: usize,
    pub status: i32,
    pub njev: usize,
    pub optimality: f64,
}
/// Optional diagnostics; main evaluations exclude the initial residual and
/// Jacobian probes, matching SciPy's trf_bounds call boundary.
#[derive(Debug, Default)]
pub struct LeastSquaresTrace {
    /// Every residual call, including initial and two-point probes.
    pub evaluations: Vec<Evaluation>,
    /// Every Jacobian base point, including the initial point.
    pub jacobian_points: Vec<Vec<f64>>,
    pub main_x: Vec<Vec<f64>>,
    pub main_cost: Vec<f64>,
    /// Jacobian points inside TRF; the separately supplied initial J0 is excluded.
    pub jac_x: Vec<Vec<f64>>,
    pub j0: Vec<f64>,
    pub j0_shape: [usize; 2],
    pub initial_cost: f64,
    pub trials: Vec<TrfTrial>,
}
#[derive(Debug)]
pub struct Evaluation {
    pub x: Vec<f64>,
    pub cost: f64,
    pub column: Option<usize>,
}
/// Exact-TRF termination tolerances. None disables one test.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub max_nfev: usize,
    pub ftol: Option<f64>,
    pub xtol: Option<f64>,
    pub gtol: Option<f64>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_nfev: 100,
            ftol: Some(1e-8),
            xtol: Some(1e-8),
            gtol: Some(1e-8),
        }
    }
}
#[derive(Debug, Default, serde::Serialize)]
pub struct TrfTrial {
    pub iteration: usize,
    pub x: Vec<f64>,
    pub f: Vec<f64>,
    /// Row-major unscaled Jacobian at this trial's base point.
    #[serde(rename = "J")]
    pub j: Vec<f64>,
    pub g: Vec<f64>,
    pub d: Vec<f64>,
    #[serde(rename = "J_h")]
    pub j_h: Vec<f64>,
    pub diag_h: Vec<f64>,
    pub g_h: Vec<f64>,
    pub theta: f64,
    /// Row-major right singular vectors; column signs match `uf`.
    #[serde(rename = "V")]
    pub right_vectors: Vec<f64>,
    pub g_norm: f64,
    pub v: Vec<f64>,
    pub dv: Vec<f64>,
    #[serde(rename = "Delta")]
    pub delta: f64,
    pub s: Vec<f64>,
    pub uf: Vec<f64>,
    pub alpha_in: f64,
    pub alpha_out: f64,
    pub n_iter: usize,
    pub full_rank: bool,
    pub p_h: Vec<f64>,
    pub select: SelectTrace,
    pub step: Vec<f64>,
    pub step_h: Vec<f64>,
    pub x_new: Vec<f64>,
    pub actual_reduction: f64,
    pub predicted_reduction: f64,
    pub ratio: f64,
    #[serde(rename = "Delta_new")]
    pub delta_new: f64,
    pub accepted: bool,
    pub jacobian_recomputed: bool,
}
#[derive(Debug, Default, serde::Serialize)]
pub struct SelectTrace {
    pub branch: &'static str,
    pub p_value: Option<f64>,
    pub r_value: Option<f64>,
    pub ag_value: Option<f64>,
    pub p_stride: Option<f64>,
    pub r_stride: Option<f64>,
    pub r_stride_l: Option<f64>,
    pub r_stride_u: Option<f64>,
    pub ag_stride: Option<f64>,
    pub to_tr: Option<f64>,
    pub to_bound: Option<f64>,
}
// A single long accumulator perturbs the anti-gradient line minimum enough
// to change its next finite-difference Jacobian (the replay pins that case).
// Independent lanes and a balanced merge reduce that error without FMA or BLAS.
fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut sums = [0.; 8];
    let chunks = a.len() / 8 * 8;
    for i in (0..chunks).step_by(8) {
        for k in 0..8 {
            sums[k] += a[i + k] * b[i + k];
        }
    }
    let mut sum =
        ((sums[0] + sums[1]) + (sums[2] + sums[3])) + ((sums[4] + sums[5]) + (sums[6] + sums[7]));
    for i in chunks..a.len() {
        sum += a[i] * b[i];
    }
    sum
}
fn norm(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}
fn row_major(a: &Mat<f64>) -> Vec<f64> {
    (0..a.nrows())
        .flat_map(|i| (0..a.ncols()).map(move |j| a[(i, j)]))
        .collect()
}
fn mv(a: &Mat<f64>, x: &[f64]) -> Vec<f64> {
    (0..a.nrows())
        .map(|i| (0..a.ncols()).map(|j| a[(i, j)] * x[j]).sum())
        .collect()
}
fn grad(a: &Mat<f64>, f: &[f64]) -> Vec<f64> {
    (0..a.ncols())
        .map(|j| (0..a.nrows()).map(|i| a[(i, j)] * f[i]).sum())
        .collect()
}
fn feasible(x: &mut [f64], lb: &[f64], ub: &[f64], initial: bool) {
    for ((x, &l), &u) in x.iter_mut().zip(lb).zip(ub) {
        if initial {
            let ld = *x - l;
            let ud = u - *x;
            if l.is_finite() && ld <= ud.min(1e-10 * l.abs().max(1.)) {
                *x = l + 1e-10 * l.abs().max(1.);
            }
            if u.is_finite() && ud <= ld.min(1e-10 * u.abs().max(1.)) {
                *x = u - 1e-10 * u.abs().max(1.);
            }
        } else {
            if *x <= l {
                *x = l.next_up();
            } else if *x >= u {
                *x = u.next_down();
            }
        }
        if *x < l || *x > u {
            *x = 0.5 * (l + u);
        }
    }
}
fn jacobian<F: FnMut(&[f64], Option<usize>, &mut Vec<f64>)>(
    fun: &mut F,
    x: &[f64],
    f: &[f64],
    lb: &[f64],
    ub: &[f64],
) -> Result<Mat<f64>, DspError> {
    let mut j = Mat::zeros(f.len(), x.len());
    let mut y = x.to_vec();
    let mut fy = vec![0.; f.len()];
    for k in 0..x.len() {
        let mut h = f64::EPSILON.sqrt() * if x[k] >= 0. { 1. } else { -1. } * x[k].abs().max(1.);
        let lower = x[k] - lb[k];
        let upper = ub[k] - x[k];
        let fitting = h.abs() <= lower.max(upper);
        if (x[k] + h < lb[k] || x[k] + h > ub[k]) && fitting {
            h = -h;
        }
        if !fitting {
            h = if upper >= lower { upper } else { -lower };
        }
        y[k] += h;
        let dx = y[k] - x[k];
        fun(&y, Some(k), &mut fy);
        y[k] = x[k];
        if fy.len() != f.len() || fy.iter().any(|x| !x.is_finite()) {
            return Err(DspError::InvalidArgument(
                "nonfinite or inconsistent residual Jacobian".into(),
            ));
        }
        for i in 0..f.len() {
            j[(i, k)] = (fy[i] - f[i]) / dx;
        }
    }
    Ok(j)
}
fn scaling(x: &[f64], g: &[f64], lb: &[f64], ub: &[f64]) -> (Vec<f64>, Vec<f64>) {
    (0..x.len())
        .map(|i| {
            if g[i] < 0. && ub[i].is_finite() {
                (ub[i] - x[i], -1.)
            } else if g[i] > 0. && lb[i].is_finite() {
                (x[i] - lb[i], 1.)
            } else {
                (1., 0.)
            }
        })
        .unzip()
}
fn bound_step(x: &[f64], s: &[f64], lb: &[f64], ub: &[f64]) -> (f64, Vec<bool>) {
    let steps: Vec<_> = (0..x.len())
        .map(|i| {
            if s[i] == 0. {
                f64::INFINITY
            } else {
                ((lb[i] - x[i]) / s[i]).max((ub[i] - x[i]) / s[i])
            }
        })
        .collect();
    let t = steps.iter().copied().fold(f64::INFINITY, f64::min);
    (t, steps.iter().map(|s| *s == t).collect())
}
fn quadratic(j: &Mat<f64>, g: &[f64], diag: &[f64], s: &[f64]) -> f64 {
    let js = mv(j, s);
    0.5 * (dot(&js, &js) + (0..s.len()).map(|i| s[i] * diag[i] * s[i]).sum::<f64>()) + dot(s, g)
}
fn quadratic_line(
    j: &Mat<f64>,
    g: &[f64],
    diag: &[f64],
    s: &[f64],
    s0: Option<&[f64]>,
) -> (f64, f64, f64) {
    let v = mv(j, s);
    let a = 0.5 * (dot(&v, &v) + (0..s.len()).map(|i| s[i] * diag[i] * s[i]).sum::<f64>());
    let mut b = dot(g, s);
    let mut c = 0.;
    if let Some(s0) = s0 {
        let u = mv(j, s0);
        b += dot(&u, &v);
        c = 0.5 * dot(&u, &u) + dot(g, s0);
        b += (0..s.len()).map(|i| s0[i] * diag[i] * s[i]).sum::<f64>();
        c += 0.5 * (0..s.len()).map(|i| s0[i] * diag[i] * s0[i]).sum::<f64>();
    }
    (a, b, c)
}
fn minimize((a, b, c): (f64, f64, f64), lo: f64, hi: f64) -> (f64, f64) {
    let value = |t: f64| t * (a * t + b) + c;
    let mut best = (lo, value(lo));
    if value(hi) < best.1 {
        best = (hi, value(hi));
    }
    let e = -0.5 * b / a;
    if a != 0. && lo < e && e < hi && value(e) < best.1 {
        best = (e, value(e));
    }
    best
}
enum QuadraticModel<'a> {
    Dense(&'a Mat<f64>),
    Normal(&'a Mat<f64>),
}
impl QuadraticModel<'_> {
    fn value(&self, g: &[f64], diag: &[f64], s: &[f64]) -> f64 {
        match self {
            Self::Dense(j) => quadratic(j, g, diag, s),
            // common.py:evaluate_quadratic: ||J_h s||² = sᵀ H s.
            Self::Normal(h) => {
                0.5 * (dot(s, &mv(h, s)) + (0..s.len()).map(|i| s[i] * diag[i] * s[i]).sum::<f64>())
                    + dot(s, g)
            }
        }
    }
    fn line(&self, g: &[f64], diag: &[f64], s: &[f64], s0: Option<&[f64]>) -> (f64, f64, f64) {
        match self {
            Self::Dense(j) => quadratic_line(j, g, diag, s, s0),
            // common.py:build_quadratic_1d: (J_h s0)ᵀ(J_h s) = s0ᵀ H s.
            Self::Normal(h) => {
                let hs = mv(h, s);
                let a =
                    0.5 * (dot(s, &hs) + (0..s.len()).map(|i| s[i] * diag[i] * s[i]).sum::<f64>());
                let mut b = dot(g, s);
                let mut c = 0.;
                if let Some(s0) = s0 {
                    b += dot(s0, &hs);
                    c = 0.5 * dot(s0, &mv(h, s0)) + dot(g, s0);
                    b += (0..s.len()).map(|i| s0[i] * diag[i] * s[i]).sum::<f64>();
                    c += 0.5 * (0..s.len()).map(|i| s0[i] * diag[i] * s0[i]).sum::<f64>();
                }
                (a, b, c)
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn select(
    x: &[f64],
    model: QuadraticModel<'_>,
    diag: &[f64],
    g: &[f64],
    mut p: Vec<f64>,
    mut ph: Vec<f64>,
    d: &[f64],
    delta: f64,
    lb: &[f64],
    ub: &[f64],
    theta: f64,
    mut trace: Option<&mut SelectTrace>,
) -> (Vec<f64>, Vec<f64>, f64) {
    if (0..x.len()).all(|i| x[i] + p[i] >= lb[i] && x[i] + p[i] <= ub[i]) {
        let q = model.value(g, diag, &ph);
        if let Some(t) = trace {
            t.branch = "p";
            t.p_value = Some(q);
        }
        return (p, ph, -q);
    }
    let (stride, hits) = bound_step(x, &p, lb, ub);
    let mut rh: Vec<_> = ph
        .iter()
        .zip(hits)
        .map(|(v, h)| if h { -v } else { *v })
        .collect();
    let r: Vec<_> = rh.iter().zip(d).map(|(a, b)| a * b).collect();
    for v in &mut p {
        *v *= stride;
    }
    for v in &mut ph {
        *v *= stride;
    }
    let xb: Vec<_> = x.iter().zip(&p).map(|(a, b)| a + b).collect();
    let a = dot(&rh, &rh);
    let b = dot(&ph, &rh);
    let c = dot(&ph, &ph) - delta * delta;
    let root = (b * b - a * c).sqrt();
    let q = -(b + root.copysign(b));
    let tr = (q / a).max(c / q);
    let (bound, _) = bound_step(&xb, &r, lb, ub);
    let rs = bound.min(tr);
    let (lo, hi) = if rs > 0. {
        (
            (1. - theta) * stride / rs,
            if rs == bound { theta * bound } else { tr },
        )
    } else {
        (0., -1.)
    };
    if let Some(t) = trace.as_deref_mut() {
        t.p_stride = Some(stride);
        t.r_stride_l = Some(lo);
        t.r_stride_u = Some(hi);
        t.r_stride = Some(rs);
    }
    let mut rv = f64::INFINITY;
    if lo <= hi {
        let (t, v) = minimize(model.line(g, diag, &rh, Some(&ph)), lo, hi);
        rv = v;
        if let Some(trace) = trace.as_deref_mut() {
            trace.r_stride = Some(t);
        }
        for i in 0..rh.len() {
            rh[i] = rh[i] * t + ph[i];
        }
    }
    for v in &mut p {
        *v *= theta;
    }
    for v in &mut ph {
        *v *= theta;
    }
    let pv = model.value(g, diag, &ph);
    let mut ah: Vec<_> = g.iter().map(|v| -v).collect();
    let mut ag: Vec<_> = ah.iter().zip(d).map(|(a, b)| a * b).collect();
    let tr = delta / norm(&ah);
    let (bound, _) = bound_step(x, &ag, lb, ub);
    let upper = if bound < tr { theta * bound } else { tr };
    let (stride, av) = minimize(model.line(g, diag, &ah, None), 0., upper);
    for v in &mut ah {
        *v *= stride;
    }
    for v in &mut ag {
        *v *= stride;
    }
    if let Some(t) = trace {
        t.p_value = Some(pv);
        t.r_value = Some(rv);
        t.ag_value = Some(av);
        t.ag_stride = Some(stride);
        t.to_tr = Some(tr);
        t.to_bound = Some(bound);
        t.branch = if pv < rv && pv < av {
            "p"
        } else if rv < pv && rv < av {
            "reflected"
        } else {
            "anti-gradient"
        };
    }
    if pv < rv && pv < av {
        (p, ph, -pv)
    } else if rv < pv && rv < av {
        (rh.iter().zip(d).map(|(a, b)| a * b).collect(), rh, -rv)
    } else {
        (ag, ah, -av)
    }
}
fn termination(
    actual: f64,
    cost: f64,
    step: &[f64],
    x: &[f64],
    ratio: f64,
    ftol: f64,
    xtol: f64,
) -> i32 {
    let ft = actual < ftol * cost && ratio > 0.25;
    let xt = norm(step) < xtol * (xtol + norm(x));
    match (ft, xt) {
        (true, true) => 4,
        (true, false) => 2,
        (false, true) => 3,
        _ => 0,
    }
}
fn phi_and_derivative(suf: &[f64], s: &[f64], alpha: f64, delta: f64) -> (f64, f64) {
    let p_norm = norm(
        &suf.iter()
            .zip(s)
            .map(|(u, s)| u / (s * s + alpha))
            .collect::<Vec<_>>(),
    );
    // SciPy uses NumPy power (libm pow for exponent 3), then np.sum's
    // pairwise reduction, not repeated multiplication and a linear sum.
    let derivative = -crate::numeric::sum(
        &suf.iter()
            .zip(s)
            .map(|(u, s)| u * u / (s * s + alpha).powf(3.))
            .collect::<Vec<_>>(),
    ) / p_norm;
    (p_norm - delta, derivative)
}
fn trust(
    n: usize,
    m: usize,
    uf: &[f64],
    s: &[f64],
    v: &Mat<f64>,
    delta: f64,
    mut alpha: f64,
) -> (Vec<f64>, f64, usize, bool) {
    let suf: Vec<_> = s.iter().zip(uf).map(|(a, b)| a * b).collect();
    let full = m >= n && s[n - 1] > f64::EPSILON * m as f64 * s[0];
    if full {
        let p = mv(
            v,
            &uf.iter().zip(s).map(|(a, b)| -a / b).collect::<Vec<_>>(),
        );
        if norm(&p) <= delta {
            return (p, 0., 0, full);
        }
    }
    let phi = |a: f64| phi_and_derivative(&suf, s, a, delta);
    let mut upper = norm(&suf) / delta;
    let mut lower = if full {
        let (p, d) = phi(0.);
        -p / d
    } else {
        0.
    };
    if !full && alpha == 0. {
        alpha = (0.001 * upper).max((lower * upper).sqrt());
    }
    let mut n_iter = 0;
    for _ in 0..10 {
        n_iter += 1;
        if alpha < lower || alpha > upper {
            alpha = (0.001 * upper).max((lower * upper).sqrt());
        }
        let (p, dp) = phi(alpha);
        if p < 0. {
            upper = alpha;
        }
        let ratio = p / dp;
        lower = lower.max(alpha - ratio);
        alpha -= (p + delta) * ratio / delta;
        if p.abs() < 0.01 * delta {
            break;
        }
    }
    let mut p = mv(
        v,
        &suf.iter()
            .zip(s)
            .map(|(u, s)| -u / (s * s + alpha))
            .collect::<Vec<_>>(),
    );
    let scale = delta / norm(&p);
    for v in &mut p {
        *v *= scale;
    }
    (p, alpha, n_iter, full)
}
/// SciPy's bounded exact-TRF subset. The closure may cache computations but
/// must return the same residual at the same x. Bounds may be infinite.
pub fn least_squares<F: FnMut(&[f64]) -> Vec<f64>>(
    fun: F,
    x0: &[f64],
    lb: &[f64],
    ub: &[f64],
    max_nfev: usize,
) -> Result<LeastSquaresResult, DspError> {
    least_squares_with_trace(fun, x0, lb, ub, max_nfev, None)
}
/// The production solver, with opt-in capture. None allocates no trace storage.
pub fn least_squares_with_trace<F: FnMut(&[f64]) -> Vec<f64>>(
    mut fun: F,
    x0: &[f64],
    lb: &[f64],
    ub: &[f64],
    max_nfev: usize,
    trace: Option<&mut LeastSquaresTrace>,
) -> Result<LeastSquaresResult, DspError> {
    least_squares_with_options(
        |x| fun(x),
        x0,
        lb,
        ub,
        Options {
            max_nfev,
            ..Options::default()
        },
        trace,
    )
}
/// Bounded solver with optional complete trace. At least one finite bound is
/// required: unconstrained SciPy uses a different algorithm and is not silently
/// routed through this bounded port. Residual length changes/nonfinite initial
/// or Jacobian values return Err; nonfinite trial residuals shrink the radius.
pub fn least_squares_with_options<F: FnMut(&[f64]) -> Vec<f64>>(
    residual: F,
    x0: &[f64],
    lb: &[f64],
    ub: &[f64],
    options: Options,
    trace: Option<&mut LeastSquaresTrace>,
) -> Result<LeastSquaresResult, DspError> {
    solve(residual, x0, lb, ub, options, trace, None, None)
}

/// Dense analytic-Jacobian reference path, sharing the original exact-SVD loop.
#[doc(hidden)]
pub fn least_squares_analytic<F, J>(
    residual: F,
    mut jac: J,
    x0: &[f64],
    lb: &[f64],
    ub: &[f64],
    options: Options,
    trace: Option<&mut LeastSquaresTrace>,
) -> Result<LeastSquaresResult, DspError>
where
    F: FnMut(&[f64]) -> Vec<f64>,
    J: FnMut(&[f64]) -> Mat<f64>,
{
    solve(residual, x0, lb, ub, options, trace, None, Some(&mut jac))
}

/// Replay one trial of the production loop at an oracle base point. Recomputes
/// f and the bounded two-point J, including on rejected-trial retries, but does
/// not perturb x with initial feasibility adjustment or apply the gtol exit.
/// `jacobian_recomputed` describes the update after this trial, not that setup.
#[doc(hidden)]
pub fn trf_one_step<F: FnMut(&[f64]) -> Vec<f64>>(
    residual: F,
    x: &[f64],
    lb: &[f64],
    ub: &[f64],
    delta: f64,
    alpha: f64,
) -> Result<TrfTrial, DspError> {
    if !delta.is_finite() || delta <= 0. || !alpha.is_finite() || alpha < 0. {
        return Err(DspError::InvalidArgument(
            "invalid replay radius/alpha".into(),
        ));
    }
    let mut trace = LeastSquaresTrace::default();
    solve(
        residual,
        x,
        lb,
        ub,
        Options {
            max_nfev: 2,
            gtol: None,
            ..Options::default()
        },
        Some(&mut trace),
        Some((delta, alpha)),
        None,
    )?;
    trace
        .trials
        .pop()
        .ok_or_else(|| DspError::InvalidArgument("no replay trial".into()))
}

type AnalyticJacobian<'a> = dyn FnMut(&[f64]) -> Mat<f64> + 'a;

#[allow(clippy::too_many_arguments)]
fn solve<F: FnMut(&[f64]) -> Vec<f64>>(
    mut residual: F,
    x0: &[f64],
    lb: &[f64],
    ub: &[f64],
    options: Options,
    mut trace: Option<&mut LeastSquaresTrace>,
    replay: Option<(f64, f64)>,
    mut analytic: Option<&mut AnalyticJacobian<'_>>,
) -> Result<LeastSquaresResult, DspError> {
    let max_nfev = options.max_nfev;
    let ftol = options.ftol.unwrap_or(0.);
    let xtol = options.xtol.unwrap_or(0.);
    let gtol = options.gtol.unwrap_or(0.);
    if [ftol, xtol, gtol].iter().any(|v| !v.is_finite() || *v < 0.)
        || [ftol, xtol, gtol].iter().all(|v| *v <= f64::EPSILON)
    {
        return Err(DspError::InvalidArgument(
            "invalid stopping tolerances".into(),
        ));
    }
    if !lb.iter().chain(ub).any(|v| v.is_finite()) {
        return Err(DspError::InvalidArgument(
            "bounded TRF requires at least one finite bound".into(),
        ));
    }
    let evaluations = std::cell::RefCell::new(Vec::new());
    let capture = trace.is_some();
    let mut fun = |x: &[f64], column: Option<usize>, out: &mut Vec<f64>| {
        *out = residual(x);
        if capture {
            evaluations.borrow_mut().push(Evaluation {
                x: x.to_vec(),
                cost: 0.5 * dot(out, out),
                column,
            });
        }
    };
    if let Some(t) = trace.as_deref_mut() {
        *t = LeastSquaresTrace::default();
    }
    let n = x0.len();
    if n == 0
        || lb.len() != n
        || ub.len() != n
        || max_nfev == 0
        || (0..n).any(|i| {
            !x0[i].is_finite()
                || lb[i].is_nan()
                || ub[i].is_nan()
                || lb[i] >= ub[i]
                || x0[i] < lb[i]
                || x0[i] > ub[i]
        })
    {
        return Err(DspError::InvalidArgument(
            "invalid least-squares start/bounds/budget".into(),
        ));
    }
    let mut x = x0.to_vec();
    if replay.is_none() {
        feasible(&mut x, lb, ub, true);
    }
    let mut f = Vec::new();
    fun(&x, None, &mut f);
    let m = f.len();
    if m == 0 || f.iter().any(|v| !v.is_finite()) {
        return Err(DspError::InvalidArgument("invalid initial residual".into()));
    }
    let mut get_jacobian = |x: &[f64], f: &[f64], fun: &mut _| {
        if let Some(jac) = analytic.as_mut() {
            let j = jac(x);
            if j.nrows() != f.len()
                || j.ncols() != x.len()
                || row_major(&j).iter().any(|v| !v.is_finite())
            {
                return Err(DspError::InvalidArgument(
                    "invalid analytic Jacobian".into(),
                ));
            }
            Ok(j)
        } else {
            jacobian(fun, x, f, lb, ub)
        }
    };
    let mut j = get_jacobian(&x, &f, &mut fun)?;
    let mut cost = 0.5 * dot(&f, &f);
    let mut njev = 1;
    if let Some(t) = trace.as_deref_mut() {
        t.jacobian_points.push(x.clone());
        t.initial_cost = cost;
        t.j0_shape = [m, n];
        t.j0 = (0..m)
            .flat_map(|i| (0..n).map(move |k| (i, k)))
            .map(|(i, k)| j[(i, k)])
            .collect();
    }
    let mut g = grad(&j, &f);
    let (v, _) = scaling(&x, &g, lb, ub);
    let mut delta = norm(
        &x.iter()
            .zip(v)
            .map(|(x, v)| x / v.sqrt())
            .collect::<Vec<_>>(),
    );
    if delta == 0. {
        delta = 1.;
    }
    let mut alpha = 0.;
    if let Some((oracle_delta, oracle_alpha)) = replay {
        delta = oracle_delta;
        alpha = oracle_alpha;
    }
    let mut nfev = 1;
    let mut status = 0;
    let mut iteration = 0;
    loop {
        let (v, dv) = scaling(&x, &g, lb, ub);
        let gn = g
            .iter()
            .zip(&v)
            .map(|(g, v)| (g * v).abs())
            .fold(0., f64::max);
        if gn < gtol {
            status = 1;
        }
        if status != 0 || nfev == max_nfev {
            break;
        }
        let d: Vec<_> = v.iter().map(|v| v.sqrt()).collect();
        let diag: Vec<_> = g.iter().zip(&dv).map(|(g, v)| g * v).collect();
        let gh: Vec<_> = d.iter().zip(&g).map(|(d, g)| d * g).collect();
        let jh = Mat::from_fn(m, n, |i, k| j[(i, k)] * d[k]);
        let aug = Mat::from_fn(m + n, n, |i, k| {
            if i < m {
                jh[(i, k)]
            } else if i - m == k {
                diag[k].sqrt()
            } else {
                0.
            }
        });
        // Explicit Seq also survives downstream Cargo feature unification.
        use faer::{
            Par,
            diag::Diag,
            dyn_stack::{MemBuffer, MemStack},
            linalg::svd::{self, ComputeSvdVectors},
        };
        let mut u = Mat::zeros(m + n, n);
        let mut vv = Mat::zeros(n, n);
        let mut singular = Diag::zeros(n);
        let mut scratch = MemBuffer::new(svd::svd_scratch::<f64>(
            m + n,
            n,
            ComputeSvdVectors::Thin,
            ComputeSvdVectors::Thin,
            Par::Seq,
            Default::default(),
        ));
        svd::svd(
            aug.as_ref(),
            singular.as_mut(),
            Some(u.as_mut()),
            Some(vv.as_mut()),
            Par::Seq,
            MemStack::new(&mut scratch),
            Default::default(),
        )
        .map_err(|_| DspError::InvalidArgument("TRF SVD did not converge".into()))?;
        let s: Vec<_> = (0..n).map(|i| singular.column_vector()[i]).collect();
        let uf: Vec<_> = (0..n)
            .map(|k| (0..m).map(|i| u[(i, k)] * f[i]).sum())
            .collect();
        let theta = 0.995_f64.max(1. - gn);
        let mut actual = -1.;
        let mut xn = x.clone();
        let mut fnn = f.clone();
        let mut cn = cost;
        while actual <= 0. && nfev < max_nfev {
            let (ph, a, n_iter, full_rank) = trust(n, m, &uf, &s, &vv, delta, alpha);
            let mut trial = trace.as_ref().map(|_| TrfTrial {
                iteration,
                x: x.clone(),
                f: f.clone(),
                j: row_major(&j),
                g: g.clone(),
                d: d.clone(),
                j_h: row_major(&jh),
                diag_h: diag.clone(),
                g_h: gh.clone(),
                theta,
                right_vectors: row_major(&vv),
                g_norm: gn,
                v: v.clone(),
                dv: dv.clone(),
                delta,
                s: s.clone(),
                uf: uf.clone(),
                alpha_in: alpha,
                alpha_out: a,
                n_iter,
                full_rank,
                p_h: ph.clone(),
                ..Default::default()
            });
            alpha = a;
            let p = ph.iter().zip(&d).map(|(a, b)| a * b).collect();
            let (step, sh, pred) = select(
                &x,
                QuadraticModel::Dense(&jh),
                &diag,
                &gh,
                p,
                ph,
                &d,
                delta,
                lb,
                ub,
                theta,
                trial.as_mut().map(|t| &mut t.select),
            );
            xn = x.iter().zip(&step).map(|(a, b)| a + b).collect();
            feasible(&mut xn, lb, ub, false);
            fun(&xn, None, &mut fnn);
            nfev += 1;
            if let Some(t) = trace.as_deref_mut() {
                t.main_x.push(xn.clone());
                t.main_cost.push(0.5 * dot(&fnn, &fnn));
            }
            let shn = norm(&sh);
            if fnn.len() != m {
                return Err(DspError::InvalidArgument("residual length changed".into()));
            }
            if fnn.iter().any(|v| !v.is_finite()) {
                if let (Some(t), Some(mut trial)) = (trace.as_deref_mut(), trial) {
                    trial.step = step;
                    trial.step_h = sh;
                    trial.x_new = xn.clone();
                    trial.actual_reduction = f64::NAN;
                    trial.predicted_reduction = pred;
                    trial.ratio = f64::NAN;
                    trial.delta_new = 0.25 * shn;
                    t.trials.push(trial);
                }
                delta = 0.25 * shn;
                continue;
            }
            cn = 0.5 * dot(&fnn, &fnn);
            actual = cost - cn;
            let ratio = if pred > 0. {
                actual / pred
            } else if pred == 0. && actual == 0. {
                1.
            } else {
                0.
            };
            let dn = if ratio < 0.25 {
                0.25 * shn
            } else if ratio > 0.75 && shn > 0.95 * delta {
                2. * delta
            } else {
                delta
            };
            if let (Some(t), Some(mut trial)) = (trace.as_deref_mut(), trial) {
                trial.step = step.clone();
                trial.step_h = sh.clone();
                trial.x_new = xn.clone();
                trial.actual_reduction = actual;
                trial.predicted_reduction = pred;
                trial.ratio = ratio;
                trial.delta_new = dn;
                trial.accepted = actual > 0.;
                t.trials.push(trial);
            }
            status = termination(actual, cost, &step, &x, ratio, ftol, xtol);
            if status != 0 {
                break;
            }
            alpha *= delta / dn;
            delta = dn;
        }
        if actual > 0. {
            x = xn;
            f = fnn;
            cost = cn;
            j = get_jacobian(&x, &f, &mut fun)?;
            njev += 1;
            if let Some(t) = trace.as_deref_mut() {
                t.jac_x.push(x.clone());
                t.jacobian_points.push(x.clone());
                if let Some(trial) = t.trials.last_mut() {
                    trial.jacobian_recomputed = true;
                }
            }
            g = grad(&j, &f);
        }
        iteration += 1;
    }
    let (v, _) = scaling(&x, &g, lb, ub);
    let optimality = g
        .iter()
        .zip(v)
        .map(|(g, v)| (g * v).abs())
        .fold(0., f64::max);
    if let Some(t) = trace {
        t.evaluations = evaluations.into_inner();
    }
    Ok(LeastSquaresResult {
        x,
        cost,
        nfev,
        status,
        njev,
        optimality,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn newton_derivative_uses_numpy_power_and_pairwise_sum() {
        let suf: Vec<_> = (1..10).map(|i| i as f64 * 0.173).collect();
        let s: Vec<_> = (1..10).map(|i| i as f64 * 0.117).collect();
        let (_, derivative) = super::phi_and_derivative(&suf, &s, 0.17, 1.);
        // NumPy 2.5.3: -np.sum(suf**2 / (s**2 + alpha)**3) / norm(...).
        secs_oracle::assert_same(
            "TRF Newton derivative",
            &[derivative],
            &[-10.858296042275914],
        );
    }

    #[test]
    fn feasible_adjacent_bounds_uses_original_active_mask() {
        let lower = 1.0_f64;
        let upper = lower.next_up();
        let mut x = [lower];
        super::feasible(&mut x, &[lower], &[upper], false);
        assert_eq!(x[0], upper);
        super::feasible(&mut x, &[lower], &[upper], false);
        assert_eq!(x[0], lower);
    }
}
