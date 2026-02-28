use std::collections::HashMap;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::group::{GroupId, GroupManager};
use crate::state;
use crate::window::{self, WindowInfo};

pub const TAB_HEIGHT: i32 = 28;
const TAB_PADDING: i32 = 6;
const ICON_SIZE: i32 = 16;
const MIN_TAB_WIDTH: i32 = 40;
const MAX_TAB_WIDTH: i32 = 200;

const COLOR_ACTIVE: u32 = 0x00A06030;
const COLOR_INACTIVE: u32 = 0x00705040;
const COLOR_HOVER: u32 = 0x00C08050;
const COLOR_TEXT: u32 = 0x00FFFFFF;

static OVERLAY_CLASS_UTF16: &[u16] = &[
    b'W' as u16, b'i' as u16, b'n' as u16, b'T' as u16, b'a' as u16, b'b' as u16,
    b'O' as u16, b'v' as u16, b'e' as u16, b'r' as u16, b'l' as u16, b'a' as u16,
    b'y' as u16, 0,
];

#[repr(C)]
struct OverlayData {
    group_id: GroupId,
    hover_tab: i32,
}

fn get_x_lparam(lparam: isize) -> i32 {
    (lparam & 0xFFFF) as i16 as i32
}

fn get_y_lparam(lparam: isize) -> i32 {
    ((lparam >> 16) & 0xFFFF) as i16 as i32
}

pub fn register_class() {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: std::mem::size_of::<usize>() as i32,
            hInstance: instance,
            hIcon: 0 as _,
            hCursor: LoadCursorW(0 as _, IDC_ARROW),
            hbrBackground: 0 as _,
            lpszMenuName: std::ptr::null(),
            lpszClassName: OVERLAY_CLASS_UTF16.as_ptr(),
            hIconSm: 0 as _,
        };
        RegisterClassExW(&wc);
    }
}

pub fn create_overlay(group_id: GroupId) -> HWND {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            OVERLAY_CLASS_UTF16.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0, 0, 100, TAB_HEIGHT,
            0 as _, 0 as _, instance, std::ptr::null(),
        );

        if !hwnd.is_null() {
            let data = Box::new(OverlayData {
                group_id,
                hover_tab: -1,
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(data) as isize);
        }
        hwnd
    }
}

pub fn destroy_overlay(hwnd: HWND) {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayData;
        if !ptr.is_null() {
            drop(Box::from_raw(ptr));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
        DestroyWindow(hwnd);
    }
}

/// Reposition and repaint an overlay. Takes disjoint fields to avoid borrow conflicts.
pub fn update_overlay(
    overlay_hwnd: HWND,
    group_id: GroupId,
    groups: &GroupManager,
    windows: &HashMap<HWND, WindowInfo>,
) {
    let Some(group) = groups.groups.get(&group_id) else {
        return;
    };

    if group.tabs.is_empty() {
        return;
    }

    if window::is_minimized(group.active_hwnd()) {
        unsafe {
            ShowWindow(overlay_hwnd, SW_HIDE);
        }
        return;
    }

    let rect = group.active_rect();
    let width = rect.right - rect.left;

    unsafe {
        SetWindowPos(
            overlay_hwnd,
            HWND_TOPMOST,
            rect.left,
            rect.top - TAB_HEIGHT,
            width,
            TAB_HEIGHT,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }

    paint_tabs(overlay_hwnd, group, &rect, windows);
}

/// Standalone version for calls from overlay wndproc (outside with_state).
pub fn update_overlay_standalone(overlay_hwnd: HWND, group_id: GroupId) {
    state::with_state(|s| {
        update_overlay(overlay_hwnd, group_id, &s.groups, &s.windows);
    });
}

fn paint_tabs(
    overlay_hwnd: HWND,
    group: &crate::group::TabGroup,
    rect: &RECT,
    windows: &HashMap<HWND, WindowInfo>,
) {
    unsafe {
        let width = (rect.right - rect.left).max(1);
        let height = TAB_HEIGHT;

        let hdc_screen = GetDC(0 as _);
        let hdc_mem = CreateCompatibleDC(hdc_screen);

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB as u32,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbmp = CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, 0 as _, 0);
        if hbmp.is_null() || bits.is_null() {
            DeleteDC(hdc_mem);
            ReleaseDC(0 as _, hdc_screen);
            return;
        }
        let old_bmp = SelectObject(hdc_mem, hbmp);

        let pixel_count = (width * height) as usize;
        std::ptr::write_bytes(bits as *mut u32, 0, pixel_count);

        let hover_tab = {
            let ptr = GetWindowLongPtrW(overlay_hwnd, GWLP_USERDATA) as *const OverlayData;
            if !ptr.is_null() {
                (*ptr).hover_tab
            } else {
                -1
            }
        };

        let tab_count = group.tabs.len() as i32;
        let tab_width = if tab_count > 0 {
            (width / tab_count).clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH)
        } else {
            MIN_TAB_WIDTH
        };

        let font_name: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
        let font = CreateFontW(
            14, 0, 0, 0,
            FW_NORMAL as i32, 0, 0, 0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            (DEFAULT_PITCH | FF_SWISS) as u32,
            font_name.as_ptr(),
        );
        let old_font = SelectObject(hdc_mem, font);
        SetBkMode(hdc_mem, TRANSPARENT as i32);
        SetTextColor(hdc_mem, COLOR_TEXT);

        for (i, &hwnd) in group.tabs.iter().enumerate() {
            let x = i as i32 * tab_width;
            let is_active = i == group.active;
            let is_hover = i as i32 == hover_tab;

            let color = if is_hover {
                COLOR_HOVER
            } else if is_active {
                COLOR_ACTIVE
            } else {
                COLOR_INACTIVE
            };

            fill_rect_alpha(
                bits as *mut u32,
                width,
                height,
                x,
                0,
                tab_width,
                height,
                color,
                if is_active { 220 } else { 160 },
            );

            let info = windows.get(&hwnd);
            if let Some(info) = info {
                if !info.icon.is_null() {
                    DrawIconEx(
                        hdc_mem,
                        x + TAB_PADDING,
                        (height - ICON_SIZE) / 2,
                        info.icon,
                        ICON_SIZE,
                        ICON_SIZE,
                        0,
                        0 as _,
                        DI_NORMAL,
                    );
                }

                let text: Vec<u16> = info.title.encode_utf16().chain(std::iter::once(0)).collect();
                let mut text_rect = RECT {
                    left: x + TAB_PADDING + ICON_SIZE + 4,
                    top: 0,
                    right: x + tab_width - TAB_PADDING,
                    bottom: height,
                };
                DrawTextW(
                    hdc_mem,
                    text.as_ptr(),
                    text.len() as i32 - 1,
                    &mut text_rect,
                    DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
            }
        }

        let pt_src = POINT { x: 0, y: 0 };
        let size = SIZE { cx: width, cy: height };
        let pt_dst = POINT {
            x: rect.left,
            y: rect.top - TAB_HEIGHT,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        UpdateLayeredWindow(
            overlay_hwnd, hdc_screen, &pt_dst, &size,
            hdc_mem, &pt_src, 0, &blend, ULW_ALPHA,
        );

        SelectObject(hdc_mem, old_font);
        DeleteObject(font);
        SelectObject(hdc_mem, old_bmp);
        DeleteObject(hbmp);
        DeleteDC(hdc_mem);
        ReleaseDC(0 as _, hdc_screen);
    }
}

/// Compute a premultiplied ARGB pixel from an RGB color and alpha value.
fn premultiply_pixel(color: u32, alpha: u8) -> u32 {
    let r = color & 0xFF;
    let g = (color >> 8) & 0xFF;
    let b = (color >> 16) & 0xFF;
    let a = alpha as u32;

    let pr = (r * a / 255) & 0xFF;
    let pg = (g * a / 255) & 0xFF;
    let pb = (b * a / 255) & 0xFF;
    (a << 24) | (pr << 16) | (pg << 8) | pb
}

/// Calculate which tab index an x coordinate falls on, given total width and tab count.
fn calculate_tab_index(x: i32, width: i32, tab_count: i32) -> Option<usize> {
    if tab_count <= 0 || width <= 0 {
        return None;
    }
    let tab_width = (width / tab_count).clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH);
    let index = x / tab_width;
    if index >= 0 && index < tab_count {
        Some(index as usize)
    } else {
        None
    }
}

/// Fill a rectangle in a 32-bit ARGB pixel buffer with premultiplied alpha.
fn fill_rect_alpha(
    pixels: *mut u32,
    stride: i32,
    buf_height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
    alpha: u8,
) {
    let pixel = premultiply_pixel(color, alpha);

    // Clamp ranges to buffer bounds
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(stride);
    let y1 = (y + h).min(buf_height);

    unsafe {
        for row in y0..y1 {
            for col in x0..x1 {
                *pixels.offset((row * stride + col) as isize) = pixel;
            }
        }
    }
}

pub fn hit_test_tab(overlay_hwnd: HWND, x: i32) -> Option<(GroupId, usize)> {
    unsafe {
        let ptr = GetWindowLongPtrW(overlay_hwnd, GWLP_USERDATA) as *const OverlayData;
        if ptr.is_null() {
            return None;
        }
        let data = &*ptr;
        let group_id = data.group_id;

        state::with_state(|s| {
            let group = s.groups.groups.get(&group_id)?;
            if group.tabs.is_empty() {
                return None;
            }
            let rect = group.active_rect();
            let width = rect.right - rect.left;
            let tab_count = group.tabs.len() as i32;
            let index = calculate_tab_index(x, width, tab_count)?;
            Some((group_id, index))
        })
    }
}

fn set_hover_tab(overlay_hwnd: HWND, tab_index: i32) {
    unsafe {
        let ptr = GetWindowLongPtrW(overlay_hwnd, GWLP_USERDATA) as *mut OverlayData;
        if ptr.is_null() {
            return;
        }
        let data = &mut *ptr;
        if data.hover_tab != tab_index {
            data.hover_tab = tab_index;
            // Called from wndproc (outside with_state), safe to use standalone
            update_overlay_standalone(overlay_hwnd, data.group_id);
        }
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            let x = get_x_lparam(lparam);
            if let Some((group_id, tab_index)) = hit_test_tab(hwnd, x) {
                crate::drag::on_mouse_down(hwnd, group_id, tab_index, x, get_y_lparam(lparam));
            }
            0
        }
        WM_MOUSEMOVE => {
            let x = get_x_lparam(lparam);

            if let Some((_gid, tab_index)) = hit_test_tab(hwnd, x) {
                set_hover_tab(hwnd, tab_index as i32);
            } else {
                set_hover_tab(hwnd, -1);
            }

            crate::drag::on_mouse_move(hwnd, x, get_y_lparam(lparam));

            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut tme);
            0
        }
        WM_MOUSELEAVE => {
            set_hover_tab(hwnd, -1);
            0
        }
        WM_LBUTTONUP => {
            crate::drag::on_mouse_up(hwnd, get_x_lparam(lparam), get_y_lparam(lparam));
            0
        }
        WM_NCHITTEST => HTCLIENT as LRESULT,
        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayData;
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Manages overlay windows mapped to groups.
pub struct OverlayManager {
    pub overlays: HashMap<GroupId, HWND>,
}

impl OverlayManager {
    pub fn new() -> Self {
        OverlayManager {
            overlays: HashMap::new(),
        }
    }

    pub fn ensure_overlay(&mut self, group_id: GroupId) -> HWND {
        *self.overlays.entry(group_id).or_insert_with(|| create_overlay(group_id))
    }

    pub fn remove_overlay(&mut self, group_id: GroupId) {
        if let Some(hwnd) = self.overlays.remove(&group_id) {
            destroy_overlay(hwnd);
        }
    }

    /// Update or remove overlay for a group (handles dissolved groups).
    pub fn refresh_overlay(
        &mut self,
        group_id: GroupId,
        groups: &GroupManager,
        windows: &HashMap<HWND, WindowInfo>,
    ) {
        if !groups.groups.contains_key(&group_id) {
            self.remove_overlay(group_id);
        } else if let Some(&ov) = self.overlays.get(&group_id) {
            update_overlay(ov, group_id, groups, windows);
        }
    }

    pub fn update_all(
        &self,
        groups: &GroupManager,
        windows: &HashMap<HWND, WindowInfo>,
    ) {
        for (&gid, &overlay) in &self.overlays {
            update_overlay(overlay, gid, groups, windows);
        }
    }

    pub fn destroy_all(&mut self) {
        for (_, hwnd) in self.overlays.drain() {
            destroy_overlay(hwnd);
        }
    }

    pub fn group_for_overlay(&self, overlay_hwnd: HWND) -> Option<GroupId> {
        self.overlays
            .iter()
            .find(|(_, &v)| v == overlay_hwnd)
            .map(|(&k, _)| k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_hwnd(n: usize) -> HWND {
        n as HWND
    }

    // --- get_x_lparam / get_y_lparam ---

    #[test]
    fn get_x_lparam_extracts_low_word() {
        // lparam packs x in low 16 bits, y in high 16 bits
        let lparam: isize = 150 | (200 << 16);
        assert_eq!(get_x_lparam(lparam), 150);
    }

    #[test]
    fn get_y_lparam_extracts_high_word() {
        let lparam: isize = 150 | (200 << 16);
        assert_eq!(get_y_lparam(lparam), 200);
    }

    #[test]
    fn get_x_lparam_handles_negative() {
        // Negative coordinates (e.g. -10) are stored as signed i16 in low word
        let x: i16 = -10;
        let lparam: isize = (x as u16 as isize) | (50 << 16);
        assert_eq!(get_x_lparam(lparam), -10);
    }

    #[test]
    fn get_y_lparam_handles_negative() {
        let y: i16 = -20;
        let lparam: isize = 50 | ((y as u16 as isize) << 16);
        assert_eq!(get_y_lparam(lparam), -20);
    }

    // --- premultiply_pixel ---

    #[test]
    fn premultiply_pixel_full_alpha() {
        // color=0x00FF0000 (blue=FF in our BGR layout), alpha=255
        // r = 0x00, g = 0x00, b = 0xFF
        // pr=0, pg=0, pb=255
        // result = (255 << 24) | (0 << 16) | (0 << 8) | 255
        let pixel = premultiply_pixel(0x00FF0000, 255);
        assert_eq!(pixel >> 24, 255); // alpha channel
        assert_eq!(pixel & 0xFF, 255); // blue channel (pb)
    }

    #[test]
    fn premultiply_pixel_zero_alpha() {
        let pixel = premultiply_pixel(0x00FFFFFF, 0);
        assert_eq!(pixel, 0); // All channels should be 0
    }

    #[test]
    fn premultiply_pixel_half_alpha() {
        // color = 0x000000FF (r=0xFF, g=0, b=0), alpha=128
        // pr = (255 * 128 / 255) = 128
        let pixel = premultiply_pixel(0x000000FF, 128);
        let a = pixel >> 24;
        let pr = (pixel >> 16) & 0xFF;
        assert_eq!(a, 128);
        assert_eq!(pr, 128);
    }

    // --- calculate_tab_index ---

    #[test]
    fn calculate_tab_index_first_tab() {
        // 3 tabs in 600px width → 200px each (clamped to MAX_TAB_WIDTH=200)
        assert_eq!(calculate_tab_index(10, 600, 3), Some(0));
    }

    #[test]
    fn calculate_tab_index_second_tab() {
        assert_eq!(calculate_tab_index(250, 600, 3), Some(1));
    }

    #[test]
    fn calculate_tab_index_last_tab() {
        assert_eq!(calculate_tab_index(450, 600, 3), Some(2));
    }

    #[test]
    fn calculate_tab_index_out_of_range() {
        // x beyond all tabs
        assert_eq!(calculate_tab_index(700, 600, 3), None);
    }

    #[test]
    fn calculate_tab_index_zero_tabs() {
        assert_eq!(calculate_tab_index(10, 600, 0), None);
    }

    #[test]
    fn calculate_tab_index_zero_width() {
        assert_eq!(calculate_tab_index(10, 0, 3), None);
    }

    #[test]
    fn calculate_tab_index_clamps_to_min_width() {
        // 1 tab in 20px (below MIN_TAB_WIDTH=40) → tab_width clamped to 40
        // x=10 / 40 = 0, which is < 1 → Some(0)
        assert_eq!(calculate_tab_index(10, 20, 1), Some(0));
    }

    #[test]
    fn calculate_tab_index_slightly_negative_x_maps_to_first() {
        // -5 / 200 = 0 in integer division, so slightly negative x maps to tab 0
        assert_eq!(calculate_tab_index(-5, 600, 3), Some(0));
    }

    #[test]
    fn calculate_tab_index_very_negative_x() {
        // -250 / 200 = -1, which is < 0 → None
        assert_eq!(calculate_tab_index(-250, 600, 3), None);
    }

    // --- fill_rect_alpha ---

    #[test]
    fn fill_rect_alpha_writes_correct_region() {
        let mut buf = vec![0u32; 10 * 5]; // 10 wide, 5 tall
        fill_rect_alpha(buf.as_mut_ptr(), 10, 5, 2, 1, 3, 2, 0x000000FF, 255);

        // Pixels inside the rect should be non-zero
        assert_ne!(buf[1 * 10 + 2], 0); // (2,1)
        assert_ne!(buf[1 * 10 + 3], 0); // (3,1)
        assert_ne!(buf[1 * 10 + 4], 0); // (4,1)
        assert_ne!(buf[2 * 10 + 2], 0); // (2,2)

        // Pixels outside should remain zero
        assert_eq!(buf[0 * 10 + 0], 0); // (0,0)
        assert_eq!(buf[0 * 10 + 2], 0); // (2,0) - above rect
        assert_eq!(buf[1 * 10 + 5], 0); // (5,1) - right of rect
        assert_eq!(buf[3 * 10 + 2], 0); // (2,3) - below rect
    }

    #[test]
    fn fill_rect_alpha_clamps_to_bounds() {
        let mut buf = vec![0u32; 4 * 4]; // 4x4
        // Rect extends beyond buffer: x=2, w=5 → clamps to x1=4
        fill_rect_alpha(buf.as_mut_ptr(), 4, 4, 2, 0, 5, 2, 0x000000FF, 255);

        assert_ne!(buf[0 * 4 + 2], 0); // (2,0) - inside
        assert_ne!(buf[0 * 4 + 3], 0); // (3,0) - inside (edge)
        assert_eq!(buf[0 * 4 + 0], 0); // (0,0) - outside
    }

    // --- OverlayManager ---

    #[test]
    fn group_for_overlay_finds_match() {
        let mut om = OverlayManager::new();
        om.overlays.insert(42, fake_hwnd(100));
        assert_eq!(om.group_for_overlay(fake_hwnd(100)), Some(42));
    }

    #[test]
    fn group_for_overlay_returns_none_when_empty() {
        let om = OverlayManager::new();
        assert_eq!(om.group_for_overlay(fake_hwnd(999)), None);
    }

    #[test]
    fn group_for_overlay_returns_none_for_mismatch() {
        let mut om = OverlayManager::new();
        om.overlays.insert(42, fake_hwnd(100));
        assert_eq!(om.group_for_overlay(fake_hwnd(200)), None);
    }
}
