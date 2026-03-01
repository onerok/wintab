//! Dummy window spawner for E2E tests.
//!
//! Creates N visible Win32 windows with a known title prefix,
//! then waits for stdin to close before exiting.
//!
//! Usage: dummy_window.exe [count] [title_prefix]

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{ptr, thread};

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let count: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
    let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("Dummy");

    // Register window class
    let class_name: Vec<u16> = "DummyWindowClass\0".encode_utf16().collect();
    unsafe {
        let instance = GetModuleHandleW(ptr::null());
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
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: 0 as _,
        };
        RegisterClassExW(&wc);
    }

    // Create windows
    let mut hwnds = Vec::new();
    for i in 0..count {
        let title = format!("{} {}\0", prefix, i + 1);
        let title_wide: Vec<u16> = title.encode_utf16().collect();
        let x = 100 + (i as i32) * 50;
        let y = 100 + (i as i32) * 50;
        let hwnd = unsafe {
            let instance = GetModuleHandleW(ptr::null());
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                title_wide.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                x,
                y,
                400,
                300,
                0 as _,
                0 as _,
                instance,
                ptr::null(),
            )
        };
        if hwnd.is_null() {
            eprintln!("Failed to create window {}", i);
            std::process::exit(1);
        }
        hwnds.push(hwnd);
    }

    // Pump initial messages so windows fully appear
    for _ in 0..30 {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, 0 as _, 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }

    // Background thread: wait for stdin close (signals exit)
    let should_exit = Arc::new(AtomicBool::new(false));
    let exit_flag = should_exit.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 1];
        // This blocks until stdin is closed
        let _ = std::io::stdin().read(&mut buf);
        exit_flag.store(true, Ordering::SeqCst);
        // Post WM_QUIT to break the message loop
        unsafe {
            PostQuitMessage(0);
        }
    });

    // Message loop
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while !should_exit.load(Ordering::SeqCst) {
            let ret = GetMessageW(&mut msg, 0 as _, 0, 0);
            if ret <= 0 {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup
        for hwnd in hwnds {
            DestroyWindow(hwnd);
        }
    }
}
