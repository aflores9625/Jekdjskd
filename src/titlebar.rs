//! Tints the *native* Windows title bar to match the Liquid Glass palette.
//!
//! This only recolors the OS-drawn caption (Windows 11 22000+, `DwmSetWindowAttribute`);
//! it does not replace it with custom-drawn chrome. A custom title bar was tried
//! earlier and caused a long tail of hit-testing/overlap bugs - tinting the real
//! one gets a themed look with none of that risk (dragging, snapping, resizing,
//! minimize/maximize all stay native and just work).

use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    DWMWA_USE_IMMERSIVE_DARK_MODE,
};

fn colorref(r: u8, g: u8, b: u8) -> COLORREF {
    // COLORREF is 0x00BBGGRR, not RGB order.
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

/// Apply the theme. Errors are ignored: on Windows 10 (pre-22000) these
/// attributes simply aren't supported and the stock dark title bar remains,
/// which is a fine fallback.
pub fn apply(hwnd_isize: isize) {
    let hwnd = HWND(hwnd_isize as *mut core::ffi::c_void);

    unsafe {
        set_attr(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &1i32);

        // Strictly neutral grays (R=G=B) to match the monochrome theme -
        // any blue-leaning value here reads as purple on some panels.
        let caption = colorref(16, 16, 16);
        set_attr(hwnd, DWMWA_CAPTION_COLOR, &caption);

        let border = colorref(48, 48, 48);
        set_attr(hwnd, DWMWA_BORDER_COLOR, &border);

        let text = colorref(245, 245, 245);
        set_attr(hwnd, DWMWA_TEXT_COLOR, &text);
    }
}

unsafe fn set_attr<T>(hwnd: HWND, attr: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE, value: &T) {
    let _ = DwmSetWindowAttribute(
        hwnd,
        attr,
        value as *const T as *const core::ffi::c_void,
        std::mem::size_of::<T>() as u32,
    );
}
