#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! The single owner of project files and measurement records.
//!
//! Upstream sources: `model.py`, `rewapi/session.py`, the `extra` keys written
//! by the wizard pages, `mdat_loader.suggest_assignment`, the title parsing in
//! `wizard/step5_measure.py`.
//!
//! - `.secs` versions 1, 2 and 3 read and migrated; written with the key order
//!   and number format SECS 2 writes, so SECS 2 can open the file again.
//! - The free-form `extra` map becomes typed sections (devices, positions,
//!   calibration settings, export); unknown keys survive in order.
//! - Role vocabulary and every title rule (`L_P2`, `SW 2`, `서브우퍼2`) and the
//!   measurement plan live here once.
//! - `.irs/` IR store: reads only inside the project's own folder; writes are
//!   atomic (temporary file and rename) and have a single writer; each channel
//!   is recorded as soon as it is measured.
//! - The calculation cache is a versioned data format, never a pickle.
