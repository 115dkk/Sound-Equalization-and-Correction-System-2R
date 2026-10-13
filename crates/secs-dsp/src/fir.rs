//! Adapted from Impulcifer-py313 `crates/impulcifer-dsp/src/stages/room_tuning2/primitives.rs`
//! (MIT, Copyright (c) 2024- 115dkk), corrected against SECS 2/SciPy.
//! See docs/THIRD_PARTY.md and ../THIRD_PARTY_LICENSES.txt.
//! SciPy firwin subset used by mimo.lowpass_fir: Hamming low-pass, DC scale.
use crate::{DspError, invalid, numeric::sum};
use std::f64::consts::PI;

pub fn firwin(taps: usize, cutoff: f64, fs: f64) -> Result<Vec<f64>, DspError> {
    let fc = cutoff / (0.5 * fs);
    if fc <= 0. || fc >= 1. {
        return Err(invalid("cutoff must lie between zero and Nyquist"));
    }
    if taps == 0 {
        return Err(invalid("firwin needs positive taps"));
    }
    let mut h: Vec<_> = (0..taps)
        .map(|i| {
            let m = i as f64 - 0.5 * (taps as f64 - 1.);
            let x = fc * m;
            let y = PI * if x == 0. { 1e-20 } else { x };
            let sinc = y.sin() / y;
            // scipy.windows.general_cosine uses linspace(-pi, pi, taps).
            let phase = if i == taps - 1 {
                PI
            } else {
                -PI + i as f64 * (2. * PI / (taps - 1) as f64)
            };
            let window = if taps == 1 {
                1.
            } else {
                0.54 + (1.0 - 0.54) * phase.cos()
            };
            (fc * sinc - 0.) * window
        })
        .collect();
    let scale = sum(&h);
    for v in &mut h {
        *v /= scale;
    }
    Ok(h)
}
