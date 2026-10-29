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

use std::path::Path;
use std::time::Duration;

use raw_window_handle::HasWindowHandle;
use secs_types::ipc::{
    ApplyOutcome, CamillaStatus, EqApoInstall, EqApoVariant, ErrorCode, IncludeState, IpcError,
};
use thiserror::Error;

fn eqapo_variant(install_path: &str) -> EqApoVariant {
    let normalized = install_path.replace('/', "\\").to_ascii_lowercase();
    if normalized
        .split('\\')
        .any(|part| part.starts_with("equalizerapo-xt"))
    {
        EqApoVariant::Xt
    } else if normalized.split('\\').any(|part| part == "equalizerapo") {
        EqApoVariant::Official
    } else {
        EqApoVariant::Unknown
    }
}

/// Reads Equalizer APO's machine-wide installation values. Missing, denied, or
/// non-string values mean that no usable installation was detected.
#[must_use]
pub fn eqapo_install() -> Option<EqApoInstall> {
    #[cfg(target_os = "windows")]
    {
        let key = windows_registry::LOCAL_MACHINE
            .open(r"SOFTWARE\EqualizerAPO")
            .ok()?;
        let config_path = key.get_string("ConfigPath").ok()?;
        let install_path = key.get_string("InstallPath").ok()?;
        Some(EqApoInstall {
            variant: eqapo_variant(&install_path),
            config_path,
            install_path,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Reads CamillaDSP through the platform-neutral application protocol client.
pub fn camilla_status(address: &str, timeout: Duration) -> Result<CamillaStatus, IpcError> {
    let transport =
        secs_apply::camilla::WsTransport::connect(address, timeout).map_err(camilla_error)?;
    let status = secs_apply::camilla::Camilla::new(transport)
        .status()
        .map_err(camilla_error)?;
    Ok(CamillaStatus {
        version: status.version,
        state: status.state,
        stop_reason: status.stop_reason,
        title: status.title,
        description: status.description,
        config_path: status.config_path,
        capture_rate: status.capture_rate,
        clipped_samples: status.clipped_samples,
        processing_load_pct: status.processing_load_pct,
        playback_peak_db: status.playback_peak_db,
    })
}

/// Validates, selects, and reloads one CamillaDSP configuration.
pub fn camilla_apply(
    address: &str,
    config_path: &Path,
    config_text: &str,
    timeout: Duration,
) -> Result<ApplyOutcome, IpcError> {
    let transport =
        secs_apply::camilla::WsTransport::connect(address, timeout).map_err(camilla_error)?;
    match secs_apply::camilla::Camilla::new(transport)
        .apply(&config_path.to_string_lossy(), config_text)
        .map_err(camilla_error)?
    {
        secs_apply::camilla::ApplyOutcome::Applied { .. } => Ok(ApplyOutcome::Applied),
        secs_apply::camilla::ApplyOutcome::RolledBack { error, .. } => {
            Ok(ApplyOutcome::RolledBack {
                error: IpcError::new(ErrorCode::ApplyFailed, error),
            })
        }
    }
}

/// Classifies SECS 2R's managed Equalizer APO lines.
#[must_use]
pub fn eqapo_include_state(config_text: &str) -> IncludeState {
    match secs_apply::eqapo::include_state(config_text) {
        secs_apply::eqapo::IncludeState::Absent => IncludeState::Absent,
        secs_apply::eqapo::IncludeState::Present => IncludeState::Present,
        secs_apply::eqapo::IncludeState::Damaged => IncludeState::Damaged,
    }
}

/// Adds or repairs SECS 2R's managed Equalizer APO lines.
#[must_use]
pub fn eqapo_with_include(config_text: &str) -> Option<String> {
    secs_apply::eqapo::with_include(config_text)
}

/// Removes SECS 2R's managed Equalizer APO lines.
#[must_use]
pub fn eqapo_without_include(config_text: &str) -> Option<String> {
    secs_apply::eqapo::without_include(config_text)
}

fn camilla_error(error: secs_apply::ApplyError) -> IpcError {
    let mut ipc = IpcError::new(ErrorCode::ApplyFailed, error.to_string());
    ipc.retryable = matches!(
        error,
        secs_apply::ApplyError::Connect { .. } | secs_apply::ApplyError::Transport(_)
    );
    ipc
}

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

/// Bytes in the `CreateIcon` AND mask of a `width` x `height` icon. The mask
/// is a monochrome bitmap whose rows are padded to 16-bit words, so each row
/// takes `ceil(width / 16) * 2` bytes; `width` bytes per row would leave
/// Windows reading past the buffer for a 1-pixel-wide icon.
// Only the Windows taskbar leaf calls this; it lives here so that its test
// runs on every system.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn and_mask_len(width: u32, height: u32) -> usize {
    (width as usize).div_ceil(16) * 2 * height as usize
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
    fn equalizer_apo_variant_follows_the_install_folder_name() {
        assert_eq!(
            eqapo_variant(r"C:\Users\me\AppData\Local\EqualizerAPO-XT\current"),
            EqApoVariant::Xt
        );
        assert_eq!(
            eqapo_variant(r"C:\Program Files\EqualizerAPO"),
            EqApoVariant::Official
        );
        assert_eq!(eqapo_variant(r"D:\Audio\APO"), EqApoVariant::Unknown);
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
    fn and_mask_rows_are_padded_to_words() {
        for (width, row_bytes) in [(1, 2), (16, 2), (17, 4), (256, 32)] {
            assert_eq!(and_mask_len(width, 1), row_bytes, "width {width}");
            assert_eq!(and_mask_len(width, 3), 3 * row_bytes, "width {width}");
        }
        assert_eq!(and_mask_len(256, 256), 8192);
    }

    #[test]
    fn default_backend_enumeration_is_recoverable() {
        let result = crate::audio::default_backend().enumerate();
        assert!(result.is_ok(), "native enumeration failed: {result:?}");
    }
}
