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

pub(crate) mod grid;
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
pub(crate) mod display;
pub(crate) mod export_view;
#[allow(dead_code)]
mod fit;
#[allow(dead_code)]
mod levels;
mod low_protect;
#[allow(dead_code)]
mod measurement;
mod memory;
#[allow(dead_code)]
pub mod mimo;
mod normalize;
#[allow(dead_code)]
mod notes;
pub(crate) mod pair_phase;
pub(crate) mod polarity;
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
#[cfg(any(test, feature = "test-support"))]
pub mod testkit;
#[cfg(test)]
mod tests;

pub use calibration::{Calibration, CalibrationRequest, ProjectView, calibrate};
pub use chain::StageResponses;
pub use design::{Design, RoleLateEnergy, late_energy};
pub use error::EngineError;
pub use input::{EngineConfig, Measurement, MeasurementSet, Samples, Tuning};
pub use mimo::{AccelerationSelection, MimoKey, MimoRun, availability, compute_with_acceleration};
pub use notes::{NoteRecord, NoteRecords};
pub use replay::ExportAxis;
pub use scorecard::Metrics;
pub use secs_dsp::resample::{MAX_RESAMPLE_RATE, MIN_RESAMPLE_RATE};
pub use secs_types::notes::{GuardCause, MimoNote, Note};
pub use setup::SubFilter;

pub(crate) use ba::LayoutInfo;
pub(crate) use chain::{input_response, stage_responses};
pub(crate) use design::production_fitters;
pub(crate) use export_view::ExportView;
pub(crate) use fit::PeqDerivatives;
pub(crate) use measurement::Spectra;
pub(crate) use replay::DesignReplay;

#[cfg(any(test, feature = "test-support"))]
pub use calibration::calibrate_with;
#[cfg(any(test, feature = "test-support"))]
pub use design::{DesignRequest, FitStage, FitterFactory, design};
#[cfg(any(test, feature = "test-support"))]
pub use normalize::normalize_measurements;

#[cfg(test)]
pub(crate) use calibration::engine_inputs;
#[cfg(test)]
pub(crate) use export_view::export_view;
#[cfg(test)]
pub(crate) use mimo::{IirSupport, MimoDesign, MimoIir, MimoRequest, design_mimo, design_mimo_iir};

impl Design {
    pub fn measurement_grid() -> Vec<f64> {
        display::measurement_grid()
    }

    pub fn spl_grid() -> Vec<f64> {
        display::spl_grid()
    }

    pub fn gd_grid() -> Vec<f64> {
        display::gd_grid()
    }

    pub fn smoothed_db(samples: &[f64], sample_rate: f64) -> Vec<f64> {
        display::smoothed_db(samples, sample_rate)
    }

    pub fn midrange_reference(curve: &[f64]) -> f64 {
        display::midrange_reference(curve)
    }

    pub fn magnitude_db(samples: &[f64], sample_rate: f64) -> Vec<f64> {
        display::magnitude_db(samples, sample_rate)
    }

    pub fn measurement_spl(samples: &[f64], sample_rate: f64) -> Vec<f64> {
        display::measurement_spl(samples, sample_rate)
    }

    pub fn percentile(values: &[f64], percent: f64) -> f64 {
        display::percentile(values, percent)
    }
}

/// Complex spectrum value.
pub type C = secs_dsp::Complex64;
