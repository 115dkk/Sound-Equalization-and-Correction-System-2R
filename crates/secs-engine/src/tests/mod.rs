//! Oracle tests of the engine stages (docs/ENGINE.md §3).
//!
//! One file per golden family. `cases` loads the shared case set
//! (`oracle/goldens/engine_inputs`) that every family refers to by name.

mod calibration;
pub(crate) mod cases;
mod chain;
mod decay;
mod design;
mod e2e;
pub(crate) mod fit;
mod mimo;
mod mimo_iir;
mod prep;
mod preview;
mod rc;
mod scorecard;
mod xo;
