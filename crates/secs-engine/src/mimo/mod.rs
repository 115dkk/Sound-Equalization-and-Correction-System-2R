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
pub use iir::{
    IirDerivatives, IirSolveOptions, IirSolver, IirSupport, MimoIir, design_mimo_iir,
    design_mimo_iir_with_options,
};
pub use run::{
    MimoKey, MimoRoleLateEnergy, MimoRoleScore, MimoRun, availability, cap_text, compute,
    compute_with_acceleration, estimate_seconds, fir_budget_text, fir_to_48k, kind_text,
};

pub use design::{
    MimoDesign, MimoRequest, design_mimo, lowpass_fir, support_path, support_ranges,
    worst_t60_change,
};

#[cfg(feature = "test-support")]
pub use acceleration::{RidgeReplayInput, RidgeReplayOutput, replay_ridge};
