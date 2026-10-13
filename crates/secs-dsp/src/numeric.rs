//! Complex division adapted from Impulcifer-py313
//! `crates/impulcifer-dsp/src/stages/room_tuning2/peq.rs` (MIT,
//! Copyright (c) 2024- 115dkk); see docs/THIRD_PARTY.md.
//! NumPy arithmetic order used by the primitives.
use crate::Complex64;

/// NumPy's contiguous float64 pairwise reduction (128-element blocks).
pub fn sum(x: &[f64]) -> f64 {
    if x.len() < 8 {
        x.iter().fold(-0.0, |a, b| a + b)
    } else if x.len() <= 128 {
        let mut r: [f64; 8] = x[..8].try_into().unwrap();
        let stop = x.len() / 8 * 8;
        for i in (8..stop).step_by(8) {
            for j in 0..8 {
                r[j] += x[i + j];
            }
        }
        let mut s = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        for v in &x[stop..] {
            s += v;
        }
        s
    } else {
        let n = (x.len() / 2) / 8 * 8;
        sum(&x[..n]) + sum(&x[n..])
    }
}

// Adapted from Impulcifer-py313 crates/impulcifer-dsp/src/stages/room_tuning2/peq.rs
// (MIT, Copyright (c) 2024- 115dkk); see docs/THIRD_PARTY.md.
/// NumPy's complex division (Smith's scaled formula, including a division by
/// zero), so quotients match NumPy to the last bit.
pub fn divide(a: Complex64, b: Complex64) -> Complex64 {
    if b.re.abs() >= b.im.abs() {
        if b.re == 0.0 && b.im == 0.0 {
            return Complex64::new(a.re / b.re.abs(), a.im / b.re.abs());
        }
        let r = b.im / b.re;
        let d = 1. / (b.re + b.im * r);
        Complex64::new((a.re + a.im * r) * d, (a.im - a.re * r) * d)
    } else {
        let r = b.re / b.im;
        let d = 1. / (b.re * r + b.im);
        Complex64::new((a.re * r + a.im) * d, (a.im * r - a.re) * d)
    }
}

/// NumPy linear interpolation with endpoint clamping (nonempty sorted grid).
pub fn interp(x: f64, g: &[f64], v: &[f64]) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x < g[0] {
        return v[0];
    }
    if x > g[g.len() - 1] {
        return v[v.len() - 1];
    }
    let k = g.partition_point(|a| *a <= x).saturating_sub(1);
    if k == g.len() - 1 || x == g[k] {
        return v[k];
    }
    let slope = (v[k + 1] - v[k]) / (g[k + 1] - g[k]);
    let y = slope * (x - g[k]) + v[k];
    if !y.is_nan() {
        return y;
    }
    let y = slope * (x - g[k + 1]) + v[k + 1];
    if y.is_nan() && v[k] == v[k + 1] {
        v[k]
    } else {
        y
    }
}
