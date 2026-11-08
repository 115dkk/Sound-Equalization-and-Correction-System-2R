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
//! - **grid**: shared design sample rate, FFT size and frequency grid.
//! - **input**: measurements, resolved engine settings and tuning values.
//! - **setup**: constructor normalization and subwoofer filter settings.
//! - **measurement**: time-axis convention, windows and low-rate projection for MIMO.
//! - **levels**: position weights, target sampling, band levels and Schroeder frequency.
//! - **fit**: shared PEQ fitting, ringing limits, GD cuts and acoustic XO targets.
//! - **ba**: subwoofer layouts, alignment, bus level, PEQ, delay and polarity.
//! - **xo**: crossover groups, target PEQ and recommendations.
//! - **rc**: magnitude and phase correction, FIR realization and dip limits (ADR 0034).
//! - **decay**: caller-specific late-energy windows, mode T60 and late peaks.
//! - **chain**: response simulation over the shared DSP signal-chain value.
//! - **mimo**: ideal support solves, CPU IIR and CPU/GPU FIR (ADR 0008), seat guards (0036) and early intervals (0037).
//! - **scorecard**: typed before/after metrics.
//! - **design**: immutable correction design and ordered stage orchestration.
//! - **calibration**: project-setting validation, normalization and calibration entry point (ADR 0046, 0055).
//! - **low_protect**: measured rolloff and realized-response protection (ADR 0022).
//! - **polarity**: relative main-speaker polarity checks (ADR 0033).
//! - **pair_phase**: corrected phase and cancellation checks between main pairs (ADR 0035).
//! - **normalize**: calculation-only resampling that preserves stored measurements.
//! - **replay**: retained design inputs, prepared spectra and export time axes (ADR 0012, 0046).
//! - **replay_mimo**: cached MIMO response replay using the retained design.
//! - **export_view**: derived long-window spectra and timing for response exports.
//! - **preview**: BA display curves replayed without fitting.
//! - **display**: plot grids, smoothing and measurement magnitudes.
//! - **memory**: reservations before ordered pool work, including nested maps (ADR 0007).
//! - **notes**: typed diagnostic records; translated text belongs to presentation (ADR 0042).
//! - **error**: engine errors mapped by the service to user-facing codes.
//! - **testkit**: injected fixture storage and oracle replay helpers for tests and test-support (ADR 0050).
//! - **tests**: internal stage, parity and regression tests, compiled only for tests (ADR 0004).
//!
//! Engine knobs that SECS 2 hid in environment variables are fields of an
//! explicit tuning value. Defects fixed on the way are listed in
//! docs/DEVIATIONS.md.

pub(crate) mod grid;
pub mod input;

// The stages are private: `design` is the one entry point and the values it
// returns are re-exported below.
mod ba;
mod calibration;
mod chain;
mod decay;
mod design;
pub(crate) mod display;
pub(crate) mod export_view;
mod fit;
mod levels;
mod low_protect;
mod measurement;
mod memory;
pub(crate) mod mimo;
mod normalize;
mod notes;
pub(crate) mod pair_phase;
pub(crate) mod polarity;
pub mod preview;
mod rc;
mod replay;
mod replay_mimo;
mod scorecard;
mod setup;
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
pub use mimo::cap_text as mimo_cap_text;
pub use mimo::{
    AccelerationError, AccelerationSelection, CpuRidgeBackend, MimoKey, MimoRun, RidgeBackend,
    ShiftedSystem, availability, compute_with_acceleration,
};
#[cfg(feature = "test-support")]
pub use mimo::{
    IirSolver, IirSupport, MimoDesign, MimoIir, MimoRequest, RidgeReplayInput, RidgeReplayOutput,
    compute, design_mimo, design_mimo_iir, design_mimo_iir_with_options, estimate_seconds,
    fir_budget_text, fir_to_48k, kind_text, lowpass_fir, replay_ridge, support_path,
    support_ranges,
};
pub use notes::{NoteRecord, NoteRecords};
pub use replay::ExportAxis;
pub use scorecard::Metrics;
pub use secs_types::notes::{GuardCause, MimoNote, Note};
pub use setup::SubFilter;

pub(crate) use ba::LayoutInfo;
pub(crate) use chain::{input_response, stage_responses};
#[cfg(any(test, feature = "test-support"))]
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
