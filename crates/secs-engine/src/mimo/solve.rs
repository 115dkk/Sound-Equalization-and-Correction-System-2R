//! Sequential Cholesky, falling back to partial-pivot LU.

use crate::EngineError;
use faer::{
    Mat, Par,
    dyn_stack::{MemBuffer, MemStack},
    linalg::{cholesky::llt, lu::partial_pivoting as lu},
};

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
