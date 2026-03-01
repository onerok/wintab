use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::*;
#[cfg(not(test))]
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::overlay;

pub const PREVIEW_TIMER_ID: usize = 42;
const PREVIEW_DELAY_MS: u32 = 500;
const PREVIEW_WIDTH: i32 = 300;
const PREVIEW_MAX_HEIGHT: i32 = 400;
const PREVIEW_OPACITY: u8 = 200;

static PREVIEW_CLASS_UTF16: &[u16] = &[
    b'W' as u16,
    b'i' as u16,
    b'n' as u16,
    b'T' as u16,
    b'a' as u16,
    b'b' as u16,
    b'P' as u16,
    b'r' as u16,
    b'v' as u16,
    0,
];

pub fn register_class() {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(DefWindowProcW),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: 0 as _,
            hCursor: 0 as _,
            hbrBackground: 0 as _,
            lpszMenuName: std::ptr::null(),
            lpszClassName: PREVIEW_CLASS_UTF16.as_ptr(),
            hIconSm: 0 as _,
        };
        RegisterClassExW(&wc);
    }
}

pub struct PreviewManager {
    /// Reusable preview popup window (created once).
    preview_hwnd: HWND,
    /// Active DWM thumbnail handle (0 when inactive).
    thumbnail: isize,
    /// The tab HWND currently being previewed.
    active_tab_hwnd: HWND,
    /// Overlay that has the pending timer.
    pending_overlay: HWND,
    /// Tab HWND the timer is waiting to preview.
    pending_tab_hwnd: HWND,
}

impl PreviewManager {
    pub fn new() -> Self {
        PreviewManager {
            preview_hwnd: std::ptr::null_mut(),
            thumbnail: 0,
            active_tab_hwnd: std::ptr::null_mut(),
            pending_overlay: std::ptr::null_mut(),
            pending_tab_hwnd: std::ptr::null_mut(),
        }
    }

    fn ensure_preview_window(&mut self) -> HWND {
        if self.preview_hwnd.is_null() {
            self.preview_hwnd = create_preview_window();
        }
        self.preview_hwnd
    }

    /// Start the delay timer for showing a preview of `tab_hwnd`.
    pub fn start_delay(&mut self, overlay_hwnd: HWND, tab_hwnd: HWND) {
        // If already previewing this exact tab, do nothing
        if self.active_tab_hwnd == tab_hwnd && self.thumbnail != 0 {
            return;
        }
        // If timer already pending for same tab, do nothing
        if self.pending_overlay == overlay_hwnd && self.pending_tab_hwnd == tab_hwnd {
            return;
        }
        // Cancel any existing timer/preview for different tab
        if !self.pending_overlay.is_null() {
            unsafe {
                KillTimer(self.pending_overlay, PREVIEW_TIMER_ID);
            }
        }
        if self.thumbnail != 0 {
            self.hide();
        }
        self.pending_overlay = overlay_hwnd;
        self.pending_tab_hwnd = tab_hwnd;
        unsafe {
            SetTimer(overlay_hwnd, PREVIEW_TIMER_ID, PREVIEW_DELAY_MS, None);
        }
    }

    /// Cancel pending delay timer.
    pub fn cancel_delay(&mut self, overlay_hwnd: HWND) {
        if self.pending_overlay == overlay_hwnd {
            unsafe {
                KillTimer(overlay_hwnd, PREVIEW_TIMER_ID);
            }
            self.pending_overlay = std::ptr::null_mut();
            self.pending_tab_hwnd = std::ptr::null_mut();
        }
    }

    /// Called when WM_TIMER fires for PREVIEW_TIMER_ID.
    /// Must be called with access to AppState fields (groups, overlays) since
    /// it needs to validate that the tab is still inactive and compute positioning.
    pub fn on_timer(
        &mut self,
        overlay_hwnd: HWND,
        groups: &crate::group::GroupManager,
        overlays: &crate::overlay::OverlayManager,
    ) {
        // Kill the one-shot timer
        unsafe {
            KillTimer(overlay_hwnd, PREVIEW_TIMER_ID);
        }

        let tab_hwnd = self.pending_tab_hwnd;
        self.pending_overlay = std::ptr::null_mut();
        self.pending_tab_hwnd = std::ptr::null_mut();

        if tab_hwnd.is_null() {
            return;
        }

        // Validate: tab must still be in a group and not be the active tab
        let should_show = (|| {
            let gid = groups.group_of(tab_hwnd)?;
            let group = groups.groups.get(&gid)?;
            if group.active_hwnd() == tab_hwnd {
                return None; // Now active — don't show preview
            }
            // Get overlay rect for positioning
            let _ov = *overlays.overlays.get(&gid)?;
            let rect = group.active_rect();
            let width = rect.right - rect.left;
            let tab_count = group.tabs.len() as i32;
            let tab_index = group.tabs.iter().position(|&h| h == tab_hwnd)? as i32;
            Some((rect, width, tab_count, tab_index))
        })();

        if let Some((rect, width, tab_count, tab_index)) = should_show {
            let tab_width = if tab_count > 0 {
                (width / tab_count).clamp(overlay::MIN_TAB_WIDTH, overlay::MAX_TAB_WIDTH)
            } else {
                return;
            };
            let tab_x_start = tab_index * tab_width;
            let tab_x_mid = tab_x_start + tab_width / 2;
            let screen_x_mid = rect.left + tab_x_mid;
            let overlay_bottom = rect.top; // overlay sits above the window at rect.top - TAB_HEIGHT

            self.show(tab_hwnd, screen_x_mid, overlay_bottom);
        }
    }

    /// Show the DWM thumbnail preview for `src_hwnd`.
    fn show(&mut self, src_hwnd: HWND, tab_center_x: i32, below_y: i32) {
        // Hide any existing preview first
        if self.thumbnail != 0 {
            self.hide();
        }

        let dest = self.ensure_preview_window();
        if dest.is_null() {
            return;
        }

        // Register DWM thumbnail
        let mut hthumbnail: isize = 0;
        let hr = unsafe { DwmRegisterThumbnail(dest, src_hwnd, &mut hthumbnail) };
        if hr < 0 || hthumbnail == 0 {
            return;
        }

        // Query source size for aspect ratio
        let mut src_size: SIZE = SIZE { cx: 0, cy: 0 };
        let hr = unsafe { DwmQueryThumbnailSourceSize(hthumbnail, &mut src_size) };
        if hr < 0 || src_size.cx <= 0 || src_size.cy <= 0 {
            unsafe {
                DwmUnregisterThumbnail(hthumbnail);
            }
            return;
        }

        let (preview_w, preview_h) = compute_preview_size(src_size.cx, src_size.cy);

        // Position: centered on tab, below tab bar
        let preview_left = tab_center_x - preview_w / 2;
        let preview_top = below_y;

        // Clamp to monitor work area
        let (clamped_left, clamped_top) = clamp_to_monitor(
            preview_left,
            preview_top,
            preview_w,
            preview_h,
            tab_center_x,
            below_y,
        );

        // Position and show the preview window
        unsafe {
            SetWindowPos(
                dest,
                HWND_TOPMOST,
                clamped_left,
                clamped_top,
                preview_w,
                preview_h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }

        // Update DWM thumbnail properties
        let props = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_OPACITY | DWM_TNP_VISIBLE,
            rcDestination: RECT {
                left: 0,
                top: 0,
                right: preview_w,
                bottom: preview_h,
            },
            rcSource: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            opacity: PREVIEW_OPACITY,
            fVisible: TRUE,
            fSourceClientAreaOnly: FALSE,
        };
        unsafe {
            DwmUpdateThumbnailProperties(hthumbnail, &props);
        }

        self.thumbnail = hthumbnail;
        self.active_tab_hwnd = src_hwnd;
    }

    /// Hide the current preview.
    pub fn hide(&mut self) {
        if self.thumbnail != 0 {
            unsafe {
                DwmUnregisterThumbnail(self.thumbnail);
            }
            self.thumbnail = 0;
        }
        self.active_tab_hwnd = std::ptr::null_mut();
        if !self.preview_hwnd.is_null() {
            unsafe {
                ShowWindow(self.preview_hwnd, SW_HIDE);
            }
        }
    }

    /// Returns true if a DWM thumbnail preview is currently active.
    #[cfg(test)]
    pub fn is_showing(&self) -> bool {
        self.thumbnail != 0
    }

    /// Returns the preview popup HWND (for test visibility checks / screenshots).
    #[cfg(test)]
    pub fn preview_hwnd(&self) -> HWND {
        self.preview_hwnd
    }

    /// Destroy the preview window entirely (for shutdown).
    pub fn destroy(&mut self) {
        self.hide();
        if !self.pending_overlay.is_null() {
            unsafe {
                KillTimer(self.pending_overlay, PREVIEW_TIMER_ID);
            }
            self.pending_overlay = std::ptr::null_mut();
            self.pending_tab_hwnd = std::ptr::null_mut();
        }
        if !self.preview_hwnd.is_null() {
            unsafe {
                DestroyWindow(self.preview_hwnd);
            }
            self.preview_hwnd = std::ptr::null_mut();
        }
    }
}

fn create_preview_window() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            PREVIEW_CLASS_UTF16.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            0 as _,
            0 as _,
            instance,
            std::ptr::null(),
        )
    }
}

/// Compute preview dimensions preserving aspect ratio.
/// Returns (width, height) clamped to PREVIEW_WIDTH and PREVIEW_MAX_HEIGHT.
fn compute_preview_size(src_w: i32, src_h: i32) -> (i32, i32) {
    if src_w <= 0 || src_h <= 0 {
        return (PREVIEW_WIDTH, PREVIEW_WIDTH);
    }
    let w = PREVIEW_WIDTH;
    let h = (src_h as f64 / src_w as f64 * w as f64).round() as i32;
    let h = h.clamp(1, PREVIEW_MAX_HEIGHT);
    (w, h)
}

/// Clamp preview position to the monitor work area.
fn clamp_to_monitor(
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    ref_x: i32,
    ref_y: i32,
) -> (i32, i32) {
    #[cfg(not(test))]
    {
        let pt = POINT { x: ref_x, y: ref_y };
        unsafe {
            let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            if monitor.is_null() {
                return (left, top);
            }
            let mut mi: MONITORINFO = std::mem::zeroed();
            mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut mi) == 0 {
                return (left, top);
            }
            let work = mi.rcWork;
            let mut x = left;
            let mut y = top;
            // Clamp right edge
            if x + width > work.right {
                x = work.right - width;
            }
            // Clamp left edge
            if x < work.left {
                x = work.left;
            }
            // Clamp bottom edge
            if y + height > work.bottom {
                y = work.bottom - height;
            }
            // Clamp top edge
            if y < work.top {
                y = work.top;
            }
            (x, y)
        }
    }
    #[cfg(test)]
    {
        let _ = (ref_x, ref_y);
        clamp_to_rect(
            left,
            top,
            width,
            height,
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
        )
    }
}

/// Pure clamping logic (testable).
#[cfg(test)]
fn clamp_to_rect(left: i32, top: i32, width: i32, height: i32, work: RECT) -> (i32, i32) {
    let mut x = left;
    let mut y = top;
    if x + width > work.right {
        x = work.right - width;
    }
    if x < work.left {
        x = work.left;
    }
    if y + height > work.bottom {
        y = work.bottom - height;
    }
    if y < work.top {
        y = work.top;
    }
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_ratio_landscape() {
        // 1920x1080 → 300 wide, height = 1080/1920 * 300 = 168.75 → 169
        let (w, h) = compute_preview_size(1920, 1080);
        assert_eq!(w, 300);
        assert_eq!(h, 169);
    }

    #[test]
    fn aspect_ratio_portrait() {
        // 1080x1920 → 300 wide, height = 1920/1080 * 300 = 533 → capped at 400
        let (w, h) = compute_preview_size(1080, 1920);
        assert_eq!(w, 300);
        assert_eq!(h, 400);
    }

    #[test]
    fn aspect_ratio_square() {
        let (w, h) = compute_preview_size(500, 500);
        assert_eq!(w, 300);
        assert_eq!(h, 300);
    }

    #[test]
    fn aspect_ratio_zero_source() {
        let (w, h) = compute_preview_size(0, 0);
        assert_eq!(w, 300);
        assert_eq!(h, 300);
    }

    #[test]
    fn aspect_ratio_4k() {
        // 3840x2160 → 300 wide, height = 2160/3840 * 300 = 168.75 → 169
        let (w, h) = compute_preview_size(3840, 2160);
        assert_eq!(w, 300);
        assert_eq!(h, 169);
    }

    #[test]
    fn clamp_fits_within_monitor() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let (x, y) = clamp_to_rect(100, 200, 300, 169, work);
        assert_eq!(x, 100);
        assert_eq!(y, 200);
    }

    #[test]
    fn clamp_right_overflow() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let (x, _y) = clamp_to_rect(1800, 200, 300, 169, work);
        assert_eq!(x, 1620); // 1920 - 300
    }

    #[test]
    fn clamp_left_overflow() {
        let work = RECT {
            left: 100,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let (x, _y) = clamp_to_rect(50, 200, 300, 169, work);
        assert_eq!(x, 100);
    }

    #[test]
    fn clamp_bottom_overflow() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let (_x, y) = clamp_to_rect(100, 1000, 300, 169, work);
        assert_eq!(y, 911); // 1080 - 169
    }

    #[test]
    fn clamp_top_overflow() {
        let work = RECT {
            left: 0,
            top: 50,
            right: 1920,
            bottom: 1080,
        };
        let (_x, y) = clamp_to_rect(100, 30, 300, 169, work);
        assert_eq!(y, 50);
    }
}
