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
//! - **mimo**: support planning, regularised ideal supports (the λ search of
//!   the ideal FIR goes through the `RidgeBackend` seam: CPU `RidgeSweep`
//!   here, CUDA f64 in `secs-gpu`, ADR 0008), IIR or FIR realisation (IIR
//!   always runs on the CPU).
//! - **scorecard**: typed metrics before and after.
//!
//! Engine knobs that SECS 2 hid in environment variables are fields of an
//! explicit tuning value. Defects fixed on the way are listed in
//! docs/DEVIATIONS.md.

pub mod grid;
pub mod input;

// The stages are private: `design` is the one entry point and the values it
// returns are re-exported below. Helpers only the oracle tests call yet keep
// the dead-code allowance.
#[allow(dead_code)]
mod ba;
#[allow(dead_code)]
mod calibration;
#[allow(dead_code)]
mod chain;
#[allow(dead_code)]
mod decay;
#[allow(dead_code)]
mod design;
pub mod display;
pub mod export_view;
#[allow(dead_code)]
mod fit;
#[allow(dead_code)]
mod levels;
#[allow(dead_code)]
mod measurement;
mod memory;
#[allow(dead_code)]
pub mod mimo;
mod normalize;
#[allow(dead_code)]
mod notes;
pub mod preview;
#[allow(dead_code)]
mod rc;
mod replay;
mod replay_mimo;
#[allow(dead_code)]
mod scorecard;
#[allow(dead_code)]
mod setup;
#[allow(dead_code)]
pub mod xo;

mod error;
#[cfg(test)]
pub mod test_support;
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod testkit;
#[cfg(test)]
mod tests;

pub use ba::{LayoutInfo, ManualPlan};
pub use calibration::{
    Calibration, CalibrationRequest, EngineInputs, ProjectView, calibrate, calibrate_with,
    engine_inputs,
};
pub use chain::{BaRow, StageResponses, input_response, stage_responses};
pub use design::{
    Design, DesignRequest, DesignSpectra, FitStage, FitterFactory, MainDesign, RoleLateEnergy,
    RoleScores, SubDesign, design, late_energy, production_fitters, production_fitters_with,
};
pub use error::EngineError;
pub use export_view::{ExportView, export_view};
pub use fit::{FitRequest, Fitter, PeqDerivatives, fit_peq_with};
pub use input::{EngineConfig, Measurement, MeasurementSet, Samples, Tuning};
pub use measurement::{Spectra, long_spectra};
pub use mimo::{
    IirDerivatives, IirSolveOptions, IirSolver, IirSupport, MimoDesign, MimoIir, MimoKey,
    MimoRequest, MimoRoleLateEnergy, MimoRoleScore, MimoRun, design_mimo, design_mimo_iir,
    design_mimo_iir_with_options,
};
pub use normalize::normalize_measurements;
pub use notes::{MimoNote, Note};
pub use replay::{DesignReplay, ExportAxis};
pub use scorecard::Metrics;
pub use secs_dsp::resample::{MAX_RESAMPLE_RATE, MIN_RESAMPLE_RATE};
pub use setup::SubFilter;

/// Complex spectrum value.
pub type C = secs_dsp::Complex64;
