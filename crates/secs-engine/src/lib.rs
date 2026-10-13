#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! The correction design of SECS 2, translated function by function.
//!
//! Upstream sources: `correction_engine.py`, the compute half of
//! `calibration.py`, `mimo.py`, `mimo_iir.py`, `scorecard.py`.
//!
//! The interface is one call: measurements, settings, tuning and a compute
//! context in, an immutable result out. Internally the work is cut into deep
//! modules (docs/ARCHITECTURE.md, "Engine"):
//!
//! - **measurement**: the one time-axis convention (earliest arrival at the
//!   main position is zero, lift, shift), windows, a proper low-rate
//!   projection for MIMO.
//! - **target**: target curve and level reference, evaluated once per grid.
//! - **filters**: RBJ/LR/BW/all-pass responses and the single PEQ fitter with
//!   named limit bundles (RC, sub bus, bundle, XO, MIMO path).
//! - **ba**: bass alignment (auto, fixed, bundles, DBA and multi-sub layouts,
//!   infra sub, two-way level match, bus level, bus PEQ, bus delay/polarity).
//! - **xo**: crossover groups, XO target PEQ, the recommendation.
//! - **rc**: per-channel room correction (magnitude design, common excess
//!   phase, FIR realisation ladder, PEQ refit, touch-up), Schroeder range.
//! - **decay**: late-energy windows, mode T60, late peaks, "do no harm" guards.
//! - **chain**: the DSP topology as one value; the response simulator here and
//!   the CamillaDSP writer in `secs-camilla` read the same value.
//! - **mimo**: support planning, regularised ideal supports, IIR or FIR
//!   realisation behind the `SupportFitBackend` seam (CPU here, GPU in
//!   `secs-gpu`).
//! - **scorecard**: typed metrics before and after.
//!
//! Engine knobs that SECS 2 hid in environment variables are fields of an
//! explicit tuning value. Defects fixed on the way are listed in
//! docs/DEVIATIONS.md.

pub mod grid;
pub mod input;

// The stages are private: `design` (P12) is the one entry point. Until it
// wires them, only their oracle tests call them, hence the dead-code allowance.
#[allow(dead_code)]
mod ba;
#[allow(dead_code)]
mod chain;
#[allow(dead_code)]
mod decay;
#[allow(dead_code)]
mod design;
#[allow(dead_code)]
mod fit;
#[allow(dead_code)]
mod levels;
#[allow(dead_code)]
mod measurement;
#[allow(dead_code)]
mod rc;
#[allow(dead_code)]
mod scorecard;
#[allow(dead_code)]
mod setup;
#[allow(dead_code)]
mod xo;

mod error;
#[cfg(test)]
mod tests;

pub use error::EngineError;
pub use input::{EngineConfig, Measurement, MeasurementSet, Samples, Tuning};

/// Complex spectrum value.
pub type C = secs_dsp::Complex64;
