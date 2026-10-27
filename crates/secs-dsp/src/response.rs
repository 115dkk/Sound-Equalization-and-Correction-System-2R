//! SECS analog prototypes and unnormalized RBJ transfer functions.
//! Arithmetic helpers adapted from Impulcifer-py313
//! `crates/impulcifer-dsp/src/stages/room_tuning2/{primitives,peq}.rs`
//! (MIT, Copyright (c) 2024- 115dkk); see docs/THIRD_PARTY.md.
use crate::{
    Complex64 as C, DspError, fft, invalid,
    numeric::{divide, interp},
    smooth::{Fraction, smooth_complex, smooth_complex_prefix},
};
use std::f64::consts::PI;

pub const BAND_TYPES: [(&str, (&str, usize)); 5] = [
    ("LR24", ("LR", 4)),
    ("LR12", ("LR", 2)),
    ("LR48", ("LR", 8)),
    ("BW12", ("BW", 2)),
    ("BW24", ("BW", 4)),
];

/// As upstream, every kind other than "LP" selects high-pass.
pub fn lr4(f: &[f64], fc: f64, kind: &str) -> Vec<C> {
    f.iter()
        .map(|&f| {
            let s = C::new(0., f) * (1. / fc);
            let bw = divide(C::new(1., 0.), s * s + 2_f64.sqrt() * s + 1.);
            let h = if kind == "LP" { bw } else { (s * s) * bw };
            h * h
        })
        .collect()
}
pub fn hp2(f: &[f64], fc: f64) -> Vec<C> {
    f.iter()
        .map(|&f| {
            let s = C::new(0., f) * (1. / fc);
            divide(s * s, s * s + 2_f64.sqrt() * s + 1.)
        })
        .collect()
}
pub fn band_filter(f: &[f64], fc: f64, kind: &str, typ: &str) -> Result<Vec<C>, DspError> {
    if typ == "LR24" {
        return Ok(lr4(f, fc, kind));
    }
    let &(_, (shape, order)) = BAND_TYPES
        .iter()
        .find(|(name, _)| *name == typ)
        .ok_or_else(|| invalid("unknown band filter type"))?;
    let n = if shape == "LR" { order / 2 } else { order };
    // scipy.buttap followed by np.poly, in prototype pole order.
    let mut a = vec![C::new(1., 0.)];
    for i in 0..n {
        let m = -(n as isize - 1) + 2 * i as isize;
        let p = -C::from_polar(1., PI * m as f64 / (2. * n as f64));
        let mut next = vec![C::new(0., 0.); a.len() + 1];
        for (k, &v) in a.iter().enumerate() {
            next[k] += v;
            next[k + 1] -= p * v;
        }
        a = next;
    }
    let a: Vec<_> = a.iter().map(|x| x.re).collect();
    Ok(f.iter()
        .map(|&f| {
            let s = C::new(0., f) * (1. / fc);
            let den = a.iter().fold(C::new(0., 0.), |v, &a| v * s + a);
            let num = if kind == "LP" {
                C::new(1., 0.)
            } else {
                (0..n).fold(C::new(1., 0.), |v, _| v * s)
            };
            let h = divide(num, den);
            if shape == "LR" { h * h } else { h }
        })
        .collect())
}
pub fn allpass2(f: &[f64], f0: f64, q: f64) -> Vec<C> {
    f.iter()
        .map(|&f| {
            let s = C::new(0., f) * (1. / f0);
            divide(s * s - s * (1. / q) + 1., s * s + s * (1. / q) + 1.)
        })
        .collect()
}
pub fn peaking_response(f: &[f64], f0: f64, q: f64, gain_db: f64, sr: f64) -> Vec<C> {
    let z: Vec<_> = f
        .iter()
        .map(|&f| C::from_polar(1., (-2. * PI * f) * (1. / sr)))
        .collect();
    peaking_response_z(&z, f0, q, gain_db, sr)
}
pub fn peaking_response_z(z: &[C], f0: f64, q: f64, gain_db: f64, sr: f64) -> Vec<C> {
    let a = 10_f64.powf(gain_db / 40.);
    let w = 2. * PI * f0 / sr;
    let al = w.sin() / (2. * q);
    let b = [1. + al * a, -2. * w.cos(), 1. - al * a];
    let d = [1. + al / a, -2. * w.cos(), 1. - al / a];
    z.iter()
        .map(|&z| {
            divide(
                b[0] + b[1] * z + b[2] * z * z,
                d[0] + d[1] * z + d[2] * z * z,
            )
        })
        .collect()
}

/// Logarithmic transfer derivatives `(1/H) dH/d(ln(f0), ln(Q), gain_db)`.
///
/// The numerator and denominator use the same unnormalized RBJ arithmetic as
/// `peaking_response_z`. Dividing them separately avoids dividing a cascade by
/// a possibly very small band response. This is a complex logarithmic
/// derivative; no branch of the complex logarithm is evaluated.
pub fn peaking_log_derivatives_z(z: &[C], f0: f64, q: f64, gain_db: f64, sr: f64) -> [Vec<C>; 3] {
    let a = 10_f64.powf(gain_db / 40.0);
    let w = 2.0 * PI * f0 / sr;
    let alpha = w.sin() / (2.0 * q);
    let alpha_f = w * w.cos() / (2.0 * q);
    let middle_f = 2.0 * w * w.sin();
    let gain_f = 10_f64.ln() / 40.0;
    let b = [1.0 + alpha * a, -2.0 * w.cos(), 1.0 - alpha * a];
    let d = [1.0 + alpha / a, -2.0 * w.cos(), 1.0 - alpha / a];
    let mut out: [Vec<C>; 3] = std::array::from_fn(|_| Vec::with_capacity(z.len()));
    for &z in z {
        let num = b[0] + b[1] * z + b[2] * z * z;
        let den = d[0] + d[1] * z + d[2] * z * z;
        let u = C::new(1.0, 0.0) - z * z;
        let nf = alpha_f * a * u + middle_f * z;
        let df = alpha_f / a * u + middle_f * z;
        let nq = -alpha * a * u;
        let dq = -alpha / a * u;
        let ng = gain_f * alpha * a * u;
        let dg = -gain_f * alpha / a * u;
        for (column, (n, d)) in out.iter_mut().zip([(nf, df), (nq, dq), (ng, dg)]) {
            column.push(divide(n, num) - divide(d, den));
        }
    }
    out
}

/// Analog all-pass logarithmic derivatives with respect to `ln(f0), ln(Q)`.
/// These differentiate `allpass2`, including its analog frequency convention.
pub fn allpass_log_derivatives(f: &[f64], f0: f64, q: f64) -> [Vec<C>; 2] {
    let mut out: [Vec<C>; 2] = std::array::from_fn(|_| Vec::with_capacity(f.len()));
    for &f in f {
        let s = C::new(0.0, f) * (1.0 / f0);
        let s2 = s * s;
        let t = s * (1.0 / q);
        let num = s2 - t + 1.0;
        let den = s2 + t + 1.0;
        out[0].push(divide(-2.0 * s2 + t, num) - divide(-2.0 * s2 - t, den));
        out[1].push(divide(t, num) - divide(-t, den));
    }
    out
}

pub fn peq_response(f: &[f64], bands: &[(f64, f64, f64)]) -> Vec<C> {
    let z: Vec<_> = f
        .iter()
        .map(|&f| C::from_polar(1., (-2. * PI * f) * (1. / 48000.)))
        .collect();
    let mut out = vec![C::new(1., 0.); f.len()];
    for &(f0, q, g) in bands {
        for (o, h) in out.iter_mut().zip(peaking_response_z(&z, f0, q, g, 48000.)) {
            *o *= h;
        }
    }
    out
}

/// NumPy baseline complex absolute (scaled square root, not hypot).
pub fn magnitude(z: C) -> f64 {
    let re = z.re.abs();
    let im = z.im.abs();
    if re.is_infinite() || im.is_infinite() {
        return f64::INFINITY;
    }
    if re.is_nan() || im.is_nan() {
        return f64::NAN;
    }
    let big = re.max(im);
    if big == 0. {
        return 0.;
    }
    let r = re.min(im) / big;
    (r * r + 1.).sqrt() * big
}

/// Delay-removed complex-smoothed group delay in ms, as upstream gd_curve.
pub fn gd_curve(h: &[C], f: &[f64], g: &[f64], frac: Fraction<'_>) -> Result<Vec<f64>, DspError> {
    if h.len() < 2 || h.len() != f.len() || g.len() < 2 {
        return Err(invalid("invalid group-delay shape"));
    }
    let ir = fft::irfft(h, 2 * (h.len() - 1));
    let mut peak = 0;
    for i in 1..ir.len() {
        if ir[i].is_nan() && !ir[peak].is_nan() || ir[i].abs() > ir[peak].abs() {
            peak = i;
        }
    }
    gd_curve_with_peak(h, f, g, frac, peak)
}

/// `gd_curve` with an already known absolute IR peak. The peak must come from
/// `irfft(h, 2 * (h.len() - 1))`, with the same first-maximum/NaN convention.
/// Reusing it avoids another full inverse transform in scorecard callers.
pub fn gd_curve_with_peak(
    h: &[C],
    f: &[f64],
    g: &[f64],
    frac: Fraction<'_>,
    peak: usize,
) -> Result<Vec<f64>, DspError> {
    if h.len() < 2 || h.len() != f.len() || g.len() < 2 || peak >= 2 * (h.len() - 1) {
        return Err(invalid("invalid group-delay shape or peak"));
    }
    let t = peak as f64 / (2. * f[f.len() - 1]);
    // Interpolation needs at most the two neighboring bins. Keep the original
    // grid for smoothing bounds, and accumulate from bin zero so omitting high
    // frequencies does not change cancellation or special-value propagation.
    let regular = f.iter().all(|v| v.is_finite())
        && f.windows(2).all(|v| v[0] < v[1])
        && g.iter().all(|v| !v.is_nan());
    let end = if regular {
        let top = g.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (f.partition_point(|&v| v <= top) + 1).min(f.len())
    } else {
        f.len()
    };
    let s = if regular {
        smooth_complex_prefix(f, frac, end, |i| {
            h[i] * C::from_polar(1.0, 2.0 * PI * f[i] * t)
        })?
    } else {
        let shifted: Vec<_> = h
            .iter()
            .zip(f)
            .map(|(&h, &f)| h * C::from_polar(1.0, 2.0 * PI * f * t))
            .collect();
        smooth_complex(f, &shifted, frac)?
    };
    let f = &f[..end];
    let re: Vec<_> = s.iter().map(|h| h.re).collect();
    let im: Vec<_> = s.iter().map(|h| h.im).collect();
    let p: Vec<_> = g
        .iter()
        .map(|&g| (interp(g, f, &re) + C::new(0., 1.) * C::new(interp(g, f, &im), 0.)).arg())
        .collect();
    let mut phase = p.clone();
    let mut corr = 0.;
    for i in 1..p.len() {
        let d = p[i] - p[i - 1];
        let mut dm = (d + PI).rem_euclid(2. * PI) - PI;
        if dm == -PI && d > 0. {
            dm = PI;
        }
        let mut dc = dm - d;
        if d.abs() < PI {
            dc = 0.;
        }
        corr += dc;
        phase[i] += corr;
    }
    let x: Vec<_> = g.iter().map(|g| 2. * PI * g).collect();
    let n = g.len();
    let dx: Vec<_> = x.windows(2).map(|a| a[1] - a[0]).collect();
    let uniform = dx.iter().all(|d| *d == dx[0]);
    let mut out = vec![0.; n];
    out[0] = (phase[1] - phase[0]) / dx[0];
    out[n - 1] = (phase[n - 1] - phase[n - 2]) / dx[n - 2];
    for i in 1..n - 1 {
        out[i] = if uniform {
            (phase[i + 1] - phase[i - 1]) / (2. * dx[0])
        } else {
            let d1 = dx[i - 1];
            let d2 = dx[i];
            let a = -d2 / (d1 * (d1 + d2));
            let b = (d2 - d1) / (d1 * d2);
            let c = d1 / (d2 * (d1 + d2));
            a * phase[i - 1] + b * phase[i] + c * phase[i + 1]
        };
    }
    for v in &mut out {
        *v = -*v * 1000.;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaking_and_analog_allpass_analytic_derivatives_match_central_differences() {
        let f = [0.0, 15.0, 25.0, 49.0, 80.0, 120.0, 299.0, 2000.0, 24000.0];
        let z: Vec<_> = f
            .iter()
            .map(|&f| C::from_polar(1.0, (-2.0 * PI * f) / 48000.0))
            .collect();
        for (f0, q, gain) in [
            (15.0_f64, 0.3_f64, -25.0),
            (50.0, 10.0, 6.0),
            (120.0, 2.0, 0.0),
            (2000.0, 20.0, -3.0),
        ] {
            let parameters = [f0.ln(), q.ln(), gain];
            let h = peaking_response_z(&z, f0, q, gain, 48000.0);
            let d = peaking_log_derivatives_z(&z, f0, q, gain, 48000.0);
            for a in 0..3 {
                let (mut up, mut down) = (parameters, parameters);
                up[a] += 1e-5;
                down[a] -= 1e-5;
                let up = peaking_response_z(&z, up[0].exp(), up[1].exp(), up[2], 48000.0);
                let down = peaking_response_z(&z, down[0].exp(), down[1].exp(), down[2], 48000.0);
                let mut error = 0.0_f64;
                let mut scale = 1.0_f64;
                for i in 0..f.len() {
                    let analytic = h[i] * d[a][i];
                    error = error.max(magnitude(analytic - (up[i] - down[i]) / 2e-5));
                    scale = scale.max(magnitude(analytic));
                }
                assert!(
                    error / scale < 3e-6,
                    "PEQ f0={f0}, q={q}, gain={gain}, parameter={a}: {error}/{scale}"
                );
            }
            let h = allpass2(&f, f0, q);
            let d = allpass_log_derivatives(&f, f0, q);
            for a in 0..2 {
                let (mut up, mut down) = ([f0.ln(), q.ln()], [f0.ln(), q.ln()]);
                up[a] += 1e-6;
                down[a] -= 1e-6;
                let up = allpass2(&f, up[0].exp(), up[1].exp());
                let down = allpass2(&f, down[0].exp(), down[1].exp());
                for i in 0..f.len() {
                    let analytic = h[i] * d[a][i];
                    assert!(
                        magnitude(analytic - (up[i] - down[i]) / 2e-6)
                            < 2e-7 * magnitude(analytic).max(1.0)
                    );
                    assert!(
                        d[a][i].re.abs() < 1e-12,
                        "all-pass derivative must preserve magnitude"
                    );
                }
            }
        }
    }
}
