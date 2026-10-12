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
//! - `least_squares` (bounded TRF, exact trust-region solver, two-point and
//!   caller-supplied Jacobians; `max_nfev` counts residual calls only), ported
//!   from SciPy 1.18.1 by way of Impulcifer's `optimize.rs` (MIT).
//! - `differential_evolution` with NumPy's legacy `RandomState` (MT19937),
//!   consuming random numbers in SciPy's order, so BA reproduces upstream.
//! - Rounding is half-to-even (`f64::round_ties_even`) wherever NumPy rounds.
//!
//! Hot loops use `pulp` runtime SIMD; there is no handwritten `std::arch`.

pub mod fft;
pub use fft::Complex64;
