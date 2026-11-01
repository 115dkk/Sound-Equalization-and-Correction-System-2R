//! Oracle tests of the engine stages (docs/ENGINE.md §3).
//!
//! One file per golden family. `cases` loads the shared case set
//! (`oracle/goldens/engine_inputs`) that every family refers to by name.

mod calibration;
pub(crate) mod cancel;
pub(crate) use crate::test_support as cases;
mod chain;
mod decay;
mod design;
mod e2e;
pub(crate) mod fit;
mod hf_phase;
mod input_hp;
mod lf_align;
mod low_protect;
mod mimo;
mod mimo_iir;
mod mimo_run;
mod mode_decay;
mod prep;
mod preview;
mod rc;
mod rc_bands;
mod rc_upper;
mod replay;
mod replay_cached_mimo;
mod scorecard;
mod xo;

/// Largest `|a - b|` over the element pairs (TD-52). Equal pairs, equal
/// infinities included, count 0; a pair that differs where either side is
/// NaN or infinite counts as infinitely far. Folding `(a - b).abs()` with
/// `f64::max` instead drops NaN and reads an all-NaN result as exact.
pub(crate) fn max_abs_diff(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(&a, &b)| {
            if a == b {
                0.0
            } else if a.is_finite() && b.is_finite() {
                (a - b).abs()
            } else {
                f64::INFINITY
            }
        })
        .fold(0.0, f64::max)
}

#[test]
fn max_abs_diff_does_not_swallow_nan() {
    let nan = [f64::NAN; 3];
    let ones = [1.0; 3];
    // The fold the MIMO comparisons used reads an all-NaN result as exact.
    let fold = nan
        .iter()
        .zip(&ones)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max);
    assert_eq!(fold, 0.0);
    assert_eq!(max_abs_diff(&nan, &ones), f64::INFINITY);
    assert_eq!(max_abs_diff(&ones, &nan), f64::INFINITY);
    assert_eq!(max_abs_diff(&nan, &nan), f64::INFINITY);
    assert_eq!(max_abs_diff(&[1.0, f64::NAN], &[1.0, 2.0]), f64::INFINITY);
    assert_eq!(max_abs_diff(&[f64::INFINITY], &[1.0]), f64::INFINITY);
    assert_eq!(
        max_abs_diff(&[f64::INFINITY], &[f64::NEG_INFINITY]),
        f64::INFINITY
    );
    assert_eq!(
        max_abs_diff(&[f64::INFINITY, 1.0], &[f64::INFINITY, 1.5]),
        0.5
    );
    assert_eq!(max_abs_diff(&[1.0, -2.0], &[1.25, -2.0]), 0.25);
    assert_eq!(max_abs_diff(&[], &[]), 0.0);
}
