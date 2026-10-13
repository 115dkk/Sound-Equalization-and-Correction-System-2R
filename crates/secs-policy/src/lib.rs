#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Workspace gates (docs/adr/0002). The tests in `tests/gates.rs` read the
//! repository tree directly: `#![forbid(unsafe_code)]` on every crate root,
//! `unsafe` only within `unsafe-budget.toml`, OS-facing dependencies only in
//! `secs-sys`, the `features.toml` registry, the subprocess allowlist and the
//! registration of every IPC method.
