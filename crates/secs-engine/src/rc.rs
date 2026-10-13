//! Per-channel room correction: magnitude design and masks, the PEQ-only
//! branch, common excess phase, the FIR realisation ladder, PEQ refit, broad
//! touch-up, phase-resolution selection, GD cuts.
//!
//! Upstream: `CorrectionEngine._correction`, `_no_boost_mask`,
//! `_long_mode_mask`, and the per-main loop of `compute()`.
