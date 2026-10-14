//! Sequential shifted solves, falling back to Cholesky and partial-pivot LU.

use crate::EngineError;
use faer::{
    Mat, Par,
    dyn_stack::{MemBuffer, MemStack},
    linalg::{cholesky::llt, lu::partial_pivoting as lu},
};

/// One sequential Householder reduction for all diagonal shifts of a system.
/// Keep the original system for the existing Cholesky/LU fallback.
pub(crate) struct RidgeSweep<'a> {
    ata: &'a [f64],
    atb: &'a [f64],
    reduction: Mat<f64>,
    householder: Mat<f64>,
    projected: Mat<f64>,
    finite: bool,
}

impl<'a> RidgeSweep<'a> {
    pub(crate) fn new(ata: &'a [f64], atb: &'a [f64]) -> Self {
        use faer::linalg::{evd::tridiag, householder, qr};
        let n = atb.len();
        let bs = qr::no_pivoting::factor::recommended_block_size::<f64>(n, n);
        let mut reduction = Mat::from_fn(n, n, |i, j| ata[i * n + j]);
        let mut reflectors = Mat::zeros(bs, n.saturating_sub(1));
        let mut projected = Mat::from_fn(n, 1, |i, _| atb[i]);
        let mut finite = ata.iter().chain(atb).all(|v| v.is_finite());
        if n > 1 && finite {
            let mut memory = MemBuffer::new(tridiag::tridiag_in_place_scratch::<f64>(
                n,
                Par::Seq,
                Default::default(),
            ));
            tridiag::tridiag_in_place(
                reduction.as_mut(),
                reflectors.as_mut(),
                Par::Seq,
                MemStack::new(&mut memory),
                Default::default(),
            );
            let mut memory = MemBuffer::new(
                householder::apply_block_householder_sequence_transpose_on_the_left_in_place_scratch::<f64>(n - 1, bs, 1),
            );
            householder::apply_block_householder_sequence_transpose_on_the_left_in_place_with_conj(
                reduction.as_ref().submatrix(1, 0, n - 1, n - 1),
                reflectors.as_ref(),
                faer::Conj::No,
                projected.as_mut().subrows_mut(1, n - 1),
                Par::Seq,
                MemStack::new(&mut memory),
            );
            finite = (0..n).all(|i| projected[(i, 0)].is_finite())
                && (0..n).all(|j| (j..n).all(|i| reduction[(i, j)].is_finite()));
        }
        Self {
            ata,
            atb,
            reduction,
            householder: reflectors,
            projected,
            finite,
        }
    }

    pub(crate) fn solve(&self, lambda: f64) -> Result<Vec<f64>, EngineError> {
        let Some(mut answer) = self.shifted(lambda) else {
            return solve(self.ata, self.atb, lambda, false);
        };
        // One deterministic refinement in the original coordinates removes
        // the Householder reduction's rounding error without refactorization.
        let n = self.atb.len();
        let residual: Vec<_> = self
            .ata
            .chunks_exact(n)
            .enumerate()
            .map(|(i, row)| {
                let mut dot = 0.0;
                for j in 0..n {
                    dot += (row[j] + if i == j { lambda } else { 0.0 }) * answer[j];
                }
                self.atb[i] - dot
            })
            .collect();
        let mut rhs = Mat::from_fn(n, 1, |i, _| residual[i]);
        if n > 1 {
            use faer::linalg::householder;
            let mut memory = MemBuffer::new(householder::apply_block_householder_sequence_transpose_on_the_left_in_place_scratch::<f64>(n-1, self.householder.nrows(), 1));
            householder::apply_block_householder_sequence_transpose_on_the_left_in_place_with_conj(
                self.reduction.as_ref().submatrix(1, 0, n - 1, n - 1),
                self.householder.as_ref(),
                faer::Conj::No,
                rhs.as_mut().subrows_mut(1, n - 1),
                Par::Seq,
                MemStack::new(&mut memory),
            );
        }
        let Some(correction) = self.shifted_rhs(lambda, rhs) else {
            return solve(self.ata, self.atb, lambda, false);
        };
        for (x, dx) in answer.iter_mut().zip(correction) {
            *x += dx;
        }
        if answer.iter().any(|x| !x.is_finite()) {
            return solve(self.ata, self.atb, lambda, false);
        }
        Ok(answer)
    }

    // None means this shift must use the original solver, not a repaired pivot.
    pub(crate) fn shifted(&self, lambda: f64) -> Option<Vec<f64>> {
        self.shifted_rhs(lambda, self.projected.clone())
    }

    fn shifted_rhs(&self, lambda: f64, mut rhs: Mat<f64>) -> Option<Vec<f64>> {
        use faer::linalg::householder;
        if !self.finite || !lambda.is_finite() {
            return None;
        }
        let n = self.atb.len();
        let mut pivots = vec![0.0; n];
        for i in 0..n {
            pivots[i] = self.reduction[(i, i)] + lambda;
            if i > 0 {
                let off = self.reduction[(i, i - 1)];
                let multiplier = off / pivots[i - 1];
                pivots[i] -= multiplier * off;
                rhs[(i, 0)] -= multiplier * rhs[(i - 1, 0)];
            }
            if !pivots[i].is_finite() || pivots[i] <= 0.0 {
                return None;
            }
        }
        for i in (0..n).rev() {
            rhs[(i, 0)] /= pivots[i];
            if i + 1 < n {
                rhs[(i, 0)] -= self.reduction[(i + 1, i)] / pivots[i] * rhs[(i + 1, 0)];
            }
        }
        if n > 1 {
            let mut memory = MemBuffer::new(
                householder::apply_block_householder_sequence_on_the_left_in_place_scratch::<f64>(
                    n - 1,
                    self.householder.nrows(),
                    1,
                ),
            );
            householder::apply_block_householder_sequence_on_the_left_in_place_with_conj(
                self.reduction.as_ref().submatrix(1, 0, n - 1, n - 1),
                self.householder.as_ref(),
                faer::Conj::No,
                rhs.as_mut().subrows_mut(1, n - 1),
                Par::Seq,
                MemStack::new(&mut memory),
            );
        }
        let answer: Vec<_> = (0..n).map(|i| rhs[(i, 0)]).collect();
        answer.iter().all(|v| v.is_finite()).then_some(answer)
    }
}

pub(crate) fn solve(
    ata: &[f64],
    atb: &[f64],
    lambda: f64,
    force_lu: bool,
) -> Result<Vec<f64>, EngineError> {
    let n = atb.len();
    let matrix = || {
        Mat::from_fn(n, n, |i, j| {
            ata[i * n + j] + if i == j { lambda } else { 0.0 }
        })
    };
    let mut a = matrix();
    let mut rhs = Mat::from_fn(n, 1, |i, _| atb[i]);
    let mut memory = MemBuffer::new(llt::factor::cholesky_in_place_scratch::<f64>(
        n,
        Par::Seq,
        Default::default(),
    ));
    let cholesky = !force_lu
        && llt::factor::cholesky_in_place(
            a.as_mut(),
            Default::default(),
            Par::Seq,
            MemStack::new(&mut memory),
            Default::default(),
        )
        .is_ok();
    if cholesky {
        let mut memory = MemBuffer::new(llt::solve::solve_in_place_scratch::<f64>(n, 1, Par::Seq));
        llt::solve::solve_in_place(
            a.as_ref(),
            rhs.as_mut(),
            Par::Seq,
            MemStack::new(&mut memory),
        );
    } else {
        a = matrix();
        let mut permutation = vec![0usize; n];
        let mut inverse = vec![0usize; n];
        let mut memory = MemBuffer::new(lu::factor::lu_in_place_scratch::<usize, f64>(
            n,
            n,
            Par::Seq,
            Default::default(),
        ));
        let (_, perm) = lu::factor::lu_in_place(
            a.as_mut(),
            &mut permutation,
            &mut inverse,
            Par::Seq,
            MemStack::new(&mut memory),
            Default::default(),
        );
        let mut memory = MemBuffer::new(lu::solve::solve_in_place_scratch::<usize, f64>(
            n,
            1,
            Par::Seq,
        ));
        lu::solve::solve_in_place(
            a.as_ref(),
            a.as_ref(),
            perm,
            rhs.as_mut(),
            Par::Seq,
            MemStack::new(&mut memory),
        );
    }
    let answer: Vec<_> = (0..n).map(|i| rhs[(i, 0)]).collect();
    if answer.iter().any(|v| !v.is_finite()) {
        return Err(EngineError::Input("singular MIMO normal equations".into()));
    }
    Ok(answer)
}
