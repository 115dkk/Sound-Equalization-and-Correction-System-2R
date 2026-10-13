//! Adapted from Impulcifer-py313 `crates/impulcifer-dsp/src/stages/room_tuning2/primitives.rs`
//! (MIT, Copyright (c) 2024- 115dkk), corrected against SECS 2/SciPy.
//! See docs/THIRD_PARTY.md and ../THIRD_PARTY_LICENSES.txt.
//! Cepstral minimum-phase response, using the implied even inverse length.
use crate::{Complex64, DspError, fft, invalid};

pub fn minimum_phase(mag: &[f64]) -> Result<Vec<Complex64>, DspError> {
    if mag.len() < 2 {
        return Err(invalid("minimum_phase needs at least two bins"));
    }
    let n = 2 * (mag.len() - 1);
    let logm: Vec<_> = mag
        .iter()
        .map(|&x| Complex64::new(if x.is_nan() { x } else { x.max(1e-8) }.ln(), 0.))
        .collect();
    let mut cep = fft::irfft(&logm, n);
    for x in &mut cep[1..n / 2] {
        *x *= 2.;
    }
    cep[n / 2 + 1..].fill(0.);
    Ok(fft::rfft(&cep).into_iter().map(|x| x.exp()).collect())
}
