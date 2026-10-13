//! Adapted from Impulcifer-py313 `crates/impulcifer-dsp/src/stages/room_tuning2/primitives.rs`
//! (MIT, Copyright (c) 2024- 115dkk), corrected against SECS 2/SciPy.
//! See docs/THIRD_PARTY.md and ../THIRD_PARTY_LICENSES.txt.
//! Upstream design frequency grids and fractional-octave resolution.
use crate::{DspError, invalid};

/// NumPy rfftfreq(n, 1/sr). Zero n or zero sr is an upstream division error.
pub fn freqs(n: usize, sr: f64) -> Result<Vec<f64>, DspError> {
    if n == 0 || sr == 0.0 {
        return Err(invalid("zero FFT length or sample rate"));
    }
    let step = 1.0 / (n as f64 * (1.0 / sr));
    Ok((0..=n / 2).map(|k| k as f64 * step).collect())
}

/// 1/24 below 200 Hz, log-interpolated to 1/6 above 2 kHz. NaNs propagate.
pub fn frac_for(f: &[f64]) -> Vec<f64> {
    f.iter()
        .map(|&f| {
            let lf = if f.is_nan() { f } else { f.max(1e-3) }.log10();
            let t = ((lf - 200_f64.log10()) / (2000_f64.log10() - 200_f64.log10())).clamp(0., 1.);
            24.0 * std::hint::black_box(0.25_f64).powf(t)
        })
        .collect()
}
