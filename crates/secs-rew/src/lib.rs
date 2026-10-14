#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! REW HTTP API and a caller-supplied-IR tutorial backend.
//!
//! Protocol parity: `src/secs/rewapi/{rew,interfaces,callback,fake}.py` (GPL-3.0-only).
//! The service owns project plans; [`MeasurementPlan`] is the REW adapter's input,
//! not an IPC type. No operating-system process enumeration lives here.

mod callback;
mod client;
mod fake;
mod interfaces;
pub mod launch;
mod transport;
mod types;

pub use callback::{CallbackServer, MAX_CALLBACK_BODY};
pub use client::{RewClient, b64_be_f32, channel_name_for, level_payload, parse_progress};
pub use fake::{FakeOptions, FakeRew, IrSource, fake_plan};
pub use interfaces::{DeviceLister, IRProvider, LevelMonitor, Measurer, RewStatus};
pub use transport::{
    HttpTransport, RecordedExchange, RecordedTransport, Request, Response, Transport,
};
pub use types::*;
