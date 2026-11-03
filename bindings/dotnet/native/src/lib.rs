#![deny(unsafe_code)]
//! C ABI implementation for the Secs2R NuGet package.
//!
//! The boundary and its ownership rules are specified by ADR 0032.

mod core;

#[allow(unsafe_code)]
pub mod ffi;
