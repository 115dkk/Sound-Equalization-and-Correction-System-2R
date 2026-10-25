#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Shared vocabulary of SECS 2R.
//!
//! Domain types (roles, settings, the signal-chain value), error codes, the
//! IPC types and the one atomic file writer ([`fs`]) live here, so the service, the app, the CLI and the screen agree
//! on one definition. Types the screen sees derive `ts_rs::TS`; their
//! TypeScript bindings are generated into `apps/secs2r/ui/src/ipc/bindings`
//! whenever this crate's tests run, and CI fails when the committed copy
//! differs (docs/adr/0003).

pub mod calibration;
pub mod chain;
pub mod compute;
pub mod fs;
pub mod ipc;
pub mod role;
pub mod settings_edit;
pub mod target;
