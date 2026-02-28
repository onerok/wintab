use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WM_TRAY_ICON: u32 = WM_USER + 1;
const TRAY_ICON_ID: u32 = 1;

const IDM_DISABLE: u32 = 1001;
const IDM_EXIT: u32 = 1002;

pub fn add_tray_icon(msg_hwnd: HWND) {
    unsafe {
        let mut tip = [0u16; 128];
        let tip_str: Vec<u16> = "WinTab".encode_utf16().collect();
        tip[..tip_str.len()].copy_from_slice(&tip_str);

        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = msg_hwnd;
        nid.uID = TRAY_ICON_ID;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_ICON;
        nid.hIcon = LoadIconW(0 as _, IDI_APPLICATION);
        nid.szTip = tip;

        Shell_NotifyIconW(NIM_ADD, &nid);

        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &nid);
    }
}

pub fn remove_tray_icon(msg_hwnd: HWND) {
    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = msg_hwnd;
        nid.uID = TRAY_ICON_ID;
        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

pub fn handle_tray_message(msg_hwnd: HWND, lparam: LPARAM) -> bool {
    let event = (lparam & 0xFFFF) as u32;

    match event {
        WM_RBUTTONUP | WM_CONTEXTMENU => {
            show_context_menu(msg_hwnd);
            true
        }
        _ => false,
    }
}

pub fn handle_command(msg_hwnd: HWND, wparam: usize) -> bool {
    let cmd = (wparam & 0xFFFF) as u32;
    match cmd {
        IDM_DISABLE => {
            crate::state::with_state(|s| {
                s.toggle_enabled();
            });
            true
        }
        IDM_EXIT => {
            crate::state::with_state(|s| {
                s.shutdown();
            });
            remove_tray_icon(msg_hwnd);
            unsafe {
                PostQuitMessage(0);
            }
            true
        }
        _ => false,
    }
}

fn show_context_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }

        let enabled = crate::state::with_state(|s| s.enabled);

        let label: Vec<u16> = if enabled {
            "Disable\0"
        } else {
            "Enable\0"
        }
        .encode_utf16()
        .collect();
        AppendMenuW(menu, MF_STRING, IDM_DISABLE as usize, label.as_ptr());

        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

        let exit: Vec<u16> = "Exit\0".encode_utf16().collect();
        AppendMenuW(menu, MF_STRING, IDM_EXIT as usize, exit.as_ptr());

        SetForegroundWindow(hwnd);

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        TrackPopupMenu(
            menu,
            TPM_RIGHTALIGN | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );

        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }
}
