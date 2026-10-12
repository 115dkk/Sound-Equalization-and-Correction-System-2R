#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Optional GPU adapter for the MIMO support fit (docs/adr/0007).
//!
//! Implements the engine's `SupportFitBackend` seam with wgpu compute shaders:
//! the time-domain residual and Jacobian columns of all supports are evaluated
//! in f32 (biquad responses in the sin²(ω/2) form, which stays accurate at low
//! frequency), while the normal equations and the trust-region step stay on
//! the CPU in f64. Results are checked against the CPU adapter within a
//! tolerance; they are not bit-identical. Without a usable device the app
//! falls back to the CPU adapter.
//!
//! The adapter ships only if an M3 spike shows it at least 3x faster than the
//! optimized CPU path at equal result quality.
