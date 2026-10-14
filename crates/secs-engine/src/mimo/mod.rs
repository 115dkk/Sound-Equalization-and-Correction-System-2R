//! Low-rate MIMO ideal FIR support design and do-no-harm guards.

pub(crate) mod design;
pub(crate) mod iir;
pub(crate) mod prepared;
pub(crate) mod run;
pub(crate) mod solve;
pub use iir::{IirSupport, MimoIir, design_mimo_iir};
pub use run::{
    MimoKey, MimoRun, availability, cap_text, compute, estimate_seconds, fir_budget_text,
    fir_to_48k, kind_text,
};

pub use design::{
    MimoDesign, MimoRequest, design_mimo, lowpass_fir, support_path, support_ranges,
    worst_t60_change,
};
