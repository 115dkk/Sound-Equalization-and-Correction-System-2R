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

pub mod memory;
pub mod process;

#[cfg(not(target_os = "windows"))]
pub mod cpal;
pub mod wasapi;

pub mod audio {
    use secs_audio::AudioBackend;

    /// Returns the sole supported native audio backend for this operating system.
    #[must_use]
    pub fn default_backend() -> Box<dyn AudioBackend> {
        #[cfg(target_os = "windows")]
        {
            Box::new(crate::wasapi::WasapiBackend::new())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Box::new(crate::cpal::CpalBackend::new())
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_backend_enumeration_is_recoverable() {
        let result = crate::audio::default_backend().enumerate();
        assert!(result.is_ok(), "native enumeration failed: {result:?}");
    }
}
