#![allow(unsafe_code)]

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIcon, DestroyIcon, ICON_BIG, SendMessageW, WM_SETICON,
};

use crate::LargeIconError;

pub(super) fn set_large_icon(
    window: &impl HasWindowHandle,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<(), LargeIconError> {
    let handle = window.window_handle().map_err(|_| LargeIconError::Handle)?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err(LargeIconError::Handle);
    };
    let hwnd = handle.hwnd.get() as *mut core::ffi::c_void;

    let mut bgra = rgba.to_vec();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let and_mask = vec![0; crate::and_mask_len(width, height)];

    // SAFETY: dimensions and the RGBA length were checked by the public API,
    // and the AND mask has the word-aligned rows CreateIcon reads; both
    // buffers remain alive for the call and CreateIcon copies their contents.
    let icon = unsafe {
        CreateIcon(
            core::ptr::null_mut(),
            width as i32,
            height as i32,
            1,
            32,
            and_mask.as_ptr(),
            bgra.as_ptr(),
        )
    };
    if icon.is_null() {
        return Err(LargeIconError::Create);
    }

    // SAFETY: HasWindowHandle guarantees hwnd is valid while its borrow lives,
    // and icon is a live HICON created above.
    let previous = unsafe { SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon as isize) };
    if previous != 0 {
        // SAFETY: ADR 0011 establishes that any replaced large icon was created
        // by an earlier call to this function and is no longer used by the window.
        let _ = unsafe { DestroyIcon(previous as *mut core::ffi::c_void) };
    }
    Ok(())
}
