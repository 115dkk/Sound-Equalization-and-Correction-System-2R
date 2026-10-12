#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! REW `.mdat` read and build.
//!
//! Upstream sources: `rew_mdat/*`, `mdat_loader.py`.
//!
//! The only place that knows REW class and field names, the x0.01 IR scale
//! and the timing-note pattern. A new file is built by cloning a measurement
//! of the embedded template; the UUID source is injected, so the same input
//! gives the same bytes.
