#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! REW integration.
//!
//! Upstream sources: `rewapi/*`, `proc.py`.
//!
//! - `RealRew`: HTTP to REW's API (port 4735), the local callback listener
//!   (random path token, replies before dispatch), measurement sequencing,
//!   launching REW with `-api`.
//! - `FakeRew`: the upstream sample IRs (Dolby room 7.1.4) and deterministic
//!   synthetic IRs. No test code is loaded by the product.
//!
//! Both adapters implement the service's measurement seam, next to native
//! measurement in `secs-audio`. The HTTP transport is an internal seam tested
//! with recorded transcripts.
