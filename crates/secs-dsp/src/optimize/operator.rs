//! Opt-in matrix-free bounded trust-region least squares.
//!
//! Coleman–Li scaling, reflective/Cauchy candidate selection and acceptance
//! follow the bounded solver in the parent module. The trust-region subproblem
//! uses preconditioned Steihaug CG instead of an exact SVD/eigendecomposition.
//! No residual-by-parameter Jacobian or parameter-by-parameter Gram matrix is
//! formed. The different subproblem solution intentionally changes trajectories.
//! Bound scaling/selection adapted from SciPy 1.18.1 optimize/_lsq (BSD-3-Clause,
//! Copyright SciPy Developers); see docs/THIRD_PARTY.md.

use super::{
    DspError, LeastSquaresResult, XScale, bound_step, dot, feasible, minimize, norm, scaling,
    termination,
};

/// A real residual with an independently retained linearization.
///
/// `linearize` returns the squared Euclidean norm of each Jacobian column.
/// Subsequent products use that base point until the next `linearize` call;
/// evaluating trial residuals must not invalidate it. Products overwrite all
/// entries of the supplied output buffer and use real inner products, including
/// when the implementation internally uses complex spectra.
pub trait JacobianOperator {
    fn residual(&mut self, x: &[f64]) -> Vec<f64>;
    fn linearize(&mut self, x: &[f64]) -> Result<Vec<f64>, DspError>;
    fn jvp(&self, direction: &[f64], out: &mut [f64]) -> Result<(), DspError>;
    fn vjp(&self, residual_direction: &[f64], out: &mut [f64]) -> Result<(), DspError>;
}

#[derive(Debug, Clone, Copy)]
pub struct OperatorOptions {
    pub max_nfev: usize,
    /// Maximum products with JᵀJ in one trust-region subproblem.
    pub max_cg_iterations: usize,
    /// Relative preconditioned residual tolerance of the CG subproblem.
    pub cg_rtol: f64,
    pub x_scale: XScale,
    pub ftol: f64,
    pub xtol: f64,
    pub gtol: f64,
}

impl Default for OperatorOptions {
    fn default() -> Self {
        Self {
            max_nfev: 100,
            max_cg_iterations: 64,
            cg_rtol: 1e-3,
            x_scale: XScale::Jac,
            ftol: 1e-8,
            xtol: 1e-8,
            gtol: 1e-8,
        }
    }
}

fn checked_linearization<P: JacobianOperator>(
    problem: &mut P,
    x: &[f64],
    f: &[f64],
) -> Result<(Vec<f64>, Vec<f64>), DspError> {
    let diagonal = problem.linearize(x)?;
    if diagonal.len() != x.len() || diagonal.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err(crate::invalid("invalid operator Jacobian diagonal"));
    }
    let mut gradient = vec![0.0; x.len()];
    problem.vjp(f, &mut gradient)?;
    if gradient.iter().any(|v| !v.is_finite()) {
        return Err(crate::invalid(
            "nonfinite operator Jacobian transpose product",
        ));
    }
    Ok((diagonal, gradient))
}

fn jac_scale(diagonal: &[f64], old: Option<&[f64]>) -> Vec<f64> {
    diagonal
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let s = v.sqrt();
            match old {
                Some(old) => s.max(old[i]),
                None if s == 0.0 => 1.0,
                None => s,
            }
        })
        .collect()
}

struct Model<'a, P> {
    problem: &'a P,
    d: &'a [f64],
    diagonal: &'a [f64],
    gradient: &'a [f64],
    rows: usize,
}

impl<P: JacobianOperator> Model<'_, P> {
    fn product(&self, s: &[f64]) -> Result<Vec<f64>, DspError> {
        let physical: Vec<_> = s.iter().zip(self.d).map(|(s, d)| s * d).collect();
        let mut jv = vec![0.0; self.rows];
        self.problem.jvp(&physical, &mut jv)?;
        if jv.iter().any(|v| !v.is_finite()) {
            return Err(crate::invalid("nonfinite operator Jacobian product"));
        }
        let mut out = vec![0.0; s.len()];
        self.problem.vjp(&jv, &mut out)?;
        for i in 0..s.len() {
            out[i] = self.d[i] * out[i] + self.diagonal[i] * s[i];
        }
        if out.iter().any(|v| !v.is_finite()) {
            return Err(crate::invalid("nonfinite operator trust-region model"));
        }
        Ok(out)
    }

    fn value(&self, s: &[f64]) -> Result<f64, DspError> {
        Ok(0.5 * dot(s, &self.product(s)?) + dot(s, self.gradient))
    }

    fn line(&self, s: &[f64], s0: Option<&[f64]>) -> Result<(f64, f64, f64), DspError> {
        let hs = self.product(s)?;
        let a = 0.5 * dot(s, &hs);
        let mut b = dot(self.gradient, s);
        let mut c = 0.0;
        if let Some(s0) = s0 {
            b += dot(s0, &hs);
            c = self.value(s0)?;
        }
        Ok((a, b, c))
    }
}

/// Positive intersection with ||s + tau p|| = delta. The alternate form when
/// b >= 0 avoids subtracting nearly equal roots at the trust-region boundary.
fn to_boundary(s: &[f64], p: &[f64], delta: f64) -> f64 {
    let a = dot(p, p);
    let b = dot(s, p);
    let c = dot(s, s) - delta * delta;
    let root = (b * b - a * c).max(0.0).sqrt();
    if b >= 0.0 {
        if root + b == 0.0 {
            0.0
        } else {
            -c / (root + b)
        }
    } else {
        (root - b) / a
    }
}

fn truncated_cg<P: JacobianOperator>(
    model: &Model<'_, P>,
    column_norms: &[f64],
    delta: f64,
    options: &OperatorOptions,
) -> Result<Vec<f64>, DspError> {
    let n = model.gradient.len();
    let preconditioner: Vec<_> = (0..n)
        .map(|i| {
            let v = model.d[i] * model.d[i] * column_norms[i] + model.diagonal[i];
            if v > 0.0 { v } else { 1.0 }
        })
        .collect();
    let mut s = vec![0.0; n];
    let mut r: Vec<_> = model.gradient.iter().map(|g| -g).collect();
    let mut z: Vec<_> = r.iter().zip(&preconditioner).map(|(r, d)| r / d).collect();
    let mut direction = z.clone();
    let mut rz = dot(&r, &z);
    if !rz.is_finite() || rz < 0.0 {
        return Err(crate::invalid("invalid operator CG preconditioner"));
    }
    let threshold = options.cg_rtol * options.cg_rtol * rz;
    if rz == 0.0 {
        return Ok(s);
    }
    for _ in 0..options.max_cg_iterations.min(n) {
        let hd = model.product(&direction)?;
        let curvature = dot(&direction, &hd);
        if curvature <= 0.0 {
            let tau = to_boundary(&s, &direction, delta);
            for i in 0..n {
                s[i] += tau * direction[i];
            }
            return Ok(s);
        }
        let alpha = rz / curvature;
        let next: Vec<_> = (0..n).map(|i| s[i] + alpha * direction[i]).collect();
        if norm(&next) >= delta {
            let tau = to_boundary(&s, &direction, delta);
            for i in 0..n {
                s[i] += tau * direction[i];
            }
            return Ok(s);
        }
        s = next;
        for i in 0..n {
            r[i] -= alpha * hd[i];
            z[i] = r[i] / preconditioner[i];
        }
        let next_rz = dot(&r, &z);
        if !next_rz.is_finite() || next_rz < 0.0 {
            return Err(crate::invalid("nonfinite operator CG iteration"));
        }
        if next_rz <= threshold {
            return Ok(s);
        }
        let beta = next_rz / rz;
        for i in 0..n {
            direction[i] = z[i] + beta * direction[i];
        }
        rz = next_rz;
    }
    Ok(s)
}

// The same three candidates as the dense bounded solver: feasible truncated
// step, reflection at the first hit bound, and scaled anti-gradient line.
#[allow(clippy::too_many_arguments)]
fn select<P: JacobianOperator>(
    x: &[f64],
    model: &Model<'_, P>,
    mut ph: Vec<f64>,
    delta: f64,
    lb: &[f64],
    ub: &[f64],
    theta: f64,
) -> Result<(Vec<f64>, Vec<f64>, f64), DspError> {
    let mut p: Vec<_> = ph.iter().zip(model.d).map(|(p, d)| p * d).collect();
    if (0..x.len()).all(|i| x[i] + p[i] >= lb[i] && x[i] + p[i] <= ub[i]) {
        let value = model.value(&ph)?;
        return Ok((p, ph, -value));
    }
    let (stride, hits) = bound_step(x, &p, lb, ub);
    let mut rh: Vec<_> = ph
        .iter()
        .zip(hits)
        .map(|(v, hit)| if hit { -v } else { *v })
        .collect();
    let r: Vec<_> = rh.iter().zip(model.d).map(|(a, b)| a * b).collect();
    for v in &mut p {
        *v *= stride;
    }
    for v in &mut ph {
        *v *= stride;
    }
    let xb: Vec<_> = x.iter().zip(&p).map(|(x, p)| x + p).collect();
    let tr = to_boundary(&ph, &rh, delta);
    let (bound, _) = bound_step(&xb, &r, lb, ub);
    let rs = bound.min(tr);
    let (lo, hi) = if rs > 0.0 {
        (
            (1.0 - theta) * stride / rs,
            if rs == bound { theta * bound } else { tr },
        )
    } else {
        (0.0, -1.0)
    };
    let mut reflected_value = f64::INFINITY;
    if lo <= hi {
        let (t, value) = minimize(model.line(&rh, Some(&ph))?, lo, hi);
        reflected_value = value;
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
    let step_value = model.value(&ph)?;
    let mut ah: Vec<_> = model.gradient.iter().map(|v| -v).collect();
    let mut ag: Vec<_> = ah.iter().zip(model.d).map(|(a, b)| a * b).collect();
    let tr = delta / norm(&ah);
    let (bound, _) = bound_step(x, &ag, lb, ub);
    let upper = if bound < tr { theta * bound } else { tr };
    let (stride, gradient_value) = minimize(model.line(&ah, None)?, 0.0, upper);
    for v in &mut ah {
        *v *= stride;
    }
    for v in &mut ag {
        *v *= stride;
    }
    if step_value < reflected_value && step_value < gradient_value {
        Ok((p, ph, -step_value))
    } else if reflected_value < step_value && reflected_value < gradient_value {
        Ok((
            rh.iter().zip(model.d).map(|(a, b)| a * b).collect(),
            rh,
            -reflected_value,
        ))
    } else {
        Ok((ag, ah, -gradient_value))
    }
}

/// Bounded trust-region least squares using only Jv and Jᵀv products.
///
/// This is an explicit alternative to the exact dense solvers. A failed or
/// nonfinite linearization/product returns `Err` so callers can retain their
/// dense fallback. Rejected nonfinite trial residuals shrink the trust radius.
/// All arithmetic is f64 and all reductions are deterministic and sequential.
pub fn least_squares_operator<P: JacobianOperator>(
    problem: &mut P,
    x0: &[f64],
    lb: &[f64],
    ub: &[f64],
    options: &OperatorOptions,
) -> Result<LeastSquaresResult, DspError> {
    let n = x0.len();
    let tolerances = [options.ftol, options.xtol, options.gtol];
    if n == 0
        || lb.len() != n
        || ub.len() != n
        || options.max_nfev == 0
        || options.max_cg_iterations == 0
        || !options.cg_rtol.is_finite()
        || options.cg_rtol <= 0.0
        || options.cg_rtol >= 1.0
        || tolerances.iter().any(|v| !v.is_finite() || *v < 0.0)
        || tolerances.iter().all(|v| *v <= f64::EPSILON)
        || !lb.iter().chain(ub).any(|v| v.is_finite())
        || (0..n).any(|i| {
            !x0[i].is_finite()
                || lb[i].is_nan()
                || ub[i].is_nan()
                || lb[i] >= ub[i]
                || x0[i] < lb[i]
                || x0[i] > ub[i]
        })
    {
        return Err(crate::invalid("invalid operator TRF start/bounds/options"));
    }
    let mut x = x0.to_vec();
    feasible(&mut x, lb, ub, true);
    let mut f = problem.residual(&x);
    let rows = f.len();
    if rows == 0 || f.iter().any(|v| !v.is_finite()) {
        return Err(crate::invalid("invalid initial operator residual"));
    }
    let (mut column_norms, mut gradient) = checked_linearization(problem, &x, &f)?;
    let mut scale_inv = match options.x_scale {
        XScale::Unit => vec![1.0; n],
        XScale::Jac => jac_scale(&column_norms, None),
    };
    let (mut v, dv) = scaling(&x, &gradient, lb, ub);
    for i in 0..n {
        if dv[i] != 0.0 {
            v[i] *= scale_inv[i];
        }
    }
    let mut delta = norm(
        &(0..n)
            .map(|i| x[i] * scale_inv[i] / v[i].sqrt())
            .collect::<Vec<_>>(),
    );
    if delta == 0.0 {
        delta = 1.0;
    }
    let mut cost = 0.5 * dot(&f, &f);
    if !cost.is_finite() || !delta.is_finite() {
        return Err(crate::invalid("nonfinite initial operator cost/radius"));
    }
    let (mut nfev, mut njev, mut status) = (1, 1, 0);
    let optimality = loop {
        let (mut v, dv) = scaling(&x, &gradient, lb, ub);
        let gn = gradient
            .iter()
            .zip(&v)
            .map(|(g, v)| (g * v).abs())
            .fold(0.0, f64::max);
        if gn < options.gtol {
            status = 1;
        }
        if status != 0 || nfev == options.max_nfev {
            break gn;
        }
        for i in 0..n {
            if dv[i] != 0.0 {
                v[i] *= scale_inv[i];
            }
        }
        let d: Vec<_> = (0..n).map(|i| v[i].sqrt() / scale_inv[i]).collect();
        let diagonal: Vec<_> = (0..n).map(|i| gradient[i] * dv[i] / scale_inv[i]).collect();
        let gh: Vec<_> = (0..n).map(|i| d[i] * gradient[i]).collect();
        if d.iter().chain(&diagonal).chain(&gh).any(|v| !v.is_finite()) {
            return Err(crate::invalid("nonfinite operator TRF scaling"));
        }
        let theta = 0.995_f64.max(1.0 - gn);
        let mut actual = -1.0;
        let mut candidate = x.clone();
        let mut candidate_f = f.clone();
        let mut candidate_cost = cost;
        while actual <= 0.0 && nfev < options.max_nfev {
            let model = Model {
                problem: &*problem,
                d: &d,
                diagonal: &diagonal,
                gradient: &gh,
                rows,
            };
            let ph = truncated_cg(&model, &column_norms, delta, options)?;
            let (step, sh, predicted) = select(&x, &model, ph, delta, lb, ub, theta)?;
            if !predicted.is_finite() || step.iter().any(|v| !v.is_finite()) {
                return Err(crate::invalid("nonfinite operator trust-region step"));
            }
            candidate = x.iter().zip(&step).map(|(x, s)| x + s).collect();
            feasible(&mut candidate, lb, ub, false);
            candidate_f = problem.residual(&candidate);
            nfev += 1;
            if candidate_f.len() != rows {
                return Err(crate::invalid("residual length changed"));
            }
            let shn = norm(&sh);
            if candidate_f.iter().any(|v| !v.is_finite()) {
                delta = 0.25 * shn;
                continue;
            }
            candidate_cost = 0.5 * dot(&candidate_f, &candidate_f);
            actual = cost - candidate_cost;
            let ratio = if predicted > 0.0 {
                actual / predicted
            } else if predicted == 0.0 && actual == 0.0 {
                1.0
            } else {
                0.0
            };
            let next_delta = if ratio < 0.25 {
                0.25 * shn
            } else if ratio > 0.75 && shn > 0.95 * delta {
                2.0 * delta
            } else {
                delta
            };
            status = termination(actual, cost, &step, &x, ratio, options.ftol, options.xtol);
            if status != 0 {
                break;
            }
            delta = next_delta;
            if delta <= 0.0 || !delta.is_finite() {
                return Err(crate::invalid("operator trust radius exhausted"));
            }
        }
        if actual > 0.0 {
            x = candidate;
            f = candidate_f;
            cost = candidate_cost;
            (column_norms, gradient) = checked_linearization(problem, &x, &f)?;
            njev += 1;
            if options.x_scale == XScale::Jac {
                scale_inv = jac_scale(&column_norms, Some(&scale_inv));
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Linear {
        a: Vec<Vec<f64>>,
        b: Vec<f64>,
    }
    impl JacobianOperator for Linear {
        fn residual(&mut self, x: &[f64]) -> Vec<f64> {
            self.a
                .iter()
                .zip(&self.b)
                .map(|(a, b)| dot(a, x) - b)
                .collect()
        }
        fn linearize(&mut self, x: &[f64]) -> Result<Vec<f64>, DspError> {
            Ok((0..x.len())
                .map(|j| self.a.iter().map(|a| a[j] * a[j]).sum())
                .collect())
        }
        fn jvp(&self, v: &[f64], out: &mut [f64]) -> Result<(), DspError> {
            for (out, a) in out.iter_mut().zip(&self.a) {
                *out = dot(a, v);
            }
            Ok(())
        }
        fn vjp(&self, v: &[f64], out: &mut [f64]) -> Result<(), DspError> {
            for (j, out) in out.iter_mut().enumerate() {
                *out = self.a.iter().zip(v).map(|(a, v)| a[j] * v).sum();
            }
            Ok(())
        }
    }

    #[test]
    fn operator_matches_dense_bounded_solution() {
        let mut problem = Linear {
            a: vec![vec![1.0, 0.5], vec![-0.2, 2.0], vec![0.4, -0.1]],
            b: vec![-2.0, 4.0, -1.0],
        };
        let (x, lb, ub) = ([0.4, 0.6], [0.0, 0.0], [1.0, 1.0]);
        let dense =
            super::super::least_squares(|x| problem.residual(x), &x, &lb, &ub, 100).unwrap();
        let iterative =
            least_squares_operator(&mut problem, &x, &lb, &ub, &OperatorOptions::default())
                .unwrap();
        assert!(
            iterative
                .x
                .iter()
                .zip(dense.x)
                .all(|(a, b)| (a - b).abs() < 1e-7)
        );
        assert!((iterative.cost - dense.cost).abs() < 1e-8);
        assert!(
            iterative
                .x
                .iter()
                .zip(lb)
                .zip(ub)
                .all(|((&x, l), u)| x >= l && x <= u)
        );
    }

    #[test]
    fn operator_handles_rank_deficiency_and_invalid_options() {
        let mut problem = Linear {
            a: vec![vec![1.0, 1.0, 0.0], vec![2.0, 2.0, 0.0]],
            b: vec![3.0, 6.0],
        };
        let (x, lb, ub) = ([0.0, 0.0, 0.7], [-10.0; 3], [10.0; 3]);
        let options = OperatorOptions::default();
        let result = least_squares_operator(&mut problem, &x, &lb, &ub, &options).unwrap();
        assert!((result.x[0] + result.x[1] - 3.0).abs() < 1e-7);
        assert_eq!(result.x[2], 0.7);
        assert!(
            least_squares_operator(
                &mut problem,
                &x,
                &lb,
                &ub,
                &OperatorOptions {
                    max_cg_iterations: 0,
                    ..options
                }
            )
            .is_err()
        );
    }
}
