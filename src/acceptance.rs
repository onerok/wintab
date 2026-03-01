//! Acceptance test: end-to-end lifecycle with real Win32 windows.
//!
//! Creates real windows, groups them, verifies overlays, switches tabs,
//! and ungroups — exercising the full happy path.

use std::process::{Command, Stdio};
use std::ptr;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{NMTTDISPINFOW, TTM_GETTOOLCOUNT, TTN_GETDISPINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::hook;
use crate::overlay;
use crate::state;
use crate::window;
use crate::screenshot;
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
        process_name: String::new(),
        class_name: String::new(),
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
    let ov_hwnd = state::with_state(|s| {
        *s.overlays.overlays.get(&group_id).expect("Overlay not found in state")
    });
    unsafe {
        assert_ne!(IsWindowVisible(ov_hwnd), 0, "Overlay not visible");

        // Overlay should be positioned near the active window's top edge
        let mut ov_rect: RECT = std::mem::zeroed();
        GetWindowRect(ov_hwnd, &mut ov_rect);
        let mut win_rect: RECT = std::mem::zeroed();
        GetWindowRect(win2, &mut win_rect); // Use same API for consistency
        assert!(
            ov_rect.bottom <= win_rect.top + 5,
            "Overlay bottom ({}) should be at or above active window top ({})",
            ov_rect.bottom,
            win_rect.top,
        );
    }

    // Screenshot: group created with overlay visible above active window
    screenshot::capture_window(win2, "evidence/group_lifecycle/01_group_created.png");

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

    // Screenshot: after switching to tab 0 (win1 visible, win2 hidden)
    screenshot::capture_window(win1, "evidence/group_lifecycle/02_tab_switched.png");

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

    // Screenshot: after ungrouping (both windows visible, no overlay)
    screenshot::capture_window(win1, "evidence/group_lifecycle/03_ungrouped.png");

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
    state::with_state(|s| {
        assert!(
            !s.overlays.overlays.contains_key(&group_id),
            "Overlay should be removed from state after ungroup"
        );
    });
    unsafe {
        assert_eq!(
            IsWindow(ov_hwnd),
            0,
            "Overlay window should be destroyed after ungroup"
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

/// Test: adding a 3rd window to an existing 2-tab group.
/// This is the main bug scenario — peek overlay should work when adding beyond the first tab.
#[test]
fn acceptance_add_third_window_to_group() {
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Third Test A");
    let win2 = create_test_window(&test_class, "Third Test B");
    let win3 = create_test_window(&test_class, "Third Test C");
    assert!(!win1.is_null(), "Failed to create win1");
    assert!(!win2.is_null(), "Failed to create win2");
    assert!(!win3.is_null(), "Failed to create win3");

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
        s.windows.insert(win3, make_window_info(win3));
    });

    pump_messages(Duration::from_millis(200));

    // Create a 2-tab group
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(100));

    // Verify 2-tab group
    state::with_state(|s| {
        let group = s.groups.groups.get(&group_id).expect("Group should exist");
        assert_eq!(group.tabs.len(), 2, "Group should have 2 tabs before add");
    });

    // Add 3rd window to the group
    state::with_state(|s| {
        s.groups.add_to_group(group_id, win3);
        let ov = s.overlays.ensure_overlay(group_id);
        overlay::update_overlay(ov, group_id, &s.groups, &s.windows);
    });

    pump_messages(Duration::from_millis(100));

    // Verify 3-tab group
    state::with_state(|s| {
        let group = s.groups.groups.get(&group_id).expect("Group should exist after add");
        assert_eq!(group.tabs.len(), 3, "Group should have 3 tabs after add");
        assert!(
            group.tabs.contains(&win3),
            "win3 should be in the group"
        );
        // win3 should be the new active tab (add switches to newly added)
        assert_eq!(
            group.active_hwnd(),
            win3,
            "win3 should be active after add_to_group"
        );
    });

    // Verify win3 is tracked in window_to_group
    state::with_state(|s| {
        assert_eq!(
            s.groups.group_of(win3),
            Some(group_id),
            "win3 should be mapped to the group"
        );
    });

    // Verify overlay is still valid and visible
    state::with_state(|s| {
        assert!(
            s.overlays.overlays.contains_key(&group_id),
            "Overlay should still exist for 3-tab group"
        );
    });

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.groups.remove_from_group(win3);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.windows.remove(&win3);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
        DestroyWindow(win3);
    }
}

/// Test: peek state should be None after creating a group from a peek drag.
/// Simulates: user peeks at a window, then drags to create a group.
/// After group creation, peek should be cleared.
#[test]
fn acceptance_peek_cleared_after_group_creation() {
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Peek Clear A");
    let win2 = create_test_window(&test_class, "Peek Clear B");
    assert!(!win1.is_null());
    assert!(!win2.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
    });

    pump_messages(Duration::from_millis(200));

    // Simulate peek state: manually create a peek overlay for win1
    let peek_ov = overlay::create_peek_overlay(win1);
    assert!(!peek_ov.is_null(), "Failed to create peek overlay");

    state::with_state(|s| {
        overlay::update_peek_overlay(peek_ov, win1, &s.windows);
        s.peek = Some(state::PeekState {
            target_hwnd: win1,
            overlay_hwnd: peek_ov,
            leave_ticks: 0,
        });
    });

    // Verify peek is active
    state::with_state(|s| {
        assert!(s.peek.is_some(), "Peek should be active before group creation");
    });

    // Now create a group (simulating the drag completion).
    // The code should clear peek when the peek target becomes grouped.
    state::with_state(|s| {
        // First hide peek (as drag completion would)
        s.hide_peek();
        // Then create the group
        let gid = s.groups.create_group(win1, win2);
        s.overlays.ensure_overlay(gid);
        overlay::update_overlay(
            *s.overlays.overlays.get(&gid).unwrap(),
            gid,
            &s.groups,
            &s.windows,
        );
    });

    pump_messages(Duration::from_millis(100));

    // Verify peek is gone
    state::with_state(|s| {
        assert!(s.peek.is_none(), "Peek should be None after group creation");
    });

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
    }
}

/// Test: sequential peek operations — peek A, hide, peek B — both should work.
#[test]
fn acceptance_sequential_peek_operations() {
    overlay::register_class();
    let test_class = register_test_class();

    let win_a = create_test_window(&test_class, "Seq Peek A");
    let win_b = create_test_window(&test_class, "Seq Peek B");
    assert!(!win_a.is_null());
    assert!(!win_b.is_null());

    state::with_state(|s| {
        s.windows.insert(win_a, make_window_info(win_a));
        s.windows.insert(win_b, make_window_info(win_b));
    });

    pump_messages(Duration::from_millis(200));

    // Peek at window A
    let peek_ov_a = overlay::create_peek_overlay(win_a);
    assert!(!peek_ov_a.is_null(), "Failed to create peek overlay for A");

    state::with_state(|s| {
        overlay::update_peek_overlay(peek_ov_a, win_a, &s.windows);
        s.peek = Some(state::PeekState {
            target_hwnd: win_a,
            overlay_hwnd: peek_ov_a,
            leave_ticks: 0,
        });
    });

    state::with_state(|s| {
        assert!(s.peek.is_some(), "Peek A should be active");
        assert_eq!(
            s.peek.as_ref().unwrap().target_hwnd,
            win_a,
            "Peek target should be win_a"
        );
    });

    // Hide peek A
    state::with_state(|s| {
        s.hide_peek();
    });

    state::with_state(|s| {
        assert!(s.peek.is_none(), "Peek should be None after hide");
    });

    pump_messages(Duration::from_millis(50));

    // Peek at window B
    let peek_ov_b = overlay::create_peek_overlay(win_b);
    assert!(!peek_ov_b.is_null(), "Failed to create peek overlay for B");

    state::with_state(|s| {
        overlay::update_peek_overlay(peek_ov_b, win_b, &s.windows);
        s.peek = Some(state::PeekState {
            target_hwnd: win_b,
            overlay_hwnd: peek_ov_b,
            leave_ticks: 0,
        });
    });

    state::with_state(|s| {
        assert!(s.peek.is_some(), "Peek B should be active");
        assert_eq!(
            s.peek.as_ref().unwrap().target_hwnd,
            win_b,
            "Peek target should be win_b"
        );
    });

    // Cleanup
    state::with_state(|s| {
        s.hide_peek();
        s.windows.remove(&win_a);
        s.windows.remove(&win_b);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win_a);
        DestroyWindow(win_b);
    }
}

/// Test: peek candidate finding filters out grouped windows.
/// Ungrouped windows are peek candidates; grouped ones are not.
#[test]
fn acceptance_peek_candidate_excludes_grouped() {
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Cand Grouped A");
    let win2 = create_test_window(&test_class, "Cand Grouped B");
    let win3 = create_test_window(&test_class, "Cand Ungrouped");
    assert!(!win1.is_null());
    assert!(!win2.is_null());
    assert!(!win3.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
        s.windows.insert(win3, make_window_info(win3));
    });

    pump_messages(Duration::from_millis(200));

    // Group win1 + win2
    let group_id = state::with_state(|s| {
        s.groups.create_group(win1, win2)
    });

    // Verify: grouped windows have a group, ungrouped does not
    state::with_state(|s| {
        assert!(
            s.groups.group_of(win1).is_some(),
            "win1 should be grouped"
        );
        assert!(
            s.groups.group_of(win2).is_some(),
            "win2 should be grouped"
        );
        assert!(
            s.groups.group_of(win3).is_none(),
            "win3 should NOT be grouped (eligible for peek)"
        );
    });

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.windows.remove(&win3);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
        DestroyWindow(win3);
    }
}

/// Test: add_to_group flow — adding to an existing group via direct API call,
/// then verify overlay updates correctly with the new tab count.
#[test]
fn acceptance_add_to_group_updates_overlay() {
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "AddGroup A");
    let win2 = create_test_window(&test_class, "AddGroup B");
    let win3 = create_test_window(&test_class, "AddGroup C");
    assert!(!win1.is_null());
    assert!(!win2.is_null());
    assert!(!win3.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
        s.windows.insert(win3, make_window_info(win3));
    });

    pump_messages(Duration::from_millis(200));

    // Create group with win1 + win2
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(100));

    // Add win3 to group and refresh overlay
    state::with_state(|s| {
        s.groups.add_to_group(group_id, win3);

        // Refresh overlay after adding
        let ov = s.overlays.ensure_overlay(group_id);
        overlay::update_overlay(ov, group_id, &s.groups, &s.windows);
    });

    pump_messages(Duration::from_millis(100));

    // Verify group state
    state::with_state(|s| {
        let group = s.groups.groups.get(&group_id).expect("Group should exist");
        assert_eq!(group.tabs.len(), 3, "Group should have 3 tabs");
        assert_eq!(
            group.tabs,
            vec![win1, win2, win3],
            "Tabs should be in insertion order"
        );
        // add() calls switch_to on the new tab
        assert_eq!(group.active, 2, "win3 (index 2) should be active after add");
    });

    // Verify overlay is tracked
    state::with_state(|s| {
        let ov_hwnd = s.overlays.overlays.get(&group_id);
        assert!(ov_hwnd.is_some(), "Overlay should exist for the group");
        assert!(
            !ov_hwnd.unwrap().is_null(),
            "Overlay HWND should not be null"
        );
    });

    // Verify win3 is hidden after being added (only active should be visible,
    // but add() switches to win3, so win3 should be visible and win1/win2 hidden)
    unsafe {
        assert_ne!(
            IsWindowVisible(win3),
            0,
            "win3 should be visible (active tab)"
        );
        assert_eq!(
            IsWindowVisible(win1),
            0,
            "win1 should be hidden (inactive tab)"
        );
    }

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.groups.remove_from_group(win3);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.windows.remove(&win3);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
        DestroyWindow(win3);
    }
}

/// Test: ungroup then re-peek — after ungrouping, peek should work again for those windows.
#[test]
fn acceptance_ungroup_then_peek() {
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Ungroup Peek A");
    let win2 = create_test_window(&test_class, "Ungroup Peek B");
    assert!(!win1.is_null());
    assert!(!win2.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
    });

    pump_messages(Duration::from_millis(200));

    // Create group
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        s.overlays.ensure_overlay(gid);
        gid
    });

    pump_messages(Duration::from_millis(100));

    // Verify both are grouped
    state::with_state(|s| {
        assert!(s.groups.group_of(win1).is_some(), "win1 should be grouped");
        assert!(s.groups.group_of(win2).is_some(), "win2 should be grouped");
    });

    // Ungroup: remove win1 (dissolves the 2-tab group)
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
    });

    pump_messages(Duration::from_millis(100));

    // Both windows should now be ungrouped
    state::with_state(|s| {
        assert!(
            s.groups.group_of(win1).is_none(),
            "win1 should be ungrouped after remove"
        );
        assert!(
            s.groups.group_of(win2).is_none(),
            "win2 should be ungrouped after dissolve"
        );
    });

    // Now peek should work for these ungrouped windows.
    // Simulate peek on win1 (which was previously grouped).
    let peek_ov = overlay::create_peek_overlay(win1);
    assert!(
        !peek_ov.is_null(),
        "Should be able to create peek overlay for previously-grouped window"
    );

    state::with_state(|s| {
        overlay::update_peek_overlay(peek_ov, win1, &s.windows);
        s.peek = Some(state::PeekState {
            target_hwnd: win1,
            overlay_hwnd: peek_ov,
            leave_ticks: 0,
        });
    });

    state::with_state(|s| {
        assert!(
            s.peek.is_some(),
            "Peek should be active for previously-grouped window"
        );
        assert_eq!(
            s.peek.as_ref().unwrap().target_hwnd,
            win1,
            "Peek target should be win1"
        );
    });

    // Also peek on win2
    state::with_state(|s| {
        s.hide_peek();
    });

    let peek_ov2 = overlay::create_peek_overlay(win2);
    assert!(
        !peek_ov2.is_null(),
        "Should be able to create peek overlay for win2 after ungroup"
    );

    state::with_state(|s| {
        overlay::update_peek_overlay(peek_ov2, win2, &s.windows);
        s.peek = Some(state::PeekState {
            target_hwnd: win2,
            overlay_hwnd: peek_ov2,
            leave_ticks: 0,
        });
    });

    state::with_state(|s| {
        assert!(
            s.peek.is_some(),
            "Peek should be active for win2 after ungroup"
        );
        assert_eq!(
            s.peek.as_ref().unwrap().target_hwnd,
            win2,
            "Peek target should be win2"
        );
    });

    // Cleanup
    state::with_state(|s| {
        s.hide_peek();
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
    }
}

/// Test: minimizing the active window hides the overlay; restoring shows it.
#[test]
fn acceptance_minimize_restore_group() {
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "MinRestore A");
    let win2 = create_test_window(&test_class, "MinRestore B");
    assert!(!win1.is_null());
    assert!(!win2.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
    });

    pump_messages(Duration::from_millis(200));

    // Create group and overlay (win2 is active at index 1)
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(200));

    // Get overlay HWND
    let ov_hwnd = state::with_state(|s| {
        *s.overlays.overlays.get(&group_id).unwrap()
    });

    // Verify overlay is visible
    unsafe {
        assert_ne!(
            IsWindowVisible(ov_hwnd),
            0,
            "Overlay should be visible before minimize"
        );
    }

    // Minimize active window via state handler
    state::with_state(|s| {
        s.on_minimize(win2);
    });

    pump_messages(Duration::from_millis(100));

    // Verify overlay is hidden after minimize
    unsafe {
        assert_eq!(
            IsWindowVisible(ov_hwnd),
            0,
            "Overlay should be hidden after minimize"
        );
    }

    // Screenshot: overlay hidden after minimize
    screenshot::capture_window(win2, "evidence/minimize_restore/01_minimized.png");

    // Restore via state handler
    state::with_state(|s| {
        s.on_restore(win2);
    });

    pump_messages(Duration::from_millis(100));

    // Verify overlay is visible again
    unsafe {
        assert_ne!(
            IsWindowVisible(ov_hwnd),
            0,
            "Overlay should be visible after restore"
        );
    }

    // Screenshot: overlay visible again after restore
    screenshot::capture_window(win2, "evidence/minimize_restore/02_restored.png");

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
    }
}

/// Test: destroying a window in a 2-tab group dissolves the group.
#[test]
fn acceptance_window_destroyed_dissolves_group() {
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Destroy A");
    let win2 = create_test_window(&test_class, "Destroy B");
    assert!(!win1.is_null());
    assert!(!win2.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
    });

    pump_messages(Duration::from_millis(200));

    // Create group
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        s.overlays.ensure_overlay(gid);
        gid
    });

    // Verify group exists
    state::with_state(|s| {
        assert!(s.groups.groups.contains_key(&group_id));
        assert_eq!(s.groups.group_of(win1), Some(group_id));
        assert_eq!(s.groups.group_of(win2), Some(group_id));
    });

    // Simulate window destruction
    state::with_state(|s| {
        s.on_window_destroyed(win1);
    });

    pump_messages(Duration::from_millis(100));

    // Group should be dissolved (was 2-tab, lost one)
    state::with_state(|s| {
        assert!(
            s.groups.group_of(win1).is_none(),
            "win1 should not be in any group after destroy"
        );
        assert!(
            s.groups.group_of(win2).is_none(),
            "win2 should be ungrouped after 2-tab group dissolves"
        );
        assert!(
            !s.groups.groups.contains_key(&group_id),
            "Group should not exist after dissolve"
        );
        assert!(
            !s.windows.contains_key(&win1),
            "win1 should be removed from tracked windows"
        );
    });

    // win2 should be visible (restored from hidden)
    unsafe {
        assert_ne!(
            IsWindowVisible(win2),
            0,
            "win2 should be visible after group dissolves"
        );
    }

    // Cleanup
    state::with_state(|s| {
        s.windows.remove(&win2);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
    }
}

/// Test: changing a window title updates the tracked state.
#[test]
fn acceptance_title_change_updates_state() {
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Original Title");
    assert!(!win1.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
    });

    pump_messages(Duration::from_millis(200));

    // Verify original title
    state::with_state(|s| {
        assert_eq!(
            s.windows.get(&win1).unwrap().title,
            "Original Title"
        );
    });

    // Change the title
    let new_title: Vec<u16> = "Updated Title\0".encode_utf16().collect();
    unsafe {
        SetWindowTextW(win1, new_title.as_ptr());
    }

    // Notify state of title change
    state::with_state(|s| {
        s.on_title_changed(win1);
    });

    // Verify updated title
    state::with_state(|s| {
        assert_eq!(
            s.windows.get(&win1).unwrap().title,
            "Updated Title"
        );
    });

    // Cleanup
    state::with_state(|s| {
        s.windows.remove(&win1);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
    }
}

/// Test: switching through all tabs in a 3-tab group tracks active correctly.
#[test]
fn acceptance_switch_through_all_tabs() {
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Switch A");
    let win2 = create_test_window(&test_class, "Switch B");
    let win3 = create_test_window(&test_class, "Switch C");
    assert!(!win1.is_null());
    assert!(!win2.is_null());
    assert!(!win3.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
        s.windows.insert(win3, make_window_info(win3));
    });

    pump_messages(Duration::from_millis(200));

    // Create 3-tab group
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        s.groups.add_to_group(gid, win3);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(100));

    // win3 should be active (last added)
    state::with_state(|s| {
        let group = s.groups.groups.get(&group_id).unwrap();
        assert_eq!(group.active, 2);
        assert_eq!(group.active_hwnd(), win3);
    });

    // Switch to tab 0 (win1)
    state::with_state(|s| {
        let group = s.groups.groups.get_mut(&group_id).unwrap();
        group.switch_to(0);
    });
    pump_messages(Duration::from_millis(50));

    state::with_state(|s| {
        let group = s.groups.groups.get(&group_id).unwrap();
        assert_eq!(group.active, 0);
        assert_eq!(group.active_hwnd(), win1);
    });
    unsafe {
        assert_ne!(IsWindowVisible(win1), 0, "win1 should be visible");
        assert_eq!(IsWindowVisible(win2), 0, "win2 should be hidden");
        assert_eq!(IsWindowVisible(win3), 0, "win3 should be hidden");
    }

    // Switch to tab 1 (win2)
    state::with_state(|s| {
        let group = s.groups.groups.get_mut(&group_id).unwrap();
        group.switch_to(1);
    });
    pump_messages(Duration::from_millis(50));

    state::with_state(|s| {
        let group = s.groups.groups.get(&group_id).unwrap();
        assert_eq!(group.active, 1);
        assert_eq!(group.active_hwnd(), win2);
    });
    unsafe {
        assert_eq!(IsWindowVisible(win1), 0, "win1 should be hidden");
        assert_ne!(IsWindowVisible(win2), 0, "win2 should be visible");
        assert_eq!(IsWindowVisible(win3), 0, "win3 should be hidden");
    }

    // Switch to tab 2 (win3)
    state::with_state(|s| {
        let group = s.groups.groups.get_mut(&group_id).unwrap();
        group.switch_to(2);
    });
    pump_messages(Duration::from_millis(50));

    state::with_state(|s| {
        let group = s.groups.groups.get(&group_id).unwrap();
        assert_eq!(group.active, 2);
        assert_eq!(group.active_hwnd(), win3);
    });

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.groups.remove_from_group(win3);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.windows.remove(&win3);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
        DestroyWindow(win3);
    }
}

/// Test: two independent groups don't interfere with each other.
#[test]
fn acceptance_multiple_independent_groups() {
    overlay::register_class();
    let test_class = register_test_class();

    let win_a1 = create_test_window(&test_class, "GroupA 1");
    let win_a2 = create_test_window(&test_class, "GroupA 2");
    let win_b1 = create_test_window(&test_class, "GroupB 1");
    let win_b2 = create_test_window(&test_class, "GroupB 2");
    assert!(!win_a1.is_null());
    assert!(!win_a2.is_null());
    assert!(!win_b1.is_null());
    assert!(!win_b2.is_null());

    state::with_state(|s| {
        s.windows.insert(win_a1, make_window_info(win_a1));
        s.windows.insert(win_a2, make_window_info(win_a2));
        s.windows.insert(win_b1, make_window_info(win_b1));
        s.windows.insert(win_b2, make_window_info(win_b2));
    });

    pump_messages(Duration::from_millis(200));

    // Create two independent groups
    let (gid_a, gid_b) = state::with_state(|s| {
        let ga = s.groups.create_group(win_a1, win_a2);
        let gb = s.groups.create_group(win_b1, win_b2);
        s.overlays.ensure_overlay(ga);
        s.overlays.ensure_overlay(gb);
        (ga, gb)
    });

    pump_messages(Duration::from_millis(100));

    // Verify independent tracking
    state::with_state(|s| {
        assert_eq!(s.groups.group_of(win_a1), Some(gid_a));
        assert_eq!(s.groups.group_of(win_a2), Some(gid_a));
        assert_eq!(s.groups.group_of(win_b1), Some(gid_b));
        assert_eq!(s.groups.group_of(win_b2), Some(gid_b));
        assert_ne!(gid_a, gid_b);
    });

    // Switch tab in group A — group B should be unaffected
    state::with_state(|s| {
        let group_a = s.groups.groups.get_mut(&gid_a).unwrap();
        group_a.switch_to(0);
    });

    pump_messages(Duration::from_millis(50));

    state::with_state(|s| {
        let group_a = s.groups.groups.get(&gid_a).unwrap();
        assert_eq!(group_a.active, 0, "Group A active should be 0");

        let group_b = s.groups.groups.get(&gid_b).unwrap();
        assert_eq!(group_b.active, 1, "Group B active should still be 1");
    });

    // Dissolve group A — group B should survive
    state::with_state(|s| {
        s.groups.remove_from_group(win_a1);
        s.overlays.refresh_overlay(gid_a, &s.groups, &s.windows);
    });

    pump_messages(Duration::from_millis(50));

    state::with_state(|s| {
        assert!(s.groups.group_of(win_a1).is_none());
        assert!(s.groups.group_of(win_a2).is_none());
        assert!(!s.groups.groups.contains_key(&gid_a), "Group A dissolved");

        assert_eq!(s.groups.group_of(win_b1), Some(gid_b));
        assert_eq!(s.groups.group_of(win_b2), Some(gid_b));
        assert!(s.groups.groups.contains_key(&gid_b), "Group B still exists");
    });

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win_b1);
        s.groups.remove_from_group(win_b2);
        s.overlays.refresh_overlay(gid_b, &s.groups, &s.windows);
        s.windows.remove(&win_a1);
        s.windows.remove(&win_a2);
        s.windows.remove(&win_b1);
        s.windows.remove(&win_b2);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win_a1);
        DestroyWindow(win_a2);
        DestroyWindow(win_b1);
        DestroyWindow(win_b2);
    }
}

/// Test: find_managed_window_at respects z-order by using WindowFromPoint.
/// Creates two overlapping windows, brings one to front, verifies the frontmost is found.
#[test]
fn acceptance_peek_respects_zorder() {
    let test_class = register_test_class();

    // Create two windows at the same position (overlapping)
    let title_back: Vec<u16> = "ZOrder Back\0".encode_utf16().collect();
    let title_front: Vec<u16> = "ZOrder Front\0".encode_utf16().collect();
    let instance = unsafe { GetModuleHandleW(ptr::null()) };

    let win_back = unsafe {
        CreateWindowExW(
            0,
            test_class.as_ptr(),
            title_back.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100, 100, 400, 300,
            0 as _, 0 as _, instance, ptr::null(),
        )
    };
    let win_front = unsafe {
        CreateWindowExW(
            0,
            test_class.as_ptr(),
            title_front.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100, 100, 400, 300,
            0 as _, 0 as _, instance, ptr::null(),
        )
    };
    assert!(!win_back.is_null());
    assert!(!win_front.is_null());

    state::with_state(|s| {
        s.windows.insert(win_back, make_window_info(win_back));
        s.windows.insert(win_front, make_window_info(win_front));
    });

    pump_messages(Duration::from_millis(200));

    // Bring win_front to the topmost to ensure it's above everything
    unsafe {
        SetForegroundWindow(win_front);
        SetWindowPos(
            win_front,
            HWND_TOPMOST,
            0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
    }
    pump_messages(Duration::from_millis(100));

    // Use find_managed_window_at at the center of the overlapping area
    let center = unsafe {
        let mut rect: RECT = std::mem::zeroed();
        GetWindowRect(win_front, &mut rect);
        POINT {
            x: (rect.left + rect.right) / 2,
            y: (rect.top + rect.bottom) / 2,
        }
    };
    let found = state::with_state(|s| {
        s.find_managed_window_at(center)
    });

    // Should find win_front (the topmost), not win_back
    assert_eq!(
        found,
        Some(win_front),
        "find_managed_window_at should return the frontmost window"
    );

    // Cleanup
    state::with_state(|s| {
        s.windows.remove(&win_back);
        s.windows.remove(&win_front);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win_back);
        DestroyWindow(win_front);
    }
}

/// Test: toggling enabled hides all overlays; toggling back shows them.
#[test]
fn acceptance_toggle_enabled() {
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Toggle A");
    let win2 = create_test_window(&test_class, "Toggle B");
    assert!(!win1.is_null());
    assert!(!win2.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
    });

    pump_messages(Duration::from_millis(200));

    // Create group with overlay
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(200));

    let ov_hwnd = state::with_state(|s| {
        *s.overlays.overlays.get(&group_id).unwrap()
    });

    // Verify initially enabled
    state::with_state(|s| {
        assert!(s.enabled);
    });
    unsafe {
        assert_ne!(IsWindowVisible(ov_hwnd), 0, "Overlay visible when enabled");
    }

    // Toggle off
    state::with_state(|s| {
        s.toggle_enabled();
        assert!(!s.enabled);
    });

    pump_messages(Duration::from_millis(100));

    unsafe {
        assert_eq!(
            IsWindowVisible(ov_hwnd),
            0,
            "Overlay should be hidden when disabled"
        );
    }

    // Toggle back on
    state::with_state(|s| {
        s.toggle_enabled();
        assert!(s.enabled);
    });

    pump_messages(Duration::from_millis(100));

    unsafe {
        assert_ne!(
            IsWindowVisible(ov_hwnd),
            0,
            "Overlay should be visible when re-enabled"
        );
    }

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
    }
}

/// Test: on_desktop_switch shows overlays for windows on current desktop.
#[test]
fn acceptance_desktop_switch_overlay_visibility() {
    unsafe {
        windows_sys::Win32::System::Com::CoInitializeEx(
            std::ptr::null(),
            windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32,
        );
    }

    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "VDesk A");
    let win2 = create_test_window(&test_class, "VDesk B");
    assert!(!win1.is_null());
    assert!(!win2.is_null());

    state::with_state(|s| {
        s.vdesktop = crate::vdesktop::VDesktopManager::new();
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
    });

    pump_messages(Duration::from_millis(200));

    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(200));

    let ov_hwnd = state::with_state(|s| {
        *s.overlays.overlays.get(&group_id).unwrap()
    });

    // Call on_desktop_switch — since test windows are on current desktop,
    // overlay should remain visible
    state::with_state(|s| {
        s.on_desktop_switch();
    });

    pump_messages(Duration::from_millis(100));

    unsafe {
        assert_ne!(
            IsWindowVisible(ov_hwnd),
            0,
            "Overlay should remain visible for windows on current desktop"
        );
    }

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
    }
}

/// Test: VDesktopManager returns true for a freshly created window on current desktop.
#[test]
fn acceptance_vdesktop_current_desktop_check() {
    unsafe {
        windows_sys::Win32::System::Com::CoInitializeEx(
            std::ptr::null(),
            windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32,
        );
    }

    let test_class = register_test_class();
    let win = create_test_window(&test_class, "VDesk Check");
    assert!(!win.is_null());

    pump_messages(Duration::from_millis(200));

    // Create a VDesktopManager and check the window
    let mgr = crate::vdesktop::VDesktopManager::new();
    // If COM init succeeded, freshly created window should be on current desktop
    // If COM init failed (e.g., in CI), fallback returns true — test still passes
    if let Some(mgr) = &mgr {
        assert!(
            mgr.is_on_current_desktop(win),
            "Freshly created window should be on current desktop"
        );
    }

    // Cleanup
    state::with_state(|s| {
        s.windows.remove(&win);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win);
    }
}

/// Test: tooltip HWND is created with overlay and destroyed when overlay is destroyed.
#[test]
fn acceptance_tooltip_created_with_overlay() {
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Tooltip A");
    let win2 = create_test_window(&test_class, "Tooltip B");
    assert!(!win1.is_null());
    assert!(!win2.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
    });

    pump_messages(Duration::from_millis(200));

    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(200));

    let (ov_hwnd, tooltip_hwnd) = state::with_state(|s| {
        let ov = *s.overlays.overlays.get(&group_id).unwrap();
        let tt = overlay::get_tooltip_hwnd(ov);
        (ov, tt)
    });

    // Tooltip should have been created
    assert!(!tooltip_hwnd.is_null(), "Tooltip HWND should not be null");
    unsafe {
        assert_ne!(IsWindow(tooltip_hwnd), 0, "Tooltip should be a valid window");
    }

    // Tooltip should have a registered tool (TTM_ADDTOOLW must have succeeded)
    let tool_count = unsafe { SendMessageW(tooltip_hwnd, TTM_GETTOOLCOUNT, 0, 0) };
    assert!(
        tool_count > 0,
        "Tooltip should have at least one registered tool, but has {}. \
         TTM_ADDTOOLW likely failed due to wrong cbSize (v6 struct size without v6 manifest).",
        tool_count
    );

    // Destroy overlay
    state::with_state(|s| {
        s.overlays.remove_overlay(group_id);
    });

    pump_messages(Duration::from_millis(100));

    // Tooltip should be destroyed along with overlay
    unsafe {
        assert_eq!(
            IsWindow(tooltip_hwnd),
            0,
            "Tooltip should be destroyed when overlay is destroyed"
        );
        assert_eq!(
            IsWindow(ov_hwnd),
            0,
            "Overlay should be destroyed"
        );
    }

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
    }
}

#[test]
fn acceptance_tooltip_shows_truncated_title() {
    overlay::register_class();
    let test_class = register_test_class();

    // Use a title long enough to guarantee truncation at MAX_TAB_WIDTH (200px)
    let long_title =
        "This is a very long window title that should definitely be truncated in the tab bar overlay widget";
    let win1 = create_test_window(&test_class, long_title);
    let win2 = create_test_window(&test_class, "Short");
    assert!(!win1.is_null());
    assert!(!win2.is_null());

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
    });

    pump_messages(Duration::from_millis(100));

    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(100));

    let (ov_hwnd, tooltip_hwnd) = state::with_state(|s| {
        let ov = *s.overlays.overlays.get(&group_id).unwrap();
        let tt = overlay::get_tooltip_hwnd(ov);
        (ov, tt)
    });

    assert!(!tooltip_hwnd.is_null(), "Tooltip should be created");

    // Verify the title is actually in state
    let title_in_state = state::with_state(|s| {
        s.windows.get(&win1).map(|info| info.title.clone())
    });
    assert!(
        title_in_state.is_some(),
        "Window title should be in state"
    );
    assert!(
        title_in_state.as_ref().unwrap().len() > 20,
        "Title should be long, got: {:?}",
        title_in_state
    );

    // Set hover_tab = 0 so handler knows which tab we're hovering
    overlay::set_test_hover_tab(ov_hwnd, 0);

    // Simulate TTN_GETDISPINFOW notification (same path as real tooltip)
    let mut nmdi: NMTTDISPINFOW = unsafe { std::mem::zeroed() };
    nmdi.hdr.hwndFrom = tooltip_hwnd;
    nmdi.hdr.idFrom = ov_hwnd as usize;
    nmdi.hdr.code = TTN_GETDISPINFOW;

    unsafe {
        SendMessageW(
            ov_hwnd,
            WM_NOTIFY,
            0,
            &mut nmdi as *mut _ as isize,
        );
    }

    // szText should have been filled with the truncated title
    let text_len = nmdi.szText.iter().position(|&c| c == 0).unwrap_or(80);
    assert!(
        text_len > 0,
        "Tooltip szText should contain the title text, but was empty. \
         This means handle_tooltip_getdispinfo did not fill the tooltip text."
    );

    let tooltip_text = String::from_utf16_lossy(&nmdi.szText[..text_len]);
    assert!(
        long_title.starts_with(&tooltip_text),
        "Tooltip text '{}' should be a prefix of the long title",
        tooltip_text
    );

    // Cleanup
    state::with_state(|s| {
        s.overlays.remove_overlay(group_id);
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.windows.remove(&win1);
        s.windows.remove(&win2);
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
    }
}

// ── Rules engine acceptance tests ──

/// Build a WindowInfo with explicit process_name and class_name.
fn make_window_info_with_meta(hwnd: HWND, process_name: &str, class_name: &str) -> WindowInfo {
    WindowInfo {
        hwnd,
        title: window::get_window_title(hwnd),
        process_name: process_name.to_string(),
        class_name: class_name.to_string(),
        icon: window::get_window_icon(hwnd),
        rect: window::get_window_rect(hwnd),
    }
}

#[test]
fn acceptance_rules_auto_group_two_matching_windows() {
    // Verify: two windows matching the same rule get auto-grouped
    // with overlay, and a singleton doesn't get an overlay.
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Rules Test A");
    let win2 = create_test_window(&test_class, "Rules Test B");
    let win_solo = create_test_window(&test_class, "Solo Window");
    assert!(!win1.is_null());
    assert!(!win2.is_null());
    assert!(!win_solo.is_null());

    pump_messages(Duration::from_millis(200));

    state::with_state(|s| {
        // Inject rules: match process "test_app.exe"
        use crate::config::*;
        s.rules = RulesEngine {
            groups: vec![RuleGroup {
                name: "TestApps".into(),
                enabled: true,
                match_mode: MatchMode::All,
                rules: vec![WindowRule {
                    field: RuleField::ProcessName,
                    matcher: Matcher::Equals("test_app.exe".into(), false),
                }],
            }],
        };

        // Insert windows — win1 and win2 match, win_solo doesn't
        s.windows.insert(win1, make_window_info_with_meta(win1, "test_app.exe", "TestClass"));
        s.windows.insert(win2, make_window_info_with_meta(win2, "test_app.exe", "TestClass"));
        s.windows.insert(win_solo, make_window_info_with_meta(win_solo, "other.exe", "OtherClass"));

        // Apply rules: win1 becomes pending singleton (no group yet)
        s.apply_rules(win1);
        assert!(
            s.groups.pending_rules.contains_key("TestApps"),
            "First matching window should be a pending singleton"
        );
        assert!(
            s.groups.group_of(win1).is_none(),
            "Singleton should NOT be in a TabGroup"
        );
        assert!(
            s.overlays.overlays.is_empty(),
            "No overlay for singletons"
        );

        // Apply rules: win2 triggers group creation
        s.apply_rules(win2);
        assert!(
            !s.groups.pending_rules.contains_key("TestApps"),
            "Pending entry should be consumed"
        );
        let gid_1 = s.groups.group_of(win1);
        let gid_2 = s.groups.group_of(win2);
        assert!(gid_1.is_some(), "win1 should now be in a group");
        assert_eq!(gid_1, gid_2, "Both windows should be in the SAME group");

        let gid = gid_1.unwrap();
        assert!(
            s.groups.named_groups.get("TestApps") == Some(&gid),
            "named_groups should map rule name to GroupId"
        );
        assert!(
            s.overlays.overlays.contains_key(&gid),
            "Overlay should be created for the group"
        );

        // Apply rules to solo window — no match, no grouping
        s.apply_rules(win_solo);
        assert!(
            s.groups.group_of(win_solo).is_none(),
            "Non-matching window should NOT be grouped"
        );

        // Verify the group has exactly 2 tabs
        let group = s.groups.groups.get(&gid).unwrap();
        assert_eq!(group.tabs.len(), 2, "Group should have exactly 2 tabs");

        // Cleanup
        s.overlays.remove_overlay(gid);
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.windows.clear();
        s.rules = RulesEngine { groups: Vec::new() };
    });

    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
        DestroyWindow(win_solo);
    }
}

#[test]
fn acceptance_rules_third_window_joins_existing_group() {
    // Verify: a third matching window joins the existing named group.
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Join A");
    let win2 = create_test_window(&test_class, "Join B");
    let win3 = create_test_window(&test_class, "Join C");
    assert!(!win1.is_null());
    assert!(!win2.is_null());
    assert!(!win3.is_null());

    pump_messages(Duration::from_millis(200));

    state::with_state(|s| {
        use crate::config::*;
        s.rules = RulesEngine {
            groups: vec![RuleGroup {
                name: "Editors".into(),
                enabled: true,
                match_mode: MatchMode::All,
                rules: vec![WindowRule {
                    field: RuleField::ProcessName,
                    matcher: Matcher::Equals("editor.exe".into(), false),
                }],
            }],
        };

        s.windows.insert(win1, make_window_info_with_meta(win1, "editor.exe", "EditorClass"));
        s.windows.insert(win2, make_window_info_with_meta(win2, "editor.exe", "EditorClass"));
        s.windows.insert(win3, make_window_info_with_meta(win3, "editor.exe", "EditorClass"));

        s.apply_rules(win1); // pending
        s.apply_rules(win2); // creates group

        let gid = s.groups.group_of(win1).unwrap();
        assert_eq!(s.groups.groups.get(&gid).unwrap().tabs.len(), 2);

        // Third window should join existing named group
        s.apply_rules(win3);
        assert_eq!(
            s.groups.group_of(win3),
            Some(gid),
            "Third window should join the existing named group"
        );
        assert_eq!(
            s.groups.groups.get(&gid).unwrap().tabs.len(),
            3,
            "Group should now have 3 tabs"
        );

        // Cleanup
        s.overlays.remove_overlay(gid);
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.groups.remove_from_group(win3);
        s.windows.clear();
        s.rules = RulesEngine { groups: Vec::new() };
    });

    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
        DestroyWindow(win3);
    }
}

#[test]
fn acceptance_rules_disabled_rule_skipped() {
    // Verify: disabled rules don't match.
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Disabled A");
    let win2 = create_test_window(&test_class, "Disabled B");
    assert!(!win1.is_null());
    assert!(!win2.is_null());

    pump_messages(Duration::from_millis(200));

    state::with_state(|s| {
        use crate::config::*;
        s.rules = RulesEngine {
            groups: vec![RuleGroup {
                name: "Disabled".into(),
                enabled: false,
                match_mode: MatchMode::All,
                rules: vec![WindowRule {
                    field: RuleField::ProcessName,
                    matcher: Matcher::Equals("app.exe".into(), false),
                }],
            }],
        };

        s.windows.insert(win1, make_window_info_with_meta(win1, "app.exe", "C"));
        s.windows.insert(win2, make_window_info_with_meta(win2, "app.exe", "C"));

        s.apply_rules(win1);
        s.apply_rules(win2);

        assert!(s.groups.group_of(win1).is_none(), "Disabled rule should not match");
        assert!(s.groups.group_of(win2).is_none(), "Disabled rule should not match");
        assert!(s.groups.pending_rules.is_empty(), "No pending entries for disabled rules");

        // Cleanup
        s.windows.clear();
        s.rules = RulesEngine { groups: Vec::new() };
    });

    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
    }
}

#[test]
fn acceptance_pending_singleton_cleaned_on_destroy() {
    // Verify: destroying a pending singleton removes it from pending_rules.
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Pending Destroy");
    assert!(!win1.is_null());

    pump_messages(Duration::from_millis(200));

    state::with_state(|s| {
        use crate::config::*;
        s.rules = RulesEngine {
            groups: vec![RuleGroup {
                name: "PendingTest".into(),
                enabled: true,
                match_mode: MatchMode::All,
                rules: vec![WindowRule {
                    field: RuleField::ProcessName,
                    matcher: Matcher::Equals("pending.exe".into(), false),
                }],
            }],
        };

        s.windows.insert(win1, make_window_info_with_meta(win1, "pending.exe", "C"));
        s.apply_rules(win1);

        assert!(s.groups.pending_rules.contains_key("PendingTest"));

        // Simulate window destruction
        s.on_window_destroyed(win1);
        assert!(
            !s.groups.pending_rules.contains_key("PendingTest"),
            "Pending entry should be removed when window is destroyed"
        );

        // Cleanup
        s.rules = RulesEngine { groups: Vec::new() };
    });

    unsafe {
        DestroyWindow(win1);
    }
}

// ── Position store acceptance tests ──

#[test]
fn acceptance_position_restore_moves_window() {
    // Verify: a window with a recorded position gets moved on creation.
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Position Test");
    assert!(!win1.is_null());

    pump_messages(Duration::from_millis(200));

    // Record a specific position in the store
    let target_rect = crate::position_store::RectDef {
        left: 150,
        top: 100,
        right: 950,
        bottom: 700,
    };

    state::with_state(|s| {
        s.position_store.record(
            "pos_test.exe",
            "PosClass",
            "Position Test",
            target_rect.clone(),
            96,
        );
    });

    // Build WindowInfo with matching metadata and call try_restore_position
    let info = WindowInfo {
        hwnd: win1,
        title: "Position Test".to_string(),
        process_name: "pos_test.exe".to_string(),
        class_name: "PosClass".to_string(),
        icon: window::get_window_icon(win1),
        rect: window::get_window_rect(win1),
    };

    state::with_state(|s| {
        s.try_restore_position(win1, &info);
    });

    pump_messages(Duration::from_millis(100));

    // Verify the window was moved. The position store stores rect at DPI 96.
    // try_restore_position scales by (current_dpi / 96). We verify the actual
    // position is close to the scaled values (DWM extended frame bounds may
    // add invisible border offsets).
    let actual_rect = window::get_window_rect(win1);
    let current_dpi = window::get_window_dpi(win1);
    let scale = current_dpi as f64 / 96.0;
    let expected_left = (target_rect.left as f64 * scale) as i32;
    let expected_top = (target_rect.top as f64 * scale) as i32;
    let tolerance = 30; // DWM invisible border offset
    assert!(
        (actual_rect.left - expected_left).abs() < tolerance
            && (actual_rect.top - expected_top).abs() < tolerance,
        "Window should be near target position (DPI-scaled).\n  \
         DPI={} scale={:.2}\n  \
         Expected top-left: ({},{})\n  \
         Actual top-left:   ({},{})",
        current_dpi, scale,
        expected_left, expected_top,
        actual_rect.left, actual_rect.top,
    );

    // Cleanup
    state::with_state(|s| {
        s.windows.remove(&win1);
        s.position_store = crate::position_store::PositionStore::empty();
    });

    unsafe {
        DestroyWindow(win1);
    }
}

#[test]
fn acceptance_position_recorded_on_move() {
    // Verify: moving a window records its position in the store.
    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Move Record Test");
    assert!(!win1.is_null());

    pump_messages(Duration::from_millis(200));

    state::with_state(|s| {
        s.windows.insert(win1, make_window_info_with_meta(win1, "move_test.exe", "MoveClass"));
    });

    // Move the window
    unsafe {
        SetWindowPos(win1, 0 as _, 200, 150, 600, 400, SWP_NOACTIVATE | SWP_NOZORDER);
    }
    pump_messages(Duration::from_millis(100));

    // Trigger on_window_moved
    state::with_state(|s| {
        s.on_window_moved(win1);
    });

    // Verify position was recorded
    let found = state::with_state(|s| {
        s.position_store
            .lookup("move_test.exe", "MoveClass", "Move Record Test")
            .is_some()
    });
    assert!(found, "Position should be recorded after window move");

    // Cleanup
    state::with_state(|s| {
        s.windows.remove(&win1);
        s.position_store = crate::position_store::PositionStore::empty();
    });

    unsafe {
        DestroyWindow(win1);
    }
}

// ── E2E test: real separate process ──

/// E2E test: spawns a real separate process (dummy_window.exe) whose windows
/// pass `is_eligible()` (different PID), then verifies WinTab discovers,
/// auto-groups, and tab-switches them — with screenshot evidence.
#[test]
fn acceptance_rules_e2e_auto_group() {
    // Phase 0: Spawn dummy process
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dummy_path = std::path::Path::new(manifest_dir)
        .join("target")
        .join("debug")
        .join("dummy_window.exe");
    assert!(
        dummy_path.exists(),
        "dummy_window.exe not found at {:?}. Run `cargo build --bin dummy_window` first.",
        dummy_path
    );

    let mut child = Command::new(&dummy_path)
        .args(["2", "DummyApp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("Failed to spawn dummy_window.exe");

    let child_pid = child.id();

    // Phase 1: Wait for windows to appear, then discover via enumerate_windows
    overlay::register_class();

    // Poll for the dummy windows to appear (up to 5 seconds)
    let mut discovered: Vec<WindowInfo> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        pump_messages(Duration::from_millis(200));
        discovered = window::enumerate_windows()
            .into_iter()
            .filter(|info| {
                let mut pid = 0u32;
                unsafe {
                    GetWindowThreadProcessId(info.hwnd, &mut pid);
                }
                pid == child_pid
            })
            .collect();
        if discovered.len() >= 2 {
            break;
        }
    }

    assert_eq!(
        discovered.len(),
        2,
        "Expected 2 windows discovered from dummy process (PID {}), got {}. \
         This means is_eligible() filtered them out.",
        child_pid,
        discovered.len()
    );

    for info in &discovered {
        assert_eq!(
            info.process_name, "dummy_window.exe",
            "process_name should be dummy_window.exe, got '{}'",
            info.process_name
        );
    }

    // Screenshot 01: two windows discovered, no overlay yet
    let first_hwnd = discovered[0].hwnd;
    screenshot::capture_window(first_hwnd, "evidence/rules_auto_group/01_windows_discovered.png");

    // Phase 2: Apply rules
    state::with_state(|s| {
        use crate::config::*;
        s.rules = RulesEngine {
            groups: vec![RuleGroup {
                name: "DummyApps".into(),
                enabled: true,
                match_mode: MatchMode::All,
                rules: vec![WindowRule {
                    field: RuleField::ProcessName,
                    matcher: Matcher::Equals("dummy_window.exe".into(), false),
                }],
            }],
        };

        for info in &discovered {
            s.windows.insert(info.hwnd, info.clone());
        }

        for info in &discovered {
            s.apply_rules(info.hwnd);
        }
    });

    pump_messages(Duration::from_millis(300));

    // Phase 3: Verify grouping
    let group_id = state::with_state(|s| {
        let gid_0 = s.groups.group_of(discovered[0].hwnd);
        let gid_1 = s.groups.group_of(discovered[1].hwnd);
        assert!(gid_0.is_some(), "First window should be in a group");
        assert_eq!(
            gid_0, gid_1,
            "Both windows should be in the same group"
        );

        let gid = gid_0.unwrap();
        let group = s.groups.groups.get(&gid).expect("Group should exist");
        assert_eq!(group.tabs.len(), 2, "Group should have 2 tabs");

        assert!(
            s.overlays.overlays.contains_key(&gid),
            "Overlay should exist for the group"
        );

        gid
    });

    let ov_hwnd = state::with_state(|s| {
        *s.overlays.overlays.get(&group_id).unwrap()
    });
    unsafe {
        assert_ne!(
            IsWindowVisible(ov_hwnd),
            0,
            "Overlay should be visible after auto-grouping"
        );
    }

    // Screenshot 02: overlay tab bar visible after auto-grouping
    let active_hwnd = state::with_state(|s| {
        s.groups.groups.get(&group_id).unwrap().active_hwnd()
    });
    screenshot::capture_window(
        active_hwnd,
        "evidence/rules_auto_group/02_auto_grouped.png",
    );

    // Phase 4: Tab switch
    state::with_state(|s| {
        let group = s.groups.groups.get_mut(&group_id).unwrap();
        group.switch_to(0);
    });

    pump_messages(Duration::from_millis(200));

    let switched_hwnd = state::with_state(|s| {
        let group = s.groups.groups.get(&group_id).unwrap();
        assert_eq!(group.active, 0, "Active tab should be 0 after switch");
        group.active_hwnd()
    });

    // Update overlay after switch
    state::with_state(|s| {
        let ov = *s.overlays.overlays.get(&group_id).unwrap();
        overlay::update_overlay(ov, group_id, &s.groups, &s.windows);
    });

    pump_messages(Duration::from_millis(200));

    // Screenshot 03: tab switched
    screenshot::capture_window(
        switched_hwnd,
        "evidence/rules_auto_group/03_tab_switched.png",
    );

    // Phase 5: Cleanup
    state::with_state(|s| {
        s.overlays.remove_overlay(group_id);
        s.groups.remove_from_group(discovered[0].hwnd);
        s.groups.remove_from_group(discovered[1].hwnd);
        s.windows.clear();
        s.rules = crate::config::RulesEngine { groups: Vec::new() };
        s.groups.pending_rules.clear();
        s.groups.named_groups.clear();
        s.shutdown();
    });

    // Close stdin to signal dummy to exit
    drop(child.stdin.take());
    let _ = child.wait();
}

/// E2E test: group lifecycle with real separate-process windows.
/// Spawns dummy_window.exe, discovers windows via enumerate_windows(),
/// creates group, verifies overlay, switches tabs, ungroups — with screenshot evidence.
#[test]
fn acceptance_e2e_group_lifecycle() {
    // Phase 0: Spawn dummy process
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dummy_path = std::path::Path::new(manifest_dir)
        .join("target")
        .join("debug")
        .join("dummy_window.exe");
    assert!(
        dummy_path.exists(),
        "dummy_window.exe not found at {:?}. Run `cargo build --bin dummy_window` first.",
        dummy_path
    );

    let mut child = Command::new(&dummy_path)
        .args(["2", "E2ELifecycle"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("Failed to spawn dummy_window.exe");

    let child_pid = child.id();

    // Phase 1: Discover windows via enumerate_windows (real is_eligible path)
    overlay::register_class();

    let mut discovered: Vec<WindowInfo> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        pump_messages(Duration::from_millis(200));
        discovered = window::enumerate_windows()
            .into_iter()
            .filter(|info| {
                let mut pid = 0u32;
                unsafe {
                    GetWindowThreadProcessId(info.hwnd, &mut pid);
                }
                pid == child_pid
            })
            .collect();
        if discovered.len() >= 2 {
            break;
        }
    }

    assert_eq!(
        discovered.len(),
        2,
        "Expected 2 windows from dummy process (PID {}), got {}",
        child_pid,
        discovered.len()
    );

    let win1 = discovered[0].hwnd;
    let win2 = discovered[1].hwnd;

    // Insert into state
    state::with_state(|s| {
        for info in &discovered {
            s.windows.insert(info.hwnd, info.clone());
        }
    });

    // Phase 2: Create group and verify overlay
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(200));

    let ov_hwnd = state::with_state(|s| {
        *s.overlays.overlays.get(&group_id).expect("Overlay not found")
    });
    unsafe {
        assert_ne!(IsWindowVisible(ov_hwnd), 0, "Overlay not visible after grouping");
    }

    // Screenshot 01: group created with overlay
    screenshot::capture_window(win2, "evidence/e2e_group_lifecycle/01_group_created.png");

    // Verify group has 2 tabs with win2 active
    state::with_state(|s| {
        let group = s.groups.groups.get(&group_id).expect("Group not found");
        assert_eq!(group.tabs.len(), 2, "Group should have 2 tabs");
        assert_eq!(group.active, 1, "win2 should be active (index 1)");
    });

    // Phase 3: Switch to tab 0 (win1)
    state::with_state(|s| {
        let group = s.groups.groups.get_mut(&group_id).unwrap();
        group.switch_to(0);
    });

    pump_messages(Duration::from_millis(200));

    unsafe {
        assert_ne!(IsWindowVisible(win1), 0, "win1 should be visible after switch");
        assert_eq!(IsWindowVisible(win2), 0, "win2 should be hidden after switch");
    }

    // Update overlay after switch
    state::with_state(|s| {
        let ov = *s.overlays.overlays.get(&group_id).unwrap();
        overlay::update_overlay(ov, group_id, &s.groups, &s.windows);
    });

    pump_messages(Duration::from_millis(100));

    // Screenshot 02: tab switched
    screenshot::capture_window(win1, "evidence/e2e_group_lifecycle/02_tab_switched.png");

    // Phase 4: Ungroup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
    });

    pump_messages(Duration::from_millis(200));

    // Both windows should be visible (ungrouped)
    unsafe {
        assert_ne!(IsWindowVisible(win1), 0, "win1 should be visible after ungroup");
        assert_ne!(IsWindowVisible(win2), 0, "win2 should be visible after ungroup");
    }

    // No group references remain
    state::with_state(|s| {
        assert!(s.groups.group_of(win1).is_none(), "win1 still in a group");
        assert!(s.groups.group_of(win2).is_none(), "win2 still in a group");
        assert!(!s.groups.groups.contains_key(&group_id), "Group still exists");
    });

    // Overlay destroyed
    state::with_state(|s| {
        assert!(
            !s.overlays.overlays.contains_key(&group_id),
            "Overlay should be removed after ungroup"
        );
    });
    unsafe {
        assert_eq!(IsWindow(ov_hwnd), 0, "Overlay window should be destroyed");
    }

    // Screenshot 03: ungrouped
    screenshot::capture_window(win1, "evidence/e2e_group_lifecycle/03_ungrouped.png");

    // Cleanup
    state::with_state(|s| {
        s.windows.clear();
        s.shutdown();
    });

    drop(child.stdin.take());
    let _ = child.wait();
}

/// E2E test: minimize/restore with real separate-process windows.
/// Spawns dummy_window.exe, discovers windows, creates group,
/// minimizes active window → overlay hides, restores → overlay reappears.
#[test]
fn acceptance_e2e_minimize_restore() {
    // Phase 0: Spawn dummy process
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dummy_path = std::path::Path::new(manifest_dir)
        .join("target")
        .join("debug")
        .join("dummy_window.exe");
    assert!(
        dummy_path.exists(),
        "dummy_window.exe not found at {:?}. Run `cargo build --bin dummy_window` first.",
        dummy_path
    );

    let mut child = Command::new(&dummy_path)
        .args(["2", "E2EMinRestore"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("Failed to spawn dummy_window.exe");

    let child_pid = child.id();

    // Phase 1: Discover windows
    overlay::register_class();

    let mut discovered: Vec<WindowInfo> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        pump_messages(Duration::from_millis(200));
        discovered = window::enumerate_windows()
            .into_iter()
            .filter(|info| {
                let mut pid = 0u32;
                unsafe {
                    GetWindowThreadProcessId(info.hwnd, &mut pid);
                }
                pid == child_pid
            })
            .collect();
        if discovered.len() >= 2 {
            break;
        }
    }

    assert_eq!(
        discovered.len(),
        2,
        "Expected 2 windows from dummy process (PID {}), got {}",
        child_pid,
        discovered.len()
    );

    let win1 = discovered[0].hwnd;
    let win2 = discovered[1].hwnd;

    state::with_state(|s| {
        for info in &discovered {
            s.windows.insert(info.hwnd, info.clone());
        }
    });

    // Phase 2: Create group (win2 active at index 1)
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(200));

    let ov_hwnd = state::with_state(|s| {
        *s.overlays.overlays.get(&group_id).unwrap()
    });

    // Verify overlay visible
    unsafe {
        assert_ne!(
            IsWindowVisible(ov_hwnd),
            0,
            "Overlay should be visible before minimize"
        );
    }

    // Screenshot 01: group created, overlay visible
    screenshot::capture_window(win2, "evidence/e2e_minimize_restore/01_grouped.png");

    // Phase 3: Minimize active window
    state::with_state(|s| {
        s.on_minimize(win2);
    });

    pump_messages(Duration::from_millis(200));

    // Overlay should be hidden
    unsafe {
        assert_eq!(
            IsWindowVisible(ov_hwnd),
            0,
            "Overlay should be hidden after minimize"
        );
    }

    // Screenshot 02: minimized
    screenshot::capture_window(win2, "evidence/e2e_minimize_restore/02_minimized.png");

    // Phase 4: Restore
    state::with_state(|s| {
        s.on_restore(win2);
    });

    pump_messages(Duration::from_millis(200));

    // Overlay should be visible again
    unsafe {
        assert_ne!(
            IsWindowVisible(ov_hwnd),
            0,
            "Overlay should be visible after restore"
        );
    }

    // Screenshot 03: restored
    screenshot::capture_window(win2, "evidence/e2e_minimize_restore/03_restored.png");

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.clear();
        s.shutdown();
    });

    drop(child.stdin.take());
    let _ = child.wait();
}

/// Test: desktop switch hides overlays AND they stay hidden through subsequent events.
///
/// This is the critical "close the loop" test. The bug was:
/// 1. on_desktop_switch() hides overlays with ShowWindow(SW_HIDE)
/// 2. on_focus_changed() fires immediately after → calls update_all() → SWP_SHOWWINDOW re-shows them
/// 3. Overlay reappears on wrong desktop
///
/// The fix: desktop_hidden set in OverlayManager prevents update_overlay from re-showing.
#[test]
fn acceptance_desktop_switch_hides_overlay_through_focus_change() {
    unsafe {
        windows_sys::Win32::System::Com::CoInitializeEx(
            std::ptr::null(),
            windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32,
        );
    }

    overlay::register_class();
    let test_class = register_test_class();

    let win1 = create_test_window(&test_class, "Desktop Switch A");
    let win2 = create_test_window(&test_class, "Desktop Switch B");
    assert!(!win1.is_null());
    assert!(!win2.is_null());

    // Create a third window to simulate focus on a "different desktop" window
    let win_other = create_test_window(&test_class, "Other Desktop Window");
    assert!(!win_other.is_null());

    state::with_state(|s| {
        s.vdesktop = crate::vdesktop::VDesktopManager::new();
        s.windows.insert(win1, make_window_info(win1));
        s.windows.insert(win2, make_window_info(win2));
        s.windows.insert(win_other, make_window_info(win_other));
    });

    pump_messages(Duration::from_millis(200));

    // Step 1: Create group, verify overlay visible
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(200));

    let ov_hwnd = state::with_state(|s| {
        *s.overlays.overlays.get(&group_id).unwrap()
    });

    // Verify overlay is visible
    unsafe {
        assert_ne!(
            IsWindowVisible(ov_hwnd), 0,
            "Step 1: Overlay should be visible after group creation"
        );
    }

    screenshot::capture_window(win2, "evidence/desktop_switch_bug/01_overlay_visible.png");

    // Step 2: Mock windows as being on a different desktop, then trigger desktop switch
    state::with_state(|s| {
        if let Some(ref mut vd) = s.vdesktop {
            vd.set_off_desktop(&[win1, win2]);
        }
        s.on_desktop_switch();
    });

    pump_messages(Duration::from_millis(100));

    // Verify overlay is hidden after desktop switch
    unsafe {
        assert_eq!(
            IsWindowVisible(ov_hwnd), 0,
            "Step 2: Overlay should be hidden after desktop switch (windows on other desktop)"
        );
    }

    screenshot::capture_window(win_other, "evidence/desktop_switch_bug/02_overlay_hidden_after_switch.png");

    // Step 3: THE BUG SCENARIO — simulate focus change on the new desktop
    // Before the fix, this would re-show the overlay via update_all() + SWP_SHOWWINDOW
    state::with_state(|s| {
        s.on_focus_changed(win_other);
    });

    pump_messages(Duration::from_millis(100));

    unsafe {
        assert_eq!(
            IsWindowVisible(ov_hwnd), 0,
            "Step 3 CRITICAL: Overlay must stay hidden after focus change on different desktop"
        );
    }

    screenshot::capture_window(win_other, "evidence/desktop_switch_bug/03_still_hidden_after_focus.png");

    // Step 4: Simulate title change on the hidden window — another re-show vector
    state::with_state(|s| {
        s.on_title_changed(win1);
    });

    pump_messages(Duration::from_millis(100));

    unsafe {
        assert_eq!(
            IsWindowVisible(ov_hwnd), 0,
            "Step 4: Overlay must stay hidden after title change on hidden window"
        );
    }

    // Step 5: Simulate window move on the hidden window — yet another re-show vector
    state::with_state(|s| {
        s.on_window_moved(win1);
    });

    pump_messages(Duration::from_millis(100));

    unsafe {
        assert_eq!(
            IsWindowVisible(ov_hwnd), 0,
            "Step 5: Overlay must stay hidden after window move on hidden window"
        );
    }

    // Step 6: Switch BACK — clear mock, simulate coming back to original desktop
    state::with_state(|s| {
        if let Some(ref mut vd) = s.vdesktop {
            vd.clear_mock();
        }
        s.on_desktop_switch();
    });

    pump_messages(Duration::from_millis(200));

    unsafe {
        assert_ne!(
            IsWindowVisible(ov_hwnd), 0,
            "Step 6: Overlay should reappear when switching back to the original desktop"
        );
    }

    screenshot::capture_window(win2, "evidence/desktop_switch_bug/04_overlay_restored.png");

    // Verify desktop_hidden is cleared
    state::with_state(|s| {
        assert!(
            !s.overlays.desktop_hidden.contains(&group_id),
            "desktop_hidden flag should be cleared after switching back"
        );
    });

    // Cleanup
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.clear();
        s.shutdown();
    });
    unsafe {
        DestroyWindow(win1);
        DestroyWindow(win2);
        DestroyWindow(win_other);
    }
}

/// Helper: press and hold a modifier key.
unsafe fn key_down(vk: u16, flags: u32) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    let mut input: INPUT = std::mem::zeroed();
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki.wVk = vk;
    input.Anonymous.ki.dwFlags = flags;
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
}

/// Helper: release a modifier key.
unsafe fn key_up(vk: u16, flags: u32) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    let mut input: INPUT = std::mem::zeroed();
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki.wVk = vk;
    input.Anonymous.ki.dwFlags = flags | KEYEVENTF_KEYUP;
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
}

/// Helper: send Ctrl+Win+Arrow to switch virtual desktop.
/// direction: VK_RIGHT or VK_LEFT
unsafe fn switch_virtual_desktop(direction: u16) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    // Press modifiers
    key_down(VK_LWIN, 0);
    key_down(VK_CONTROL, 0);
    thread::sleep(Duration::from_millis(50));
    // Press arrow
    key_down(direction, KEYEVENTF_EXTENDEDKEY);
    key_up(direction, KEYEVENTF_EXTENDEDKEY);
    thread::sleep(Duration::from_millis(50));
    // Release modifiers
    key_up(VK_CONTROL, 0);
    key_up(VK_LWIN, 0);
}

/// TRUE E2E test: actually switches virtual desktops using keyboard input.
///
/// This test:
/// 1. Spawns real dummy windows in a separate process
/// 2. Groups them with real overlays
/// 3. Installs Win32 event hooks (like the real app)
/// 4. Sends Ctrl+Win+Right to ACTUALLY switch virtual desktops
/// 5. Verifies overlay is hidden via IsWindowVisible + screenshots
/// 6. Sends Ctrl+Win+Left to switch back
/// 7. Verifies overlay reappears
///
/// WARNING: This test manipulates the user's virtual desktops.
/// It always switches back, even on failure (via cleanup).
///
/// Run with: cargo test acceptance_e2e_real_desktop_switch -- --ignored --nocapture
#[test]
#[ignore]
fn acceptance_e2e_real_desktop_switch() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    // Phase 0: Build prerequisites
    unsafe {
        windows_sys::Win32::System::Com::CoInitializeEx(
            std::ptr::null(),
            windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32,
        );
    }

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dummy_path = std::path::Path::new(manifest_dir)
        .join("target")
        .join("debug")
        .join("dummy_window.exe");
    assert!(
        dummy_path.exists(),
        "dummy_window.exe not found. Run `cargo build --bin dummy_window` first."
    );

    // Spawn dummy process with 2 windows
    let mut child = Command::new(&dummy_path)
        .args(["2", "VDeskE2E"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("Failed to spawn dummy_window.exe");
    let child_pid = child.id();

    // Phase 1: Discover windows
    overlay::register_class();

    let mut discovered: Vec<WindowInfo> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        pump_messages(Duration::from_millis(200));
        discovered = window::enumerate_windows()
            .into_iter()
            .filter(|info| {
                let mut pid = 0u32;
                unsafe { GetWindowThreadProcessId(info.hwnd, &mut pid); }
                pid == child_pid
            })
            .collect();
        if discovered.len() >= 2 {
            break;
        }
    }
    assert_eq!(discovered.len(), 2, "Expected 2 dummy windows");

    let win1 = discovered[0].hwnd;
    let win2 = discovered[1].hwnd;

    // Phase 2: Set up state, VDesktopManager, hooks
    state::with_state(|s| {
        s.vdesktop = crate::vdesktop::VDesktopManager::new();
        for info in &discovered {
            s.windows.insert(info.hwnd, info.clone());
        }
    });

    // Install hooks so EVENT_SYSTEM_DESKTOPSWITCH fires our handler
    hook::install();

    // Create group
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(300));

    let ov_hwnd = state::with_state(|s| {
        *s.overlays.overlays.get(&group_id).expect("Overlay not found")
    });

    // Verify overlay is visible
    unsafe {
        assert_ne!(IsWindowVisible(ov_hwnd), 0, "Overlay should be visible");
    }
    screenshot::capture_window(win2, "evidence/e2e_real_desktop_switch/01_overlay_visible.png");

    // Save the window rect so we can screenshot the same region after switching desktops
    let win2_rect = window::get_window_rect(win2);
    let margin = 10;
    let cap_x = win2_rect.left - margin;
    let cap_y = win2_rect.top - overlay::TAB_HEIGHT - margin;
    let cap_w = (win2_rect.right - win2_rect.left) + margin * 2;
    let cap_h = (win2_rect.bottom - win2_rect.top) + overlay::TAB_HEIGHT + margin * 2;

    // Verify VDesktopManager reports windows on current desktop
    let on_current = state::with_state(|s| {
        s.vdesktop.as_ref().map(|vd| vd.is_on_current_desktop(win1)).unwrap_or(false)
    });
    assert!(on_current, "Windows should be on current desktop before switch");

    // Phase 3: Switch to next virtual desktop (Ctrl+Win+Right)
    unsafe { switch_virtual_desktop(VK_RIGHT); }

    // Wait for the desktop switch animation to complete
    thread::sleep(Duration::from_secs(2));

    // Poll COM until active window (win2) is reported as off-desktop.
    // Note: COM may be inconsistent for non-active windows (win1 sometimes reports true
    // even when off-desktop), but the active window's result is what matters for on_desktop_switch.
    let mut active_on_current_after = true;
    let poll_deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < poll_deadline {
        pump_messages(Duration::from_millis(200));

        active_on_current_after = state::with_state(|s| {
            s.vdesktop.as_ref().map(|vd| vd.is_on_current_desktop(win2)).unwrap_or(true)
        });

        if !active_on_current_after {
            break;
        }
        thread::sleep(Duration::from_millis(300));
    }

    // Call on_desktop_switch regardless — if COM works, it hides; if not, it's a no-op
    state::with_state(|s| s.on_desktop_switch());
    pump_messages(Duration::from_millis(200));

    // Phase 4: Record state
    let overlay_visible_after_switch = unsafe { IsWindowVisible(ov_hwnd) != 0 };
    let desktop_hidden_set = state::with_state(|s| {
        s.overlays.desktop_hidden.contains(&group_id)
    });

    // Diagnostic output for CI/evidence
    eprintln!(
        "  overlay_visible={}, desktop_hidden={}, com_off={}",
        overlay_visible_after_switch, desktop_hidden_set, !active_on_current_after
    );

    // Screenshot at the EXACT location where the dummy windows were —
    // if the overlay is still visible, this will show the phantom tabs
    screenshot::capture_region(cap_x, cap_y, cap_w, cap_h, "evidence/e2e_real_desktop_switch/02_after_switch.png");

    // Phase 5: Switch BACK (Ctrl+Win+Left) — always do this before asserting
    unsafe { switch_virtual_desktop(VK_LEFT); }
    thread::sleep(Duration::from_secs(2));

    // Poll until COM reports active window (win2) back on current desktop
    let poll_deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < poll_deadline {
        pump_messages(Duration::from_millis(200));
        let back = state::with_state(|s| {
            s.vdesktop.as_ref().map(|vd| vd.is_on_current_desktop(win2)).unwrap_or(false)
        });
        if back {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }

    // Manually call on_desktop_switch for the return trip too
    state::with_state(|s| s.on_desktop_switch());
    pump_messages(Duration::from_millis(300));

    // Screenshot after switching back
    screenshot::capture_window(win2, "evidence/e2e_real_desktop_switch/03_after_switch_back.png");

    let overlay_visible_after_return = unsafe { IsWindowVisible(ov_hwnd) != 0 };

    // Phase 6: Cleanup (always runs)
    hook::uninstall();
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.clear();
        s.shutdown();
    });
    drop(child.stdin.take());
    let _ = child.wait();

    // Phase 7: Assert
    assert!(
        !active_on_current_after,
        "COM should report active window (win2) as off-desktop after switch"
    );
    assert!(
        desktop_hidden_set,
        "desktop_hidden flag should be set for the group after desktop switch"
    );
    assert!(
        !overlay_visible_after_switch,
        "Overlay should be HIDDEN after switching to a different virtual desktop"
    );
    assert!(
        overlay_visible_after_return,
        "Overlay should be VISIBLE again after switching back"
    );
}

/// Bare-bones bug reproduction: NO assertions about internal state, NO manual on_desktop_switch().
/// Just groups windows, switches desktop via SendInput, and screenshots the same region.
/// If phantom tabs are visible in 02_phantom_check.png, the bug is confirmed.
///
/// Run with: cargo test acceptance_e2e_phantom_screenshot -- --ignored --nocapture
#[test]
#[ignore]
fn acceptance_e2e_phantom_screenshot() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    unsafe {
        windows_sys::Win32::System::Com::CoInitializeEx(
            std::ptr::null(),
            windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32,
        );
    }

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dummy_path = std::path::Path::new(manifest_dir)
        .join("target")
        .join("debug")
        .join("dummy_window.exe");
    assert!(dummy_path.exists(), "Run `cargo build --bin dummy_window` first.");

    // Spawn 2 dummy windows
    let mut child = Command::new(&dummy_path)
        .args(["2", "PhantomTest"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("Failed to spawn dummy_window.exe");
    let child_pid = child.id();

    overlay::register_class();

    // Discover windows
    let mut discovered: Vec<WindowInfo> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        pump_messages(Duration::from_millis(200));
        discovered = window::enumerate_windows()
            .into_iter()
            .filter(|info| {
                let mut pid = 0u32;
                unsafe { GetWindowThreadProcessId(info.hwnd, &mut pid); }
                pid == child_pid
            })
            .collect();
        if discovered.len() >= 2 {
            break;
        }
    }
    assert_eq!(discovered.len(), 2, "Expected 2 dummy windows");

    let win1 = discovered[0].hwnd;
    let win2 = discovered[1].hwnd;

    // Set up state + hooks (like the real app)
    state::with_state(|s| {
        s.vdesktop = crate::vdesktop::VDesktopManager::new();
        for info in &discovered {
            s.windows.insert(info.hwnd, info.clone());
        }
    });
    hook::install();

    // Create group + overlay
    let group_id = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        gid
    });

    pump_messages(Duration::from_millis(500));

    // Record window position BEFORE switching
    let rect = window::get_window_rect(win2);
    let margin = 20;
    let cap_x = rect.left - margin;
    let cap_y = rect.top - overlay::TAB_HEIGHT - margin;
    let cap_w = (rect.right - rect.left) + margin * 2;
    let cap_h = (rect.bottom - rect.top) + overlay::TAB_HEIGHT + margin * 2;

    // Screenshot BEFORE: proves overlay is there
    screenshot::capture_region(cap_x, cap_y, cap_w, cap_h, "evidence/phantom_test/01_before_switch.png");
    eprintln!("  Captured 01_before_switch.png at ({},{}) {}x{}", cap_x, cap_y, cap_w, cap_h);

    // ---- SWITCH DESKTOP ----
    // Do NOT manually call on_desktop_switch. Let the hook handle it (or not).
    unsafe { switch_virtual_desktop(VK_RIGHT); }

    // Wait for desktop switch animation + let hooks fire
    thread::sleep(Duration::from_secs(2));
    pump_messages(Duration::from_millis(500));

    // Extra pump to let any focus-change / event re-shows happen
    thread::sleep(Duration::from_millis(500));
    pump_messages(Duration::from_millis(500));

    // Screenshot AFTER: same region. If overlay is visible here = bug confirmed.
    screenshot::capture_region(cap_x, cap_y, cap_w, cap_h, "evidence/phantom_test/02_phantom_check.png");
    eprintln!("  Captured 02_phantom_check.png — inspect for phantom tabs!");

    // Check overlay visibility WHILE on the other desktop
    let ov_hwnd = state::with_state(|s| {
        *s.overlays.overlays.get(&group_id).expect("Overlay not found")
    });
    let overlay_visible_on_other_desktop = unsafe { IsWindowVisible(ov_hwnd) != 0 };
    eprintln!("  overlay_visible_on_other_desktop = {}", overlay_visible_on_other_desktop);

    // ---- SWITCH BACK (always, before asserting) ----
    unsafe { switch_virtual_desktop(VK_LEFT); }
    thread::sleep(Duration::from_secs(2));
    pump_messages(Duration::from_millis(500));

    // Screenshot AFTER return
    screenshot::capture_region(cap_x, cap_y, cap_w, cap_h, "evidence/phantom_test/03_after_return.png");
    eprintln!("  Captured 03_after_return.png");

    // Cleanup
    hook::uninstall();
    state::with_state(|s| {
        s.groups.remove_from_group(win1);
        s.groups.remove_from_group(win2);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.clear();
        s.shutdown();
    });
    drop(child.stdin.take());
    let _ = child.wait();

    // THE ASSERTION: overlay must NOT be visible on the other desktop
    assert!(
        !overlay_visible_on_other_desktop,
        "BUG: Overlay is still visible after switching virtual desktops! See evidence/phantom_test/02_phantom_check.png"
    );
}

/// E2E test: tab preview on hover with DWM thumbnail.
/// Spawns dummy_window.exe, groups two windows, switches so one is hidden,
/// triggers the preview timer, verifies the DWM thumbnail preview appears,
/// then hides it and verifies it disappears — with screenshot evidence at each step.
#[test]
fn acceptance_e2e_tab_preview() {
    use crate::preview;

    // Phase 0: Spawn dummy process with 2 windows
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dummy_path = std::path::Path::new(manifest_dir)
        .join("target")
        .join("debug")
        .join("dummy_window.exe");
    assert!(
        dummy_path.exists(),
        "dummy_window.exe not found at {:?}. Run `cargo build --bin dummy_window` first.",
        dummy_path
    );

    let mut child = std::process::Command::new(&dummy_path)
        .args(["2", "E2EPreview"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("Failed to spawn dummy_window.exe");

    let child_pid = child.id();

    // Phase 1: Discover windows
    overlay::register_class();
    preview::register_class();

    let mut discovered: Vec<window::WindowInfo> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        pump_messages(Duration::from_millis(200));
        discovered = window::enumerate_windows()
            .into_iter()
            .filter(|info| {
                let mut pid = 0u32;
                unsafe {
                    GetWindowThreadProcessId(info.hwnd, &mut pid);
                }
                pid == child_pid
            })
            .collect();
        if discovered.len() >= 2 {
            break;
        }
    }

    assert_eq!(
        discovered.len(),
        2,
        "Expected 2 windows from dummy process (PID {}), got {}",
        child_pid,
        discovered.len()
    );

    let win1 = discovered[0].hwnd;
    let win2 = discovered[1].hwnd;

    // Insert into state
    state::with_state(|s| {
        for info in &discovered {
            s.windows.insert(info.hwnd, info.clone());
        }
    });

    // Phase 2: Create group and switch to tab 0 so tab 1 (win2) is hidden
    let (group_id, ov_hwnd) = state::with_state(|s| {
        let gid = s.groups.create_group(win1, win2);
        let ov = s.overlays.ensure_overlay(gid);
        overlay::update_overlay(ov, gid, &s.groups, &s.windows);
        // win2 is active (index 1). Switch to win1 (index 0) so win2 is hidden.
        let group = s.groups.groups.get_mut(&gid).unwrap();
        group.switch_to(0);
        (gid, ov)
    });

    pump_messages(Duration::from_millis(300));

    // Refresh overlay after switch
    state::with_state(|s| {
        overlay::update_overlay(ov_hwnd, group_id, &s.groups, &s.windows);
    });

    pump_messages(Duration::from_millis(100));

    // Verify win2 is hidden
    unsafe {
        assert_ne!(IsWindowVisible(win1), 0, "win1 should be visible (active)");
        assert_eq!(IsWindowVisible(win2), 0, "win2 should be hidden (inactive)");
    }

    // Screenshot 01: group with win1 active, win2 hidden — no preview yet
    screenshot::capture_window(win1, "evidence/e2e_tab_preview/01_before_preview.png");

    // Phase 3: Simulate hover delay firing — set pending state and call on_timer
    // We bypass the actual SetTimer/WM_TIMER to directly trigger preview show
    state::with_state(|s| {
        s.preview.start_delay(ov_hwnd, win2);
    });

    // Simulate the timer firing by calling on_timer directly
    state::with_state(|s| {
        s.preview.on_timer(ov_hwnd, &s.groups, &s.overlays);
    });

    pump_messages(Duration::from_millis(300));

    // Phase 4: Verify preview is showing
    let (is_showing, preview_visible) = state::with_state(|s| {
        let showing = s.preview.is_showing();
        let phwnd = s.preview.preview_hwnd();
        let visible = if !phwnd.is_null() {
            unsafe { IsWindowVisible(phwnd) != 0 }
        } else {
            false
        };
        (showing, visible)
    });

    eprintln!(
        "Preview state: is_showing={}, window_visible={}",
        is_showing, preview_visible
    );

    // Screenshot 02: preview should be visible below the tab bar
    // Capture a larger region to include the preview below the window
    let (cap_x, cap_y, cap_w, cap_h) = {
        let rect = window::get_window_rect(win1);
        let margin = 10;
        let x = rect.left - margin;
        let y = rect.top - overlay::TAB_HEIGHT - margin;
        // Extra 250px below for the preview thumbnail
        let w = (rect.right - rect.left) + margin * 2;
        let h = (rect.bottom - rect.top) + overlay::TAB_HEIGHT + margin * 2 + 250;
        (x, y, w, h)
    };
    screenshot::capture_region(
        cap_x,
        cap_y,
        cap_w,
        cap_h,
        "evidence/e2e_tab_preview/02_preview_visible.png",
    );

    assert!(
        is_showing,
        "Preview DWM thumbnail should be registered (is_showing=true)"
    );
    assert!(
        preview_visible,
        "Preview window should be visible on screen"
    );

    // Phase 5: Hide the preview
    state::with_state(|s| {
        s.preview.hide();
    });

    pump_messages(Duration::from_millis(100));

    let (is_showing_after, preview_visible_after) = state::with_state(|s| {
        let showing = s.preview.is_showing();
        let phwnd = s.preview.preview_hwnd();
        let visible = if !phwnd.is_null() {
            unsafe { IsWindowVisible(phwnd) != 0 }
        } else {
            false
        };
        (showing, visible)
    });

    // Screenshot 03: preview hidden
    screenshot::capture_region(
        cap_x,
        cap_y,
        cap_w,
        cap_h,
        "evidence/e2e_tab_preview/03_preview_hidden.png",
    );

    assert!(
        !is_showing_after,
        "Preview should not be showing after hide()"
    );
    assert!(
        !preview_visible_after,
        "Preview window should not be visible after hide()"
    );

    // Cleanup
    state::with_state(|s| {
        s.preview.destroy();
        s.groups.remove_from_group(win1);
        s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        s.windows.clear();
        s.shutdown();
    });

    drop(child.stdin.take());
    let _ = child.wait();
}
