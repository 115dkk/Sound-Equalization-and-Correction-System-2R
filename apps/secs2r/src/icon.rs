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
#[derive(Debug, Default)]
struct IconState {
    shown: AppIcon,
    desired: AppIcon,
    queued: bool,
}

impl IconState {
    fn request(&mut self, icon: AppIcon) -> bool {
        self.desired = icon;
        if self.queued || self.desired == self.shown {
            false
        } else {
            self.queued = true;
            true
        }
    }

    fn next(&mut self) -> Option<AppIcon> {
        if self.desired == self.shown {
            self.queued = false;
            None
        } else {
            Some(self.desired)
        }
    }

    fn mark_shown(&mut self, icon: AppIcon) {
        self.shown = icon;
    }

    fn failed(&mut self) {
        self.queued = false;
    }
}

#[derive(Default)]
pub(crate) struct Shown(Mutex<IconState>);

impl Shown {
    fn request(&self, icon: AppIcon) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .request(icon)
    }

    fn next(&self) -> Option<AppIcon> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).next()
    }

    fn mark_shown(&self, icon: AppIcon) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .mark_shown(icon);
    }

    fn failed(&self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .failed();
    }
}

fn image(app: &AppHandle, scale: f64, icon: AppIcon) -> Option<Image<'static>> {
    match icon {
        AppIcon::Expert => app.default_window_icon().cloned().map(Image::to_owned),
        AppIcon::Civilian => Some(civilian(scale)),
    }
}

/// Shows `icon` on the main window if it is not already there. A failure
/// keeps the previous icon: the logo is decoration, so nothing is reported.
pub(crate) fn show(app: &AppHandle, icon: AppIcon) {
    let shown = app.state::<Shown>();
    if !shown.request(icon) {
        return;
    }
    let Some(window) = app.get_webview_window("main") else {
        shown.failed();
        return;
    };

    #[cfg(target_os = "windows")]
    {
        let callback_app: AppHandle = app.clone();
        let result = window.clone().run_on_main_thread(move || {
            let shown = callback_app.state::<Shown>();
            while let Some(desired) = shown.next() {
                let Some(image) =
                    image(&callback_app, window.scale_factor().unwrap_or(1.0), desired)
                else {
                    shown.failed();
                    break;
                };
                if window.set_icon(image.clone()).is_err() {
                    shown.failed();
                    break;
                }
                let _ =
                    secs_sys::set_large_icon(&window, image.rgba(), image.width(), image.height());
                shown.mark_shown(desired);
            }
        });
        if result.is_err() {
            shown.failed();
        }
    }
    #[cfg(not(target_os = "windows"))]
    while let Some(desired) = shown.next() {
        let Some(image) = image(app, window.scale_factor().unwrap_or(1.0), desired) else {
            shown.failed();
            break;
        };
        if window.set_icon(image).is_err() {
            shown.failed();
            break;
        }
        shown.mark_shown(desired);
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
    fn queued_icon_callback_uses_the_latest_desired_icon_td115() {
        let mut state = IconState::default();
        assert!(state.request(AppIcon::Civilian));
        assert!(!state.request(AppIcon::Expert));
        assert_eq!(state.desired, AppIcon::Expert);
        assert!(state.queued);
    }

    #[test]
    fn callback_rechecks_a_request_between_read_and_mark_td115() {
        let mut state = IconState::default();
        assert!(state.request(AppIcon::Civilian));
        let callback_icon = state.next().unwrap();
        assert!(!state.request(AppIcon::Expert));
        state.mark_shown(callback_icon);
        let latest = state.next().unwrap();
        state.mark_shown(latest);
        assert_eq!(state.next(), None);
        assert_eq!(state.shown, state.desired);
        assert!(!state.queued);
    }

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
