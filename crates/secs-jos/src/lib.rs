#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Java Object Serialization (the stream format under REW `.mdat`).
//!
//! Upstream source: `rew_mdat/serialization/*`.
//!
//! Parse and write back byte-identically: handle numbering, class
//! descriptors, block data with its long flag, modified UTF-8 (lone
//! surrogates kept). The parser is iterative with explicit size, depth and
//! handle limits, so hostile input ends in an error and never in a stack
//! overflow. It is a fuzz and Miri target.
