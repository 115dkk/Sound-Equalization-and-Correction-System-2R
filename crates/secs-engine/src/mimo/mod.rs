//! Low-rate MIMO ideal FIR support design and do-no-harm guards.

mod acceleration;
pub use acceleration::{
    AccelerationError, AccelerationSelection, CpuRidgeBackend, RidgeBackend, ShiftedSystem,
};
pub(crate) mod design;
pub(crate) mod iir;
pub(crate) mod prepared;
pub(crate) mod run;
pub(crate) mod solve;
pub use iir::{IirDerivatives, IirSolveOptions, IirSupport};
#[cfg(any(test, feature = "test-support"))]
pub use iir::{IirSolver, MimoIir, design_mimo_iir, design_mimo_iir_with_options};
pub use run::{MimoKey, MimoRun, availability, cap_text, compute_with_acceleration};
#[cfg(any(test, feature = "test-support"))]
pub use run::{compute, estimate_seconds, fir_budget_text, fir_to_48k, kind_text};

pub use design::{MimoDesign, MimoRequest, support_path};
#[cfg(any(test, feature = "test-support"))]
pub use design::{design_mimo, lowpass_fir, support_ranges, worst_t60_change};

#[cfg(feature = "test-support")]
pub use acceleration::{RidgeReplayInput, RidgeReplayOutput, replay_ridge};

fn validate_field(field: &str, valid: bool) -> Result<(), crate::EngineError> {
    if valid {
        Ok(())
    } else {
        Err(crate::EngineError::Input(format!("invalid MIMO {field}")))
    }
}
