//! The window icon follows the logo chosen in the app settings
//! (docs/DEVIATIONS.md U12). The bundle icon is upstream's confirmed logo and
//! Tauri already sets it as the default window icon; the other choice is
//! SECS 2R's own logo, rendered by `icons/render_icons.py`.

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
    let current = shown.0.lock().unwrap_or_else(PoisonError::into_inner);
    if *current == icon {
        return;
    }
    drop(current);
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let image: Option<Image<'static>> = match icon {
        AppIcon::Expert => app.default_window_icon().cloned().map(Image::to_owned),
        AppIcon::Civilian => Some(civilian(window.scale_factor().unwrap_or(1.0))),
    };
    let Some(image) = image else {
        return;
    };

    #[cfg(target_os = "windows")]
    {
        let app: AppHandle = app.clone();
        let _ = window.clone().run_on_main_thread(move || {
            if window.set_icon(image.clone()).is_ok() {
                let _ =
                    secs_sys::set_large_icon(&window, image.rgba(), image.width(), image.height());
                let shown = app.state::<Shown>();
                *shown.0.lock().unwrap_or_else(PoisonError::into_inner) = icon;
            }
        });
    }
    #[cfg(not(target_os = "windows"))]
    if window.set_icon(image).is_ok() {
        *shown.0.lock().unwrap_or_else(PoisonError::into_inner) = icon;
    }
}

/// Windows draws the window icon at its large-icon size, 32 px times the
/// display scale, so it gets the nearest picture at or above that size (as
/// Tauri's default icon picks its entry from the icon resource). The other
/// systems scale one large picture, as they do the bundle's 512 px icon.
fn civilian(scale: f64) -> Image<'static> {
    if !cfg!(windows) {
        return tauri::include_image!("icons/window/civilian-256.png");
    }
    match window_icon_size(scale) {
        32 => tauri::include_image!("icons/window/civilian-32.png"),
        48 => tauri::include_image!("icons/window/civilian-48.png"),
        _ => tauri::include_image!("icons/window/civilian-64.png"),
    }
}

/// The smallest of 32, 48 and 64 px that is at least 32 px times `scale`.
fn window_icon_size(scale: f64) -> u32 {
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
        assert_eq!(window_icon_size(1.0), 32);
        assert_eq!(window_icon_size(1.25), 48);
        assert_eq!(window_icon_size(1.5), 48);
        assert_eq!(window_icon_size(1.75), 64);
        assert_eq!(window_icon_size(3.0), 64);
    }

    #[test]
    fn civilian_pictures_are_square_and_match_their_names() {
        for (scale, size) in [(1.0, 32), (1.5, 48), (2.0, 64)] {
            let image = civilian(scale);
            let expected = if cfg!(windows) { size } else { 256 };
            assert_eq!((image.width(), image.height()), (expected, expected));
            assert_eq!(image.rgba().len(), (expected * expected * 4) as usize);
        }
    }
}
