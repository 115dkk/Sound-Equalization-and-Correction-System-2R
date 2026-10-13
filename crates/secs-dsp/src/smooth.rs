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
    bounds(f, frac)
        .map(|bound| bound.map(|(lo, hi)| (c[hi] - c[lo]) / (hi - lo) as f64))
        .collect()
}

/// Reusable bin bounds for repeated smoothing on the same frequency grid.
#[derive(Debug)]
pub struct PowerSmoother {
    bounds: Vec<(usize, usize)>,
}

impl PowerSmoother {
    /// Records the original inclusive floor/ceil bounds, rejecting the same
    /// invalid broadcast shapes and out-of-grid upper bounds as `smooth_power`.
    pub fn new(f: &[f64], frac: Fraction<'_>) -> Result<Self, DspError> {
        if f.len() < 2 {
            return Err(invalid("smoothing requires at least two bins"));
        }
        if let Fraction::PerBin(x) = frac
            && x.len() != f.len()
            && x.len() != 1
        {
            return Err(invalid("fraction cannot broadcast to frequency grid"));
        }
        Ok(Self {
            bounds: bounds(f, frac).collect::<Result<_, _>>()?,
        })
    }

    /// Smooths a matching power array with the original sequential prefix sum.
    pub fn apply(&self, power: &[f64]) -> Result<Vec<f64>, DspError> {
        if power.len() != self.bounds.len() {
            return Err(invalid("smoothing requires matching arrays"));
        }
        let mut c = Vec::with_capacity(power.len() + 1);
        c.push(0.0);
        for p in power {
            c.push(c.last().unwrap() + p);
        }
        Ok(self
            .bounds
            .iter()
            .map(|&(lo, hi)| (c[hi] - c[lo]) / (hi - lo) as f64)
            .collect())
    }
}

fn bounds<'a>(
    f: &'a [f64],
    frac: Fraction<'a>,
) -> impl Iterator<Item = Result<(usize, usize), DspError>> + 'a {
    let df = f[1] - f[0];
    // NumPy float->int casts NaN/inf/out-of-range to INT64_MIN, then clips.
    let index = move |x: f64| -> usize {
        if !x.is_finite() || x >= i64::MAX as f64 || x < i64::MIN as f64 {
            0
        } else {
            (x as i64).clamp(0, (f.len() - 1) as i64) as usize
        }
    };
    let scalar_h = match frac {
        Fraction::Scalar(b) => Some(2_f64.powf(0.5 / b)),
        Fraction::PerBin(b) if b.len() == 1 => Some(2_f64.powf(0.5 / b[0])),
        Fraction::PerBin(_) => None,
    };
    let f_len = f.len();
    f.iter().enumerate().map(move |(i, &f)| {
        let h = scalar_h.unwrap_or_else(|| match frac {
            Fraction::PerBin(x) => 2_f64.powf(0.5 / x[i]),
            Fraction::Scalar(_) => unreachable!(),
        });
        let lo = index((f / h / df).floor());
        let hi = index((f * h / df).ceil()).max(lo + 1);
        if hi >= f_len {
            return Err(invalid("smoothing upper bound exceeds cumulative sum"));
        }
        Ok((lo, hi + 1))
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_bounds_match_smoothing_bits_and_errors() {
        let f: Vec<_> = (0..1024).map(|i| i as f64 * 0.366).collect();
        let per_bin: Vec<_> = f.iter().map(|f| 6.0 + f / 100.0).collect();
        for fraction in [
            Fraction::Scalar(12.0),
            Fraction::PerBin(&[24.0]),
            Fraction::PerBin(&per_bin),
        ] {
            let cache = PowerSmoother::new(&f, fraction).unwrap();
            for offset in [0.0, 0.1, -0.1] {
                let power: Vec<_> = f.iter().map(|f| (f + offset).sin()).collect();
                let expected = smooth_power(&f, &power, fraction).unwrap();
                let actual = cache.apply(&power).unwrap();
                assert!(
                    actual
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| a.to_bits() == b.to_bits())
                );
            }
            assert!(cache.apply(&[0.0]).is_err());
        }
        assert!(PowerSmoother::new(&[], Fraction::Scalar(12.0)).is_err());
        assert!(PowerSmoother::new(&f, Fraction::PerBin(&[])).is_err());
        assert!(PowerSmoother::new(&f, Fraction::Scalar(-12.0)).is_err());
    }
}
