//! Display-only transforms from `step5_measure.magnitude_db` and the pre-run SPL preview.

use secs_dsp::{
    fft, interp, pairwise_sum,
    smooth::{Fraction, smooth_power},
};

/// Measurement plot's logarithmic 280-point grid, in Hz.
pub fn measurement_grid() -> Vec<f64> {
    crate::design::geomspace(5.0, 22000.0, 280)
}

/// Calibration SPL plot's 1390-point grid, in Hz.
pub fn spl_grid() -> Vec<f64> {
    crate::design::geomspace(5.0, 20000.0, 1390)
}

/// Calibration GD plot's 1070-point grid, in Hz.
pub fn gd_grid() -> Vec<f64> {
    crate::design::geomspace(5.0, 5000.0, 1070)
}

fn spectrum(samples: &[f64], sample_rate: f64, minimum: usize) -> (Vec<f64>, Vec<crate::C>) {
    let n = samples.len().max(minimum).next_power_of_two();
    let mut padded = vec![0.0; n];
    padded[..samples.len()].copy_from_slice(samples);
    let frequencies = (0..=n / 2)
        .map(|i| i as f64 * sample_rate / n as f64)
        .collect();
    (frequencies, fft::rfft(&padded))
}

/// Unwindowed measurement FFT, log interpolation and nearest-edge Gaussian
/// smoothing at 1/12 octave, before choosing a graph level reference.
pub fn smoothed_db(samples: &[f64], sample_rate: f64) -> Vec<f64> {
    let grid = measurement_grid();
    if samples.is_empty() {
        return vec![0.0; grid.len()];
    }
    let (frequencies, h) = spectrum(samples, sample_rate, 8192);
    let log_f: Vec<_> = frequencies.iter().map(|f| f.max(1e-6).log10()).collect();
    let db: Vec<_> = h
        .iter()
        .map(|v| 20.0 * v.re.hypot(v.im).max(1e-12).log10())
        .collect();
    let sampled: Vec<_> = grid
        .iter()
        .map(|f| interp(f.log10(), &log_f, &db))
        .collect();
    let sigma = (grid.len() - 1) as f64 / (grid[grid.len() - 1] / grid[0]).log2() / (12.0 * 2.3548);
    let radius = (4.0 * sigma + 0.5) as isize;
    let mut weights: Vec<_> = (-radius..=radius)
        .map(|k| (-0.5 * (k as f64 / sigma).powi(2)).exp())
        .collect();
    let total = pairwise_sum(&weights);
    weights.iter_mut().for_each(|w| *w /= total);
    (0..grid.len())
        .map(|i| {
            (-radius..=radius)
                .zip(&weights)
                .map(|(k, w)| {
                    sampled[(i as isize + k).clamp(0, sampled.len() as isize - 1) as usize] * w
                })
                .sum::<f64>()
        })
        .collect()
}

/// Mean of a measurement-grid curve over 200–2000 Hz.
pub fn midrange_reference(curve: &[f64]) -> f64 {
    let mid: Vec<_> = measurement_grid()
        .into_iter()
        .zip(curve)
        .filter(|(f, _)| (200.0..=2000.0).contains(f))
        .map(|(_, v)| *v)
        .collect();
    pairwise_sum(&mid) / mid.len() as f64
}

/// Smoothed measurement response with a zero mean over 200–2000 Hz, exactly
/// as upstream step 5 displays each curve independently.
pub fn magnitude_db(samples: &[f64], sample_rate: f64) -> Vec<f64> {
    let mut smoothed = smoothed_db(samples, sample_rate);
    let reference = midrange_reference(&smoothed);
    smoothed.iter_mut().for_each(|v| *v -= reference);
    smoothed
}

/// Pre-calibration position-one SPL, using the measurement's native sample rate
/// and a minimum 65536-sample FFT, with 1/24-octave power smoothing.
pub fn measurement_spl(samples: &[f64], sample_rate: f64) -> Vec<f64> {
    let (f, h) = spectrum(samples, sample_rate, 65536);
    let power: Vec<_> = h.iter().map(|v| v.re.hypot(v.im).powi(2)).collect();
    let db: Vec<_> = smooth_power(&f, &power, Fraction::Scalar(24.0))
        .expect("FFT grid")
        .iter()
        .map(|p| 10.0 * (p + 1e-30).log10())
        .collect();
    spl_grid().iter().map(|&g| interp(g, &f, &db)).collect()
}

/// NumPy's linear percentile, ignoring nonfinite values for graph axis ranges.
pub fn percentile(values: &[f64], percent: f64) -> f64 {
    let mut values: Vec<_> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let index = (values.len() - 1) as f64 * percent / 100.0;
    let lo = index.floor() as usize;
    let hi = index.ceil() as usize;
    values[lo] + (values[hi] - values[lo]) * index.fract()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_measurement_transform_preserves_upstream_normalization() {
        let samples: Vec<_> = (0..4096)
            .map(|index| {
                let time = index as f64 / 48_000.0;
                (2.0 * std::f64::consts::PI * 997.0 * time).sin() * (-30.0 * time).exp()
            })
            .collect();
        let mut split = smoothed_db(&samples, 48_000.0);
        let reference = midrange_reference(&split);
        split.iter_mut().for_each(|value| *value -= reference);
        assert_eq!(split, magnitude_db(&samples, 48_000.0));
    }
}
