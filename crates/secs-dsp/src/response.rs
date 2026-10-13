//! SECS analog prototypes and unnormalized RBJ transfer functions.
//! Arithmetic helpers adapted from Impulcifer-py313
//! `crates/impulcifer-dsp/src/stages/room_tuning2/{primitives,peq}.rs`
//! (MIT, Copyright (c) 2024- 115dkk); see docs/THIRD_PARTY.md.
use crate::{
    Complex64 as C, DspError, fft, invalid,
    numeric::{divide, interp},
    smooth::{Fraction, smooth_complex},
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
    let t = peak as f64 / (2. * f[f.len() - 1]);
    let shifted: Vec<_> = h
        .iter()
        .zip(f)
        .map(|(&h, &f)| h * C::from_polar(1., 2. * PI * f * t))
        .collect();
    let s = smooth_complex(f, &shifted, frac)?;
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
