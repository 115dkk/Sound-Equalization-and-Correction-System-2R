//! The design grid every full-resolution spectrum lives on.
//!
//! Upstream: `correction_engine.py` `SR`, `N_FFT`, `freqs()`.

use std::sync::OnceLock;

/// Design sample rate in Hz. Every measurement must use it (upstream checks
/// only the main position; SECS 2R checks all, D2).
pub const SR: u32 = 48_000;
/// [`SR`] as a float, for arithmetic.
pub const SR_F: f64 = 48_000.0;
/// Design FFT length, 2^17 (0.366 Hz bins), enough to resolve low room modes.
pub const N_FFT: usize = 1 << 17;
/// Bins of a full design spectrum, `N_FFT / 2 + 1`.
pub const BINS: usize = N_FFT / 2 + 1;

/// `secs_dsp::grid::freqs(N_FFT, SR)`, computed once per process.
pub fn design_freqs() -> &'static [f64] {
    static F: OnceLock<Vec<f64>> = OnceLock::new();
    F.get_or_init(|| {
        let step = 1.0 / (N_FFT as f64 * (1.0 / SR_F));
        (0..BINS).map(|k| k as f64 * step).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_grid_matches_the_dsp_primitive() {
        let expected = secs_dsp::grid::freqs(N_FFT, SR_F).unwrap();
        assert_eq!(design_freqs().len(), BINS);
        assert!(
            design_freqs()
                .iter()
                .zip(&expected)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }
}
