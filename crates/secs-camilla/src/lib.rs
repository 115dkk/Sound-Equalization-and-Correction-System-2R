#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! CamillaDSP v4 export.
//!
//! Upstream source: `export_camilladsp.py`.
//!
//! Reads one signal-chain value (not engine internals) and writes
//! `configs/*.yml` and `coeffs/*.wav` in the CamillaGUI folder layout, with
//! Python's float formatting and half-to-even delay rounding, ASCII-only
//! (the CamillaGUI server reads CP949). Every check (ASCII, finite numbers)
//! runs before the first file is written.
