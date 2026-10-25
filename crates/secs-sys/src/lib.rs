#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! The isolation crate (docs/adr/0002).
//!
//! The only first-party crate that may depend on OS-facing crates (`wasapi`,
//! `cpal`, `windows*`, `libc` and the OS probes). Its budget in
//! `unsafe-budget.toml` covers only the Windows taskbar icon leaf from ADR
//! 0011; everything else goes through those crates' safe APIs, as Impulcifer's
//! `impulcifer-sys-win` does. Raising the budget needs an ADR first.
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

pub mod clock;
pub mod memory;
pub mod process;
pub mod rew;
#[cfg(target_os = "windows")]
mod taskbar;

use raw_window_handle::HasWindowHandle;
use thiserror::Error;

/// A checked failure while setting a window's large icon.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LargeIconError {
    /// The icon dimensions are outside the supported range.
    #[error("icon dimensions {width}x{height} are outside 1..=256")]
    Size { width: u32, height: u32 },
    /// The RGBA buffer length does not match its dimensions.
    #[error("icon RGBA length is {got}, expected {expected}")]
    Length { expected: usize, got: usize },
    /// The window handle is unavailable or is not a Win32 handle.
    #[error("window handle is unavailable or is not Win32")]
    Handle,
    /// Win32 could not create the icon.
    #[error("Win32 could not create the icon")]
    Create,
}

/// Sets the large icon of a window: the picture the Windows taskbar and
/// Alt+Tab draw (ADR 0011). Does nothing on other systems.
pub fn set_large_icon(
    window: &impl HasWindowHandle,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<(), LargeIconError> {
    if !(1..=256).contains(&width) || !(1..=256).contains(&height) {
        return Err(LargeIconError::Size { width, height });
    }
    let expected = width as usize * height as usize * 4;
    if rgba.len() != expected {
        return Err(LargeIconError::Length {
            expected,
            got: rgba.len(),
        });
    }

    #[cfg(target_os = "windows")]
    {
        taskbar::set_large_icon(window, rgba, width, height)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        Ok(())
    }
}

/// How long an open audio stream may go without progress before its backend
/// reports it as stalled: no capture callback or packet, no render frame
/// accepted and no fall in render padding. The cpal and WASAPI backends share
/// it.
const STALL: std::time::Duration = std::time::Duration::from_secs(2);

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
    use raw_window_handle::{HandleError, WindowHandle};

    use super::*;

    struct NoWindow;

    impl HasWindowHandle for NoWindow {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            Err(HandleError::Unavailable)
        }
    }

    #[test]
    fn rejects_large_icon_dimensions_outside_supported_range() {
        assert_eq!(
            set_large_icon(&NoWindow, &[], 0, 1),
            Err(LargeIconError::Size {
                width: 0,
                height: 1
            })
        );
        assert_eq!(
            set_large_icon(&NoWindow, &[], 1, 257),
            Err(LargeIconError::Size {
                width: 1,
                height: 257
            })
        );
    }

    #[test]
    fn rejects_large_icon_rgba_length_mismatch() {
        assert_eq!(
            set_large_icon(&NoWindow, &[0; 15], 2, 2),
            Err(LargeIconError::Length {
                expected: 16,
                got: 15
            })
        );
    }

    #[test]
    fn default_backend_enumeration_is_recoverable() {
        let result = crate::audio::default_backend().enumerate();
        assert!(result.is_ok(), "native enumeration failed: {result:?}");
    }
}
