//! Acceptance test: end-to-end lifecycle with real Win32 windows.
//!
//! Creates real windows, groups them, verifies overlays, switches tabs,
//! and ungroups — exercising the full happy path.

use std::ptr;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::overlay;
use crate::state;
use crate::window;
use crate::window::WindowInfo;

/// Pump the Win32 message queue for the given duration.
fn pump_messages(duration: Duration) {
    let start = std::time::Instant::now();
    while start.elapsed() < duration {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, 0 as _, 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Register a minimal window class for test windows.
fn register_test_class() -> Vec<u16> {
    let class_name: Vec<u16> = "WinTabTestWindow\0".encode_utf16().collect();
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
    class_name
}

/// Create a visible test window with the given title.
fn create_test_window(class_name: &[u16], title: &str) -> HWND {
    let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let instance = GetModuleHandleW(ptr::null());
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title_wide.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            400,
            300,
            0 as _,
            0 as _,
            instance,
            ptr::null(),
        )
    }
}

/// Build a WindowInfo manually (bypasses is_eligible PID check).
fn make_window_info(hwnd: HWND) -> WindowInfo {
    WindowInfo {
        hwnd,
        title: window::get_window_title(hwnd),
        icon: window::get_window_icon(hwnd),
        rect: window::get_window_rect(hwnd),
    }
}

#[test]
fn acceptance_group_lifecycle() {
    // 1. Register window classes
    overlay::register_class();
    let test_class = register_test_class();

    // 2. Create two test windows
    let win1 = create_test_window(&test_class, "Test Window A");
    let win2 = create_test_window(&test_class, "Test Window B");
    assert!(!win1.is_null(), "Failed to create test window 1");
    assert!(!win2.is_null(), "Failed to create test window 2");

    // 3. Insert WindowInfo into state (bypass is_eligible)
    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
    });

    // 4. Pump messages to let windows appear
    pump_messages(Duration::from_millis(200));

    // 5. Assert both windows are tracked
    state::with_state(|s| {
        assert!(s.windows.contains_key(&win1), "win1 not in state");
        assert!(s.windows.contains_key(&win2), "win2 not in state");
    });

    // 6. Create a group (win1 + win2), create and update overlay
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    // 7. Pump messages for overlay to render
    pump_messages(Duration::from_millis(200));

    // 8. Assert overlay exists and is visible
    let overlay_class: Vec<u16> = "WinTabOverlay\0".encode_utf16().collect();
    unsafe {
        let ov_hwnd = FindWindowExW(0 as _, 0 as _, overlay_class.as_ptr(), ptr::null());
        assert!(!ov_hwnd.is_null(), "Overlay window not found");
        assert_ne!(IsWindowVisible(ov_hwnd), 0, "Overlay not visible");

        // Overlay should be positioned above the active window
        let mut ov_rect: RECT = std::mem::zeroed();
        GetWindowRect(ov_hwnd, &mut ov_rect);
        let win_rect = window::get_window_rect(win2); // win2 is active after create_group
        assert!(
            ov_rect.bottom <= win_rect.top + 5,
            "Overlay bottom ({}) should be at or above active window top ({})",
            ov_rect.bottom,
            win_rect.top,
        );
    }

    // 9. Assert group has 2 tabs with win2 active
    state::with_state(|s| {
        let group = s.groups.groups.get(&group_id).expect("Group not found");
        assert_eq!(group.tabs.len(), 2, "Group should have 2 tabs");
        assert_eq!(group.active, 1, "win2 should be the active tab (index 1)");
    });

    // 10. Switch to tab 0 (win1)
    state::with_state(|s| {
        let group = s.groups.groups.get_mut(&group_id).expect("Group not found");
        group.switch_to(0);
    });

    pump_messages(Duration::from_millis(100));

    // 11. Assert win1 visible, win2 hidden
    unsafe {
        assert_ne!(
            IsWindowVisible(win1),
            0,
            "win1 should be visible after switch"
        );
        assert_eq!(
            IsWindowVisible(win2),
            0,
            "win2 should be hidden after switch"
        );
    }

    // 12. Ungroup: remove win1 from the group (dissolves the 2-tab group)
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
    });

    pump_messages(Duration::from_millis(100));

    // 13. Assert both windows visible (ungrouped)
    unsafe {
        assert_ne!(
            IsWindowVisible(win1),
            0,
            "win1 should be visible after ungroup"
        );
        assert_ne!(
            IsWindowVisible(win2),
            0,
            "win2 should be visible after ungroup"
        );
    }

    // 14. Assert no group references remain
    state::with_state(|s| {
        assert!(s.groups.group_of(win1).is_none(), "win1 still in a group");
        assert!(s.groups.group_of(win2).is_none(), "win2 still in a group");
        assert!(
            !s.groups.groups.contains_key(&group_id),
            "Group still exists"
        );
    });

    // 15. Assert overlay destroyed
    unsafe {
        let ov_hwnd = FindWindowExW(0 as _, 0 as _, overlay_class.as_ptr(), ptr::null());
        assert!(
            ov_hwnd.is_null(),
            "Overlay should be destroyed after ungroup"
        );
    }

    // 16. Cleanup
    state::with_state(|s| {
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
    }
}
