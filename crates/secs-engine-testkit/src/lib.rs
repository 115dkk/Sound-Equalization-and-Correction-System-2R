#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Re-export of `secs_engine::testkit` for dependent crate parity tests.
//!
//! This facade keeps the engine's `test-support` feature behind a
//! dev-dependency, so normal builds of dependent crates do not enable it.

pub use secs_engine::testkit::*;
