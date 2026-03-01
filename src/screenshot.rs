//! Screen capture for E2E test evidence.
//!
//! Uses Win32 GDI BitBlt with CAPTUREBLT to capture screen regions
//! (including WS_EX_LAYERED overlays) and saves as PNG via the `image` crate.
//!
//! For reliable screenshots, run acceptance tests single-threaded:
//!   cargo test -- --test-threads=1

use std::path::Path;

use image::{ImageBuffer, Rgba};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;

use crate::overlay::TAB_HEIGHT;
use crate::window;

/// Capture a screen region and save as PNG.
///
/// Creates parent directories automatically. Panics on failure.
pub fn capture_region(x: i32, y: i32, w: i32, h: i32, path: &str) {
    assert!(
        w > 0 && h > 0,
        "capture dimensions must be positive: {}x{}",
        w,
        h
    );

    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).expect("Failed to create evidence directory");
    }

    unsafe {
        let hdc_screen = GetDC(std::ptr::null_mut());
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let hbm = CreateCompatibleBitmap(hdc_screen, w, h);
        let old = SelectObject(hdc_mem, hbm);

        // CAPTUREBLT captures WS_EX_LAYERED and WS_EX_TOPMOST windows
        BitBlt(hdc_mem, 0, 0, w, h, hdc_screen, x, y, SRCCOPY | CAPTUREBLT);

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h; // negative = top-down rows (no flip needed)
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut pixels = vec![0u8; (w * h * 4) as usize];
        GetDIBits(
            hdc_mem,
            hbm,
            0,
            h as u32,
            pixels.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_mem, old);
        DeleteObject(hbm);
        DeleteDC(hdc_mem);
        ReleaseDC(std::ptr::null_mut(), hdc_screen);

        // BGRA → RGBA
        for px in pixels.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let img = ImageBuffer::<Rgba<u8>, _>::from_raw(w as u32, h as u32, pixels)
            .expect("Failed to create image buffer");
        img.save(path)
            .unwrap_or_else(|e| panic!("Failed to save screenshot to {}: {}", path, e));
    }
}

/// Capture a window with surrounding context, including the overlay tab bar above it.
///
/// Adds 10px margin around the window and TAB_HEIGHT extra space above for the tab bar.
pub fn capture_window(hwnd: HWND, path: &str) {
    let margin = 10;
    let rect = window::get_window_rect(hwnd);
    let x = rect.left - margin;
    let y = rect.top - TAB_HEIGHT - margin;
    let w = (rect.right - rect.left) + margin * 2;
    let h = (rect.bottom - rect.top) + TAB_HEIGHT + margin * 2;
    capture_region(x, y, w, h, path);
}
