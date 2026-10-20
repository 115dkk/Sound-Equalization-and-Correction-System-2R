#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! f64 DSP primitives and optimizers with SciPy/NumPy semantics.
//!
//! Shared by `secs-engine`, `secs-mdat` (response caches), `secs-camilla`
//! (FIR resampling) and `secs-audio` (sweep deconvolution). Functions match
//! the NumPy/SciPy calls SECS 2 makes, including the details a port must keep
//! (docs/ARCHITECTURE.md, "Numerics"):
//!
//! - FFT through cached `realfft` plans; pairwise summation where NumPy sums.
//! - RBJ biquads, analog LR/BW prototypes, all-pass, minimum phase (cepstrum),
//!   fractional-octave smoothing, group delay, `butter`/`sosfilt`/`firwin`,
//!   `find_peaks` with the plateau-midpoint rule.
//! - `least_squares` (bounded TRF, exact trust-region solver, two-point
//!   Jacobians; `max_nfev` counts residual calls only), ported
//!   from SciPy 1.18.1 by way of Impulcifer's `optimize.rs` (MIT).
//! - Differential evolution uses legacy RandomState (MT19937) with SciPy's
//!   immediate best1bin draw order and Latin-hypercube initialization.
//! - Rounding is half-to-even (`f64::round_ties_even`) wherever NumPy rounds.
//!
//! There is no handwritten `std::arch`; faer SIMD dispatch is provided by pulp.
//!
//! New primitive APIs return `Result` for data-dependent upstream failures.
//! Allocation failure or unrepresentable array/order sizes are programmer errors.
//! FFT APIs retain their documented panic contract. TRF parity is partial;
//! see `tests/golden.rs` and the feature registry before using it in an engine.

pub mod fft;
pub use fft::Complex64;
pub mod fir;
pub mod grid;
pub mod iir;
pub mod lowrate;
pub mod minphase;
mod numeric;
pub use numeric::divide as complex_div;
pub use numeric::interp;
pub use numeric::sum as pairwise_sum;
pub mod optimize;
pub mod peaks;
pub mod random;
pub mod resample;
pub mod response;
pub mod smooth;

/// Data-dependent errors corresponding to upstream invalid inputs.
#[derive(Debug, thiserror::Error)]
pub enum DspError {
    #[error("{0}")]
    InvalidArgument(String),
}

pub(crate) fn invalid(message: &str) -> DspError {
    DspError::InvalidArgument(message.to_owned())
}
