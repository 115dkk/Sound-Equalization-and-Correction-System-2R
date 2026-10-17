//! The window icon follows the logo chosen in the app settings
//! (docs/DEVIATIONS.md U12). The bundle icon is upstream's confirmed logo and
//! Tauri already sets it as the default window icon; the other choice is
//! upstream's sixth draft, rendered by `icons/render_icons.py`.

use std::sync::{Mutex, PoisonError};

use secs_types::ipc::AppIcon;
use tauri::image::Image;
use tauri::{AppHandle, Manager};

/// The logo the main window shows. The window starts with the bundle icon;
/// a settings write that keeps the logo (a recent-projects update) leaves
/// the window alone.
#[derive(Default)]
pub(crate) struct Shown(Mutex<AppIcon>);

/// Shows `icon` on the main window if it is not already there. A failure
/// keeps the previous icon: the logo is decoration, so nothing is reported.
pub(crate) fn show(app: &AppHandle, icon: AppIcon) {
    let shown = app.state::<Shown>();
    let mut current = shown.0.lock().unwrap_or_else(PoisonError::into_inner);
    if *current == icon {
        return;
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let image = match icon {
        AppIcon::Drip => app.default_window_icon().cloned(),
        AppIcon::Bubble => Some(bubble(window.scale_factor().unwrap_or(1.0))),
    };
    if let Some(image) = image
        && window.set_icon(image).is_ok()
    {
        *current = icon;
    }
}

/// Windows draws the window icon at its large-icon size, 32 px times the
/// display scale, so it gets the nearest picture at or above that size (as
/// Tauri's default icon picks its entry from the icon resource). The other
/// systems scale one large picture, as they do the bundle's 512 px icon.
fn bubble(scale: f64) -> Image<'static> {
    if !cfg!(windows) {
        return tauri::include_image!("icons/window/bubble-256.png");
    }
    match bubble_size(scale) {
        32 => tauri::include_image!("icons/window/bubble-32.png"),
        48 => tauri::include_image!("icons/window/bubble-48.png"),
        _ => tauri::include_image!("icons/window/bubble-64.png"),
    }
}

/// The smallest of 32, 48 and 64 px that is at least 32 px times `scale`.
fn bubble_size(scale: f64) -> u32 {
    let wanted = 32.0 * scale;
    [32, 48, 64]
        .into_iter()
        .find(|&size| f64::from(size) >= wanted - 0.5)
        .unwrap_or(64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_icon_size_follows_the_display_scale() {
        assert_eq!(bubble_size(1.0), 32);
        assert_eq!(bubble_size(1.25), 48);
        assert_eq!(bubble_size(1.5), 48);
        assert_eq!(bubble_size(1.75), 64);
        assert_eq!(bubble_size(3.0), 64);
    }

    #[test]
    fn bubble_pictures_are_square_and_match_their_names() {
        for (scale, size) in [(1.0, 32), (1.5, 48), (2.0, 64)] {
            let image = bubble(scale);
            let expected = if cfg!(windows) { size } else { 256 };
            assert_eq!((image.width(), image.height()), (expected, expected));
            assert_eq!(image.rgba().len(), (expected * expected * 4) as usize);
        }
    }
}
