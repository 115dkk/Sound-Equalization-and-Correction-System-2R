//! Oracle tests of the engine stages (docs/ENGINE.md §3).
//!
//! One file per golden family. `cases` loads the shared case set
//! (`oracle/goldens/engine_inputs`) that every family refers to by name.

mod calibration;
mod cases;
mod chain;
mod decay;
pub(crate) mod fit;
mod prep;
mod scorecard;
