use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcessId};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Metadata for a tracked window.
#[derive(Clone)]
pub struct WindowInfo {
    pub hwnd: HWND,
    pub title: String,
    pub icon: HICON,
    pub rect: RECT,
}

impl WindowInfo {
    pub fn from_hwnd(hwnd: HWND) -> Option<Self> {
        if !is_eligible(hwnd) {
            return None;
        }
        Some(WindowInfo {
            hwnd,
            title: get_window_title(hwnd),
            icon: get_window_icon(hwnd),
            rect: get_window_rect(hwnd),
        })
    }

    pub fn refresh_title(&mut self) {
        self.title = get_window_title(self.hwnd);
    }

    pub fn refresh_rect(&mut self) {
        self.rect = get_window_rect(self.hwnd);
    }
}

/// Check whether a window should be managed by WinTab.
pub fn is_eligible(hwnd: HWND) -> bool {
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return false;
        }

        // Skip our own process
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == GetCurrentProcessId() {
            return false;
        }

        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

        // Must have a caption (title bar)
        if style & WS_CAPTION != WS_CAPTION {
            return false;
        }

        // Skip child windows
        if style & WS_CHILD != 0 {
            return false;
        }

        // WS_EX_TOOLWINDOW without WS_EX_APPWINDOW → skip
        if ex_style & WS_EX_TOOLWINDOW != 0 && ex_style & WS_EX_APPWINDOW == 0 {
            return false;
        }

        // Skip windows with an owner (owned popups, dialogs)
        let owner = GetWindow(hwnd, GW_OWNER);
        if !owner.is_null() {
            return false;
        }

        // Skip cloaked windows (virtual desktop, UWP hidden)
        let mut cloaked: u32 = 0;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED as u32,
            &mut cloaked as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return false;
        }

        // Skip very small windows
        let rect = get_window_rect(hwnd);
        let w = rect.right - rect.left;
        let h = rect.bottom - rect.top;
        if w < 100 || h < 50 {
            return false;
        }

        true
    }
}

/// Enumerate all eligible top-level windows.
pub fn enumerate_windows() -> Vec<WindowInfo> {
    let mut results: Vec<WindowInfo> = Vec::new();

    unsafe {
        EnumWindows(
            Some(enum_callback),
            &mut results as *mut Vec<WindowInfo> as LPARAM,
        );
    }

    results
}

unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> i32 {
    let results = &mut *(lparam as *mut Vec<WindowInfo>);
    if let Some(info) = WindowInfo::from_hwnd(hwnd) {
        results.push(info);
    }
    TRUE
}

pub fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        OsString::from_wide(&buf[..copied as usize])
            .to_string_lossy()
            .into_owned()
    }
}

pub fn get_window_icon(hwnd: HWND) -> HICON {
    unsafe {
        // Try WM_GETICON (small icon)
        let icon = SendMessageW(hwnd, WM_GETICON, ICON_SMALL as usize, 0);
        if icon != 0 {
            return icon as HICON;
        }

        // Try WM_GETICON (big icon)
        let icon = SendMessageW(hwnd, WM_GETICON, ICON_BIG as usize, 0);
        if icon != 0 {
            return icon as HICON;
        }

        // Try class icon
        let icon = GetClassLongPtrW(hwnd, GCLP_HICONSM);
        if icon != 0 {
            return icon as HICON;
        }

        let icon = GetClassLongPtrW(hwnd, GCLP_HICON);
        if icon != 0 {
            return icon as HICON;
        }

        // Default application icon
        LoadIconW(0 as _, IDI_APPLICATION)
    }
}

pub fn get_window_rect(hwnd: HWND) -> RECT {
    unsafe {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // Prefer DWM extended frame bounds (excludes invisible resize borders)
        let hr = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        );
        if hr != 0 {
            GetWindowRect(hwnd, &mut rect);
        }
        rect
    }
}

/// Check if a window is currently minimized.
pub fn is_minimized(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd) != 0 }
}

/// Check if a window still exists and is valid.
pub fn is_window_valid(hwnd: HWND) -> bool {
    unsafe { IsWindow(hwnd) != 0 }
}
