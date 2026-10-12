#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! The isolation crate (docs/adr/0002).
//!
//! The only first-party crate that may depend on OS-facing crates (`wasapi`,
//! `cpal`, `windows*`, `libc` and the OS probes). Its budget in
//! `unsafe-budget.toml` is 0: everything here goes through those crates' safe
//! APIs, as Impulcifer's `impulcifer-sys-win` does. Raising the budget needs
//! an ADR first.
//!
//! The interface follows the leaf contract: owned values, checked
//! configurations, ordinary borrowed slices, enums and `Result`. No raw
//! pointers, handles, COM interfaces, `unsafe fn`, `static mut`, transmute or
//! manual `Send`/`Sync` cross it, and invalid safe input produces an error or
//! a documented panic, never undefined behaviour. SECURITY.md treats a
//! violation as a vulnerability.
//!
//! Only crates under `apps/` may depend on this crate; they inject its
//! adapters into the seams of `secs-audio` and `secs-compute`.
