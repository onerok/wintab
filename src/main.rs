#![windows_subsystem = "windows"]

mod appdata;
mod config;
mod drag;
mod group;
mod hook;
mod overlay;
mod position_store;
mod preview;
mod state;
mod tray;
mod vdesktop;
mod window;

#[cfg(test)]
mod acceptance;
#[cfg(test)]
mod screenshot;

use std::panic;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static MSG_WINDOW_CLASS: &[u16] = &[
    b'W' as u16, b'i' as u16, b'n' as u16, b'T' as u16, b'a' as u16, b'b' as u16,
    b'M' as u16, b's' as u16, b'g' as u16, 0,
];

fn main() {
    // Safety net: show all hidden windows if we panic
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        // Use try_with_state to avoid double-panic if RefCell is already borrowed
        state::try_with_state(|s| {
            s.groups.show_all_windows();
        });
        default_hook(info);
    }));

    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);

        register_msg_window_class();
        overlay::register_class();
        preview::register_class();

        let msg_hwnd = create_msg_window();

        state::with_state(|s| {
            s.init();
        });

        hook::install();
        SetTimer(msg_hwnd, 1, 60, None);
        tray::add_tray_icon(msg_hwnd);

        // Message loop
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0 as _, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup
        KillTimer(msg_hwnd, 1);
        hook::uninstall();
        tray::remove_tray_icon(msg_hwnd);
        state::with_state(|s| {
            s.shutdown();
        });
    }
}

fn register_msg_window_class() {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(msg_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: 0 as _,
            hCursor: 0 as _,
            hbrBackground: 0 as _,
            lpszMenuName: std::ptr::null(),
            lpszClassName: MSG_WINDOW_CLASS.as_ptr(),
            hIconSm: 0 as _,
        };
        RegisterClassExW(&wc);
    }
}

fn create_msg_window() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());

        CreateWindowExW(
            0,
            MSG_WINDOW_CLASS.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            0 as _,
            instance,
            std::ptr::null(),
        )
    }
}

unsafe extern "system" fn msg_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> LRESULT {
    match msg {
        m if m == tray::WM_TRAY_ICON => {
            tray::handle_tray_message(hwnd, lparam);
            0
        }
        WM_COMMAND => {
            if tray::handle_command(hwnd, wparam) {
                0
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_TIMER => {
            state::with_state(|s| s.on_peek_timer());
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
