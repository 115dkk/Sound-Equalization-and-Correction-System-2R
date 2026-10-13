//! Low-rate MIMO ideal FIR support design and do-no-harm guards.

pub(crate) mod design;
pub(crate) mod solve;

pub use design::{
    MimoDesign, MimoRequest, design_mimo, lowpass_fir, support_path, support_ranges,
    worst_t60_change,
};
