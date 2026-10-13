//! Normal-equation form of SciPy 1.18.1 optimize/_lsq/trf.py (BSD-3-Clause,
//! Copyright SciPy Developers; see docs/THIRD_PARTY.md).
//! The residual is retained, but no m-by-n Jacobian is allocated here.

use super::{
    DspError, LeastSquaresResult, Mat, QuadraticModel, SelectTrace, dot, feasible, grad, norm,
    scaling, select, termination, trust,
};
use faer::{
    Par,
    diag::Diag,
    dyn_stack::{MemBuffer, MemStack},
    linalg::evd::{self, ComputeEigenvectors},
};

/// Supplies the residual and the normal equations of a least-squares problem.
pub trait NormalEquations {
    /// Residual f(x), counted in nfev like SciPy's fun.
    fn residual(&mut self, x: &[f64]) -> Vec<f64>;
    /// JᵀJ (n-by-n, both triangles filled) and Jᵀf at x, using the residual
    /// just returned for the same x. Counted in njev like SciPy's jac.
    fn normal(&mut self, x: &[f64], f: &[f64]) -> Result<(Mat<f64>, Vec<f64>), DspError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XScale {
    Unit,
    Jac,
}

#[derive(Debug, Clone, Copy)]
pub struct NormalOptions {
    pub max_nfev: usize,
    pub x_scale: XScale,
    pub ftol: f64,
    pub xtol: f64,
    pub gtol: f64,
}
impl Default for NormalOptions {
    fn default() -> Self {
        Self {
            max_nfev: 100,
            x_scale: XScale::Unit,
            ftol: 1e-8,
            xtol: 1e-8,
            gtol: 1e-8,
        }
    }
}

/// Trial diagnostics without storing the large residual or a Jacobian.
#[derive(Debug, Default)]
pub struct NormalTrial {
    pub x: Vec<f64>,
    pub cost: f64,
    pub scale_inv: Vec<f64>,
    pub v: Vec<f64>,
    pub delta: f64,
    pub alpha_in: f64,
    /// Alpha returned by solve_lsq_trust_region, before the radius rescaling.
    pub alpha_out: f64,
    pub s: Vec<f64>,
    pub full_rank: bool,
    pub step: Vec<f64>,
    pub x_new: Vec<f64>,
    pub predicted_reduction: f64,
    pub actual_reduction: f64,
    pub delta_new: f64,
    pub ratio: f64,
    pub accepted: bool,
    pub select: SelectTrace,
}
#[derive(Debug, Default)]
pub struct NormalTrace {
    pub trials: Vec<NormalTrial>,
}

/// Bounded exact TRF from JᵀJ and Jᵀf. Nonfinite or malformed normal equations
/// and failed eigendecompositions return Err, allowing the caller to retry
/// with Unit scaling. Nonfinite trial residuals shrink the trust radius.
pub fn least_squares_normal<P: NormalEquations>(
    problem: &mut P,
    x0: &[f64],
    lb: &[f64],
    ub: &[f64],
    options: &NormalOptions,
) -> Result<LeastSquaresResult, DspError> {
    least_squares_normal_with_trace(problem, x0, lb, ub, options, None)
}

pub fn least_squares_normal_with_trace<P: NormalEquations>(
    problem: &mut P,
    x0: &[f64],
    lb: &[f64],
    ub: &[f64],
    options: &NormalOptions,
    trace: Option<&mut NormalTrace>,
) -> Result<LeastSquaresResult, DspError> {
    solve(problem, x0, lb, ub, options, trace, None)
}

/// Replay a recorded trial, including a rejected-trial retry. The supplied
/// scale_inv is the historical maximum at that base point, not a fresh scale.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn trf_normal_one_step<P: NormalEquations>(
    problem: &mut P,
    x: &[f64],
    lb: &[f64],
    ub: &[f64],
    delta: f64,
    alpha: f64,
    scale_inv: &[f64],
) -> Result<NormalTrial, DspError> {
    if !delta.is_finite()
        || delta <= 0.
        || !alpha.is_finite()
        || alpha < 0.
        || scale_inv.len() != x.len()
        || scale_inv.iter().any(|s| !s.is_finite() || *s <= 0.)
    {
        return Err(crate::invalid("invalid normal TRF replay state"));
    }
    let mut trace = NormalTrace::default();
    solve(
        problem,
        x,
        lb,
        ub,
        &NormalOptions {
            max_nfev: 2,
            gtol: 0.,
            ..NormalOptions::default()
        },
        Some(&mut trace),
        Some((delta, alpha, scale_inv)),
    )?;
    trace
        .trials
        .pop()
        .ok_or_else(|| crate::invalid("no normal TRF replay trial"))
}

fn checked_normal<P: NormalEquations>(
    problem: &mut P,
    x: &[f64],
    f: &[f64],
) -> Result<(Mat<f64>, Vec<f64>), DspError> {
    let (h, g) = problem.normal(x, f)?;
    let n = x.len();
    if h.nrows() != n
        || h.ncols() != n
        || g.len() != n
        || g.iter().any(|v| !v.is_finite())
        || (0..n).any(|i| h[(i, i)] < 0. || (0..n).any(|j| !h[(i, j)].is_finite()))
    {
        return Err(crate::invalid("invalid/nonfinite TRF normal equations"));
    }
    Ok((h, g))
}

fn jac_scale(h: &Mat<f64>, old: Option<&[f64]>) -> Vec<f64> {
    // common.py:compute_jac_scale lines 603–610. Only the initial call
    // substitutes one for zero; later calls take the historical maximum.
    (0..h.ncols())
        .map(|i| {
            let s = h[(i, i)].sqrt();
            match old {
                Some(old) => s.max(old[i]),
                None if s == 0. => 1.,
                None => s,
            }
        })
        .collect()
}

struct Model {
    h: Mat<f64>,
    diag: Vec<f64>,
    gh: Vec<f64>,
    d: Vec<f64>,
    s: Vec<f64>,
    vectors: Mat<f64>,
    uf: Vec<f64>,
}
impl Model {
    fn new(
        h: &Mat<f64>,
        g: &[f64],
        v: &[f64],
        dv: &[f64],
        scale_inv: &[f64],
    ) -> Result<Self, DspError> {
        let n = g.len();
        // trf.py:288–296: d=sqrt(v)*scale, diag_h=g*dv*scale, g_h=d*g.
        let scale: Vec<_> = scale_inv.iter().map(|s| 1. / s).collect();
        let d: Vec<_> = (0..n).map(|i| v[i].sqrt() * scale[i]).collect();
        let diag: Vec<_> = (0..n).map(|i| g[i] * dv[i] * scale[i]).collect();
        let gh: Vec<_> = (0..n).map(|i| d[i] * g[i]).collect();
        // trf.py:300–305: augmented SVD replaced by M=diag(d) JᵀJ diag(d)+diag_h.
        let h = Mat::from_fn(n, n, |i, j| d[i] * h[(i, j)] * d[j]);
        let augmented = Mat::from_fn(n, n, |i, j| h[(i, j)] + if i == j { diag[i] } else { 0. });
        if (0..n).any(|i| (0..n).any(|j| !augmented[(i, j)].is_finite())) {
            return Err(crate::invalid("nonfinite TRF eigenproblem"));
        }
        let mut eigenvalues = Diag::zeros(n);
        let mut vectors = Mat::zeros(n, n);
        let mut scratch = MemBuffer::new(evd::self_adjoint_evd_scratch::<f64>(
            n,
            ComputeEigenvectors::Yes,
            Par::Seq,
            Default::default(),
        ));
        evd::self_adjoint_evd(
            augmented.as_ref(),
            eigenvalues.as_mut(),
            Some(vectors.as_mut()),
            Par::Seq,
            MemStack::new(&mut scratch),
            Default::default(),
        )
        .map_err(|_| crate::invalid("TRF eigendecomposition did not converge"))?;
        if (0..n).any(|i| {
            !eigenvalues.column_vector()[i].is_finite()
                || (0..n).any(|j| !vectors[(i, j)].is_finite())
        }) {
            return Err(crate::invalid("nonfinite TRF eigendecomposition"));
        }
        // faer returns ascending eigenvalues; SciPy's SVD is descending.
        let s: Vec<_> = (0..n)
            .rev()
            .map(|i| eigenvalues.column_vector()[i].max(0.).sqrt())
            .collect();
        let vectors = Mat::from_fn(n, n, |i, j| vectors[(i, n - 1 - j)]);
        let suf = grad(&vectors, &gh);
        let uf = suf
            .iter()
            .zip(&s)
            .map(|(v, s)| if *s > 0. { v / s } else { 0. })
            .collect();
        // common.py:118–123: trust reconstructs s*uf and applies the original
        // EPS*m*s[0] full-rank test, using the residual row count (not m+n).
        Ok(Self {
            h,
            diag,
            gh,
            d,
            s,
            vectors,
            uf,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn solve<P: NormalEquations>(
    problem: &mut P,
    x0: &[f64],
    lb: &[f64],
    ub: &[f64],
    options: &NormalOptions,
    mut trace: Option<&mut NormalTrace>,
    replay: Option<(f64, f64, &[f64])>,
) -> Result<LeastSquaresResult, DspError> {
    let n = x0.len();
    let tols = [options.ftol, options.xtol, options.gtol];
    if n == 0
        || lb.len() != n
        || ub.len() != n
        || options.max_nfev == 0
        || tols.iter().any(|t| !t.is_finite() || *t < 0.)
        || tols.iter().all(|t| *t <= f64::EPSILON)
        || !lb.iter().chain(ub).any(|b| b.is_finite())
        || (0..n).any(|i| {
            !x0[i].is_finite()
                || lb[i].is_nan()
                || ub[i].is_nan()
                || lb[i] >= ub[i]
                || x0[i] < lb[i]
                || x0[i] > ub[i]
        })
    {
        return Err(crate::invalid("invalid normal TRF start/bounds/options"));
    }
    if let Some(t) = trace.as_deref_mut() {
        t.trials.clear();
    }
    let mut x = x0.to_vec();
    if replay.is_none() {
        feasible(&mut x, lb, ub, true);
    }
    let mut f = problem.residual(&x);
    let m = f.len();
    if m == 0 || f.iter().any(|v| !v.is_finite()) {
        return Err(crate::invalid("invalid initial residual"));
    }
    let (mut h, mut g) = checked_normal(problem, &x, &f)?;
    let mut scale_inv = match options.x_scale {
        XScale::Unit => vec![1.; n],
        XScale::Jac => jac_scale(&h, None),
    };
    let (mut v, dv) = scaling(&x, &g, lb, ub);
    for i in 0..n {
        if dv[i] != 0. {
            v[i] *= scale_inv[i];
        }
    }
    // trf.py:234–238, including both x scaling and Coleman–Li scaling.
    let mut delta = norm(
        &(0..n)
            .map(|i| x[i] * scale_inv[i] / v[i].sqrt())
            .collect::<Vec<_>>(),
    );
    if delta == 0. {
        delta = 1.;
    }
    let mut alpha = 0.;
    if let Some((dd, aa, ss)) = replay {
        delta = dd;
        alpha = aa;
        scale_inv = ss.to_vec();
    }
    let mut cost = 0.5 * dot(&f, &f);
    let (mut nfev, mut njev, mut status) = (1, 1, 0);
    let optimality = loop {
        let (mut v, dv) = scaling(&x, &g, lb, ub);
        let gn = g
            .iter()
            .zip(&v)
            .map(|(g, v)| (g * v).abs())
            .fold(0., f64::max);
        if gn < options.gtol {
            status = 1;
        }
        if status != 0 || nfev == options.max_nfev {
            break gn;
        }
        // trf.py:285: scale v only after measuring the unscaled optimality.
        for i in 0..n {
            if dv[i] != 0. {
                v[i] *= scale_inv[i];
            }
        }
        let model = Model::new(&h, &g, &v, &dv, &scale_inv)?;
        let theta = 0.995_f64.max(1. - gn);
        let mut actual = -1.;
        let mut xn = x.clone();
        let mut fnn = f.clone();
        let mut cn = cost;
        while actual <= 0. && nfev < options.max_nfev {
            let (ph, a, _, full_rank) =
                trust(n, m, &model.uf, &model.s, &model.vectors, delta, alpha);
            let mut trial = trace.as_ref().map(|_| NormalTrial {
                x: x.clone(),
                cost,
                scale_inv: scale_inv.clone(),
                v: v.clone(),
                delta,
                alpha_in: alpha,
                alpha_out: a,
                s: model.s.clone(),
                full_rank,
                ..NormalTrial::default()
            });
            alpha = a;
            let p = ph.iter().zip(&model.d).map(|(p, d)| p * d).collect();
            let (step, sh, pred) = select(
                &x,
                QuadraticModel::Normal(&model.h),
                &model.diag,
                &model.gh,
                p,
                ph,
                &model.d,
                delta,
                lb,
                ub,
                theta,
                trial.as_mut().map(|t| &mut t.select),
            );
            xn = x.iter().zip(&step).map(|(x, s)| x + s).collect();
            feasible(&mut xn, lb, ub, false);
            fnn = problem.residual(&xn);
            nfev += 1;
            let shn = norm(&sh);
            if fnn.len() != m {
                return Err(crate::invalid("residual length changed"));
            }
            if fnn.iter().any(|f| !f.is_finite()) {
                if let (Some(t), Some(mut trial)) = (trace.as_deref_mut(), trial) {
                    trial.step = step;
                    trial.x_new = xn.clone();
                    trial.predicted_reduction = pred;
                    trial.actual_reduction = f64::NAN;
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
                trial.x_new = xn.clone();
                trial.predicted_reduction = pred;
                trial.actual_reduction = actual;
                trial.ratio = ratio;
                trial.delta_new = dn;
                trial.accepted = actual > 0.;
                t.trials.push(trial);
            }
            status = termination(actual, cost, &step, &x, ratio, options.ftol, options.xtol);
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
            // trf.py:376–386: accepted steps always update J/g/scale, including
            // the final accepted step that satisfied ftol or xtol.
            (h, g) = checked_normal(problem, &x, &f)?;
            njev += 1;
            if options.x_scale == XScale::Jac {
                scale_inv = jac_scale(&h, Some(&scale_inv));
            }
        }
    };
    Ok(LeastSquaresResult {
        x,
        cost,
        nfev,
        status,
        njev,
        optimality,
    })
}
