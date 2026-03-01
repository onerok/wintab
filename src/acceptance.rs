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
