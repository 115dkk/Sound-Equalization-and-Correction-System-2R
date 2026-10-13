//! Adapted from Impulcifer-py313 `crates/impulcifer-dsp/src/stages/room_tuning2/primitives.rs`
//! (MIT, Copyright (c) 2024- 115dkk), corrected against SECS 2/SciPy.
//! See docs/THIRD_PARTY.md and ../THIRD_PARTY_LICENSES.txt.
//! Fractional-octave smoothing, including upstream's sequential cumsum.
use crate::{Complex64, DspError, invalid};

/// Scalar or per-bin fractional-octave denominator (24 means 1/24 octave).
#[derive(Debug, Clone, Copy)]
pub enum Fraction<'a> {
    Scalar(f64),
    PerBin(&'a [f64]),
}

/// Linear-grid mean over inclusive floor/ceil bounds. Shape and upstream
/// out-of-bounds errors return Err (not silently clamped at Nyquist).
pub fn smooth_power(f: &[f64], power: &[f64], frac: Fraction<'_>) -> Result<Vec<f64>, DspError> {
    if f.len() < 2 || f.len() != power.len() {
        return Err(invalid(
            "smoothing requires matching arrays of at least two bins",
        ));
    }
    if let Fraction::PerBin(x) = frac
        && x.len() != f.len()
        && x.len() != 1
    {
        return Err(invalid("fraction cannot broadcast to frequency grid"));
    }
    let mut c = Vec::with_capacity(power.len() + 1);
    c.push(0.0);
    for p in power {
        c.push(c.last().unwrap() + p);
    }
    let df = f[1] - f[0];
    // NumPy float->int casts NaN/inf/out-of-range to INT64_MIN, then clips.
    let index = |x: f64| -> usize {
        if !x.is_finite() || x >= i64::MAX as f64 || x < i64::MIN as f64 {
            0
        } else {
            (x as i64).clamp(0, (f.len() - 1) as i64) as usize
        }
    };
    f.iter()
        .enumerate()
        .map(|(i, &f)| {
            let b = match frac {
                Fraction::Scalar(x) => x,
                Fraction::PerBin(x) => x[if x.len() == 1 { 0 } else { i }],
            };
            let h = 2_f64.powf(0.5 / b);
            let lo = index((f / h / df).floor());
            let hi = index((f * h / df).ceil()).max(lo + 1);
            if hi + 1 >= c.len() {
                return Err(invalid("smoothing upper bound exceeds cumulative sum"));
            }
            Ok((c[hi + 1] - c[lo]) / (hi + 1 - lo) as f64)
        })
        .collect()
}

pub fn smooth_complex(
    f: &[f64],
    h: &[Complex64],
    frac: Fraction<'_>,
) -> Result<Vec<Complex64>, DspError> {
    let re = smooth_power(f, &h.iter().map(|h| h.re).collect::<Vec<_>>(), frac)?;
    let im = smooth_power(f, &h.iter().map(|h| h.im).collect::<Vec<_>>(), frac)?;
    Ok(re
        .into_iter()
        .zip(im)
        .map(|(re, im)| re + Complex64::new(0., 1.) * Complex64::new(im, 0.))
        .collect())
}
