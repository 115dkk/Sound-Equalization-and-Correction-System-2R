//! Caller-specific decay measures; their different windows and filters are
//! intentional upstream parity, not interchangeable definitions.
//! Upstream: `_ba_setup`, `calibration._late_db`, `mimo.mode_t60`, `_late_peaks`.

use crate::{C, grid::design_freqs};
use secs_dsp::{
    fft,
    iir::{BType, butter, sosfilt, sosfreqz},
    pairwise_sum,
    peaks::find_peaks,
};
use std::sync::OnceLock;

/// The low-frequency linear grid BA measures late energy on (`_ba_setup`).
#[derive(Debug, Clone)]
pub struct BaLateGrid {
    pub lin: usize,
    pub band: Vec<f64>,
    pub fs_eff: f64,
}

impl BaLateGrid {
    pub fn new(hi_hz: f64) -> Self {
        let f = design_freqs();
        let lin = f.partition_point(|f| *f < 200.0);
        let band = f[..lin]
            .iter()
            .map(|f| {
                ((f - 15.0) / 5.0).clamp(0.0, 1.0)
                    * ((hi_hz * 1.3 - f) / (hi_hz * 0.3)).clamp(0.0, 1.0)
            })
            .collect();
        Self {
            lin,
            band,
            fs_eff: 2.0 * f[lin - 1],
        }
    }
}

pub(crate) fn first_max(x: &[f64]) -> usize {
    let mut k = 0;
    for i in 1..x.len() {
        if x[i] > x[k] || (x[i].is_nan() && !x[k].is_nan()) {
            k = i;
        }
    }
    k
}

fn onset(x: &[f64]) -> usize {
    let top = x.iter().map(|v| v.abs()).fold(0.0, f64::max);
    x.iter().position(|v| v.abs() > 0.1 * top).unwrap_or(0)
}

pub(crate) fn energy(x: &[f64]) -> f64 {
    pairwise_sum(&x.iter().map(|v| v * v).collect::<Vec<_>>())
}

fn late_ratio(x: &[f64], fs: f64, late_start: f64, late_end: f64) -> f64 {
    let k = onset(x);
    let window = |a: f64, b: f64| {
        energy(&x[(k + (a * fs) as usize).min(x.len())..(k + (b * fs) as usize).min(x.len())])
    };
    10.0 * (window(late_start, late_end) / (window(0.0, 0.05) + 1e-30) + 1e-30).log10()
}

/// BA's closure: `spec` holds the first `grid.lin` design-grid bins.
pub fn ba_late_db(grid: &BaLateGrid, spec: &[C]) -> f64 {
    assert_eq!(spec.len(), grid.lin);
    let h: Vec<_> = spec.iter().zip(&grid.band).map(|(h, b)| h * b).collect();
    late_ratio(&fft::irfft(&h, 2 * (grid.lin - 1)), grid.fs_eff, 0.1, 0.38)
}

/// `calibration._late_db`: causal 20–150 Hz Butterworth late/early energy.
pub fn late_db_20_150(h: &[C]) -> f64 {
    static FILTER: OnceLock<Vec<C>> = OnceLock::new();
    let filter = FILTER.get_or_init(|| {
        let sos = butter(4, &[20.0, 150.0], BType::Bandpass, 48000.0).unwrap();
        sosfreqz(&sos, design_freqs(), 48000.0).unwrap()
    });
    assert_eq!(h.len(), filter.len());
    let h: Vec<_> = h.iter().zip(filter).map(|(h, b)| h * b).collect();
    late_ratio(&fft::irfft(&h, 2 * (h.len() - 1)), 48000.0, 0.15, 0.4)
}

/// Degree-one least squares with NumPy polyfit's column scaling. NumPy 2.5.3
/// calls LAPACK lstsq on [x, 1]/scale; a two-column QR solves the same full-rank
/// system here without forming the ill-conditioned unscaled normal equations.
pub(crate) fn fit_slope(x: &[f64], y: &[f64]) -> f64 {
    let scale = x.iter().fold(0.0, |s, x| s + x * x).sqrt();
    let a: Vec<_> = x.iter().map(|x| x / scale).collect();
    let anorm = energy(&a).sqrt();
    let q: Vec<_> = a.iter().map(|a| a / anorm).collect();
    let b = 1.0 / (x.len() as f64).sqrt();
    let cross = pairwise_sum(&q.iter().map(|q| q * b).collect::<Vec<_>>());
    let v: Vec<_> = q.iter().map(|q| b - cross * q).collect();
    let vnorm = energy(&v).sqrt();
    let y1 = pairwise_sum(&q.iter().zip(y).map(|(q, y)| q * y).collect::<Vec<_>>());
    let y2 = pairwise_sum(
        &v.iter()
            .zip(y)
            .map(|(v, y)| (v / vnorm) * y)
            .collect::<Vec<_>>(),
    );
    ((y1 - cross * (y2 / vnorm)) / anorm) / scale
}

/// `mimo.mode_t60`: reverse-time bandpass, Schroeder integral and −5…−25 dB fit.
/// Panics on invalid filter limits, where upstream raises ValueError.
pub fn mode_t60(ir: &[f64], rate: f64, center_hz: f64, half_band: f64) -> Option<f64> {
    let sos = butter(
        2,
        &[
            (center_hz - half_band).max(1.0),
            (center_hz + half_band).min(rate * 0.45),
        ],
        BType::Bandpass,
        rate,
    )
    .expect("invalid mode_t60 filter limits");
    let mut padded: Vec<_> = ir.iter().rev().copied().collect();
    padded.resize(padded.len() + (2.0 * rate) as usize, 0.0);
    let filtered = sosfilt(&sos, &padded).ok()?;
    let energy: Vec<_> = filtered.iter().rev().map(|v| v * v).collect();
    if energy.is_empty() {
        return None;
    }
    let start = first_max(&energy);
    let mut edc = energy[start..].to_vec();
    let mut total = 0.0;
    for e in edc.iter_mut().rev() {
        total += *e;
        *e = total;
    }
    if edc[0] <= 0.0 {
        return None;
    }
    let db: Vec<_> = edc
        .iter()
        .map(|e| 10.0 * (e / edc[0]).max(1e-30).log10())
        .collect();
    let hi = db.iter().position(|d| *d <= -5.0).unwrap_or(0);
    let lo = db.iter().position(|d| *d <= -25.0).unwrap_or(0);
    if lo <= hi + 2 {
        return None;
    }
    let x: Vec<_> = (hi..lo).map(|i| i as f64 / rate).collect();
    let slope = fit_slope(&x, &db[hi..lo]);
    (slope < 0.0).then(|| -60.0 / slope)
}

/// `mimo._late_peaks`: 600 Hz IRs, arrival +150 ms, 4096-point spectra.
pub fn late_peaks(xs: &[&[f64]], count: usize) -> Vec<f64> {
    let mut spec = vec![0.0; 2049];
    for x in xs {
        let k = onset(x) + 90;
        let mut late = vec![0.0; 4096];
        let end = x.len().min(4096);
        if k < end {
            late[k..end].copy_from_slice(&x[k..end]);
        }
        for (s, h) in spec.iter_mut().zip(fft::rfft(&late)) {
            let abs = h.re.hypot(h.im);
            *s += abs * abs;
        }
    }
    let fr = secs_dsp::grid::freqs(4096, 600.0).unwrap();
    let masked: Vec<_> = spec
        .iter()
        .zip(&fr)
        .map(|(s, f)| if (20.0..=160.0).contains(f) { *s } else { 0.0 })
        .collect();
    let mut indices = find_peaks(&masked, None);
    indices.sort_by(|&a, &b| spec[b].total_cmp(&spec[a]));
    indices.into_iter().take(count).map(|i| fr[i]).collect()
}

pub(crate) fn percentile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() || values.iter().any(|v| v.is_nan()) {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = (sorted.len() - 1) as f64 * (q / 100.0);
    let lo = index.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let t = index - lo as f64;
    let d = sorted[hi] - sorted[lo];
    if t >= 0.5 {
        sorted[hi] - d * (1.0 - t)
    } else {
        sorted[lo] + d * t
    }
}

pub(crate) fn median(values: &[f64]) -> f64 {
    if values.is_empty() || values.iter().any(|v| v.is_nan()) {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let k = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[k - 1] + sorted[k]) / 2.0
    } else {
        sorted[k]
    }
}
