//! Typed before/after metrics, ported from `scorecard.metrics`.

use crate::{
    C,
    decay::{energy, first_max, fit_slope, median, percentile},
};
use secs_dsp::{
    fft, interp,
    response::gd_curve,
    smooth::{Fraction, smooth_complex, smooth_power},
};
use std::f64::consts::PI;

/// The 17 scalars, named as upstream's keys. Percentages are on a 0–100 scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub level: f64,
    pub in1_20: f64,
    pub in2_20: f64,
    pub in1_200: f64,
    pub in2_200: f64,
    pub in1_2000: f64,
    pub in2_2000: f64,
    pub turns: f64,
    pub gd_20: f64,
    pub gd_100: f64,
    pub gd_500: f64,
    pub gd24_abs: f64,
    pub gd24_abs_hz: f64,
    pub gd24_max: f64,
    pub gd24_span: f64,
    pub gd_30: f64,
    pub pre_db: f64,
}

fn geomspace(start: f64, stop: f64, n: usize) -> Vec<f64> {
    let lo = start.log10();
    let step = (stop.log10() - lo) / (n - 1) as f64;
    let mut out: Vec<_> = (0..n)
        .map(|i| 10.0_f64.powf(i as f64 * step + lo))
        .collect();
    out[0] = start;
    out[n - 1] = stop;
    out
}

fn unwrap(phase: &mut [f64]) {
    let mut previous = phase[0];
    let mut correction = 0.0;
    for p in &mut phase[1..] {
        let d = *p - previous;
        previous = *p;
        let mut dm = (d + PI).rem_euclid(2.0 * PI) - PI;
        if dm == -PI && d > 0.0 {
            dm = PI;
        }
        if d.abs() >= PI {
            correction += dm - d;
        }
        *p += correction;
    }
}

fn gradient(y: &[f64], x: &[f64]) -> Vec<f64> {
    let n = x.len();
    let mut out = vec![0.0; n];
    out[0] = (y[1] - y[0]) / (x[1] - x[0]);
    out[n - 1] = (y[n - 1] - y[n - 2]) / (x[n - 1] - x[n - 2]);
    for i in 1..n - 1 {
        let d1 = x[i] - x[i - 1];
        let d2 = x[i + 1] - x[i];
        let a = -d2 / (d1 * (d1 + d2));
        let b = (d2 - d1) / (d1 * d2);
        let c = d1 / (d2 * (d1 + d2));
        out[i] = a * y[i - 1] + b * y[i] + c * y[i + 1];
    }
    out
}

fn band(x: &[f64], y: &[f64], lo: f64, hi: f64) -> Vec<f64> {
    x.iter()
        .zip(y)
        .filter(|(x, _)| **x >= lo && **x <= hi)
        .map(|(_, y)| *y)
        .collect()
}

/// `scorecard.metrics`. `h`, `f` and `target_db` have the same length, at least
/// two; `f` is the linear real-FFT frequency grid with a positive Nyquist.
pub fn metrics(h: &[C], f: &[f64], target_db: &[f64], norm: (f64, f64)) -> Metrics {
    assert!(h.len() >= 2 && h.len() == f.len() && f.len() == target_db.len());
    let power: Vec<_> = h
        .iter()
        .map(|h| {
            let a = h.re.hypot(h.im);
            a * a
        })
        .collect();
    let db: Vec<_> = smooth_power(f, &power, Fraction::Scalar(6.0))
        .unwrap()
        .iter()
        .zip(target_db)
        .map(|(p, t)| 10.0 * (p + 1e-30).log10() - t)
        .collect();
    let level = median(&band(f, &db, norm.0, norm.1));
    let dev: Vec<_> = db.iter().map(|d| d - level).collect();
    let mut percentages = Vec::new();
    for (lo, hi) in [(20.0, 200.0), (200.0, 2000.0), (2000.0, 16000.0)] {
        let values = band(f, &dev, lo, hi);
        for threshold in [1.0, 2.0] {
            percentages.push(
                100.0
                    * (values.iter().filter(|v| v.abs() <= threshold).count() as f64
                        / values.len() as f64),
            );
        }
    }
    let ir = fft::irfft(h, 2 * (h.len() - 1));
    let peak = first_max(&ir.iter().map(|v| v.abs()).collect::<Vec<_>>());
    let fs = 2.0 * f[f.len() - 1];
    let t = peak as f64 / fs;
    let shifted: Vec<_> = h
        .iter()
        .zip(f)
        .map(|(h, f)| h * C::from_polar(1.0, 0.0 + (2.0 * PI * f) * t))
        .collect();
    let hs = smooth_complex(f, &shifted, Fraction::Scalar(6.0)).unwrap();
    let re: Vec<_> = hs.iter().map(|h| h.re).collect();
    let im: Vec<_> = hs.iter().map(|h| h.im).collect();
    let g = geomspace(20.0, 20000.0, 2000);
    let mut phase: Vec<_> = g
        .iter()
        .map(|g| {
            (C::new(interp(*g, f, &re), 0.0) + C::new(0.0, 1.0) * C::new(interp(*g, f, &im), 0.0))
                .arg()
        })
        .collect();
    unwrap(&mut phase);
    let slope = fit_slope(
        &band(&g, &g, 1000.0, 10000.0),
        &band(&g, &phase, 1000.0, 10000.0),
    );
    for (p, g) in phase.iter_mut().zip(&g) {
        *p -= slope * g;
    }
    let turns = (phase.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - phase.iter().copied().fold(f64::INFINITY, f64::min))
        / (2.0 * PI);
    let x: Vec<_> = g.iter().map(|g| 2.0 * PI * g).collect();
    let gd: Vec<_> = gradient(&phase, &x).iter().map(|v| -v * 1000.0).collect();
    let span = |lo, hi| {
        let v = band(&g, &gd, lo, hi);
        percentile(&v, 90.0) - percentile(&v, 10.0)
    };
    let g24 = geomspace(20.0, 500.0, 1500);
    let gd24 = gd_curve(h, f, &g24, Fraction::Scalar(24.0)).unwrap();
    let k24 = first_max(&gd24);
    let gd24_abs = gd24[k24];
    let center = median(&gd24);
    let centered: Vec<_> = gd24.iter().map(|d| d - center).collect();
    let a3 = (0.003 * fs) as isize;
    let a50 = (0.05 * fs) as isize;
    let wrapped = |a: isize, b: isize| {
        energy(
            &(a..b)
                .map(|i| ir[i.rem_euclid(ir.len() as isize) as usize])
                .collect::<Vec<_>>(),
        )
    };
    let pre = wrapped(peak as isize - a50, peak as isize - a3);
    let main = wrapped(peak as isize - a3, peak as isize + a50);
    Metrics {
        level,
        in1_20: percentages[0],
        in2_20: percentages[1],
        in1_200: percentages[2],
        in2_200: percentages[3],
        in1_2000: percentages[4],
        in2_2000: percentages[5],
        turns,
        gd_20: span(20.0, 100.0),
        gd_100: span(100.0, 500.0),
        gd_500: span(500.0, 5000.0),
        gd24_abs,
        gd24_abs_hz: g24[k24],
        gd24_max: gd24_abs - center,
        gd24_span: percentile(&centered, 95.0) - percentile(&centered, 5.0),
        gd_30: span(30.0, 100.0),
        pre_db: 10.0 * (pre / (main + 1e-30) + 1e-30).log10(),
    }
}
