#![deny(unsafe_code)]
//! Node-API implementation for the secs2r npm package.
//!
//! The isolation boundary and its generated-code exception are specified by
//! ADR 0040.

mod client;
mod core;
mod engine;
mod events;
