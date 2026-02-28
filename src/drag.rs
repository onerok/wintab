use std::cell::RefCell;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::group::GroupId;
use crate::overlay;
use crate::state;

const DRAG_THRESHOLD: i32 = 5;

/// Check if movement exceeds drag threshold.
fn exceeds_drag_threshold(dx: i32, dy: i32) -> bool {
    dx.abs() > DRAG_THRESHOLD || dy.abs() > DRAG_THRESHOLD
}

struct DragState {
    source_overlay: HWND,
    source_group: GroupId,
    source_tab: usize,
    start_x: i32,
    start_y: i32,
    dragging: bool,
}

thread_local! {
    static DRAG: RefCell<Option<DragState>> = RefCell::new(None);
}

pub fn on_mouse_down(overlay_hwnd: HWND, group_id: GroupId, tab_index: usize, x: i32, y: i32) {
    let mut pt = POINT { x, y };
    unsafe {
        ClientToScreen(overlay_hwnd, &mut pt);
        SetCapture(overlay_hwnd);
    }

    DRAG.with(|d| {
        *d.borrow_mut() = Some(DragState {
            source_overlay: overlay_hwnd,
            source_group: group_id,
            source_tab: tab_index,
            start_x: pt.x,
            start_y: pt.y,
            dragging: false,
        });
    });
}

pub fn on_mouse_move(overlay_hwnd: HWND, x: i32, y: i32) {
    DRAG.with(|d| {
        let mut drag_opt = d.borrow_mut();
        let Some(drag) = drag_opt.as_mut() else {
            return;
        };

        let mut pt = POINT { x, y };
        unsafe {
            ClientToScreen(overlay_hwnd, &mut pt);
        }

        if !drag.dragging {
            let dx = pt.x - drag.start_x;
            let dy = pt.y - drag.start_y;
            if exceeds_drag_threshold(dx, dy) {
                drag.dragging = true;
                unsafe {
                    SetCursor(LoadCursorW(0 as _, IDC_SIZEALL));
                }
            }
        }
    });
}

pub fn on_mouse_up(_overlay_hwnd: HWND, x: i32, y: i32) {
    unsafe {
        ReleaseCapture();
    }

    let drag = DRAG.with(|d| d.borrow_mut().take());
    let Some(drag) = drag else {
        return;
    };

    if !drag.dragging {
        // Click — switch tabs
        state::with_state(|s| {
            if let Some(group) = s.groups.groups.get_mut(&drag.source_group) {
                group.switch_to(drag.source_tab);
            }
            if let Some(&ov) = s.overlays.overlays.get(&drag.source_group) {
                overlay::update_overlay(ov, drag.source_group, &s.groups, &s.windows);
            }
        });
        return;
    }

    // Drag completed — determine drop target
    let mut screen_pt = POINT { x, y };
    unsafe {
        ClientToScreen(drag.source_overlay, &mut screen_pt);
    }
    let target_hwnd = unsafe { WindowFromPoint(screen_pt) };

    state::with_state(|s| {
        let dragged_hwnd = {
            let Some(group) = s.groups.groups.get(&drag.source_group) else {
                return;
            };
            if drag.source_tab >= group.tabs.len() {
                return;
            }
            group.tabs[drag.source_tab]
        };

        let target_group = s.overlays.group_for_overlay(target_hwnd);

        if let Some(target_gid) = target_group {
            if target_gid != drag.source_group {
                s.groups.remove_from_group(dragged_hwnd);
                s.groups.add_to_group(target_gid, dragged_hwnd);

                update_source_overlay(s, drag.source_group);
                let ov = s.overlays.ensure_overlay(target_gid);
                overlay::update_overlay(ov, target_gid, &s.groups, &s.windows);
            }
        } else {
            let target_managed = s.find_managed_window_at(screen_pt);

            if let Some(target_win) = target_managed {
                if target_win != dragged_hwnd {
                    if let Some(target_gid) = s.groups.group_of(target_win) {
                        s.groups.remove_from_group(dragged_hwnd);
                        s.groups.add_to_group(target_gid, dragged_hwnd);
                        update_source_overlay(s, drag.source_group);
                        let ov = s.overlays.ensure_overlay(target_gid);
                        overlay::update_overlay(ov, target_gid, &s.groups, &s.windows);
                    } else {
                        s.groups.remove_from_group(dragged_hwnd);
                        let new_gid = s.groups.create_group(target_win, dragged_hwnd);
                        update_source_overlay(s, drag.source_group);
                        let ov = s.overlays.ensure_overlay(new_gid);
                        overlay::update_overlay(ov, new_gid, &s.groups, &s.windows);
                    }
                }
            } else {
                // Dropped on empty space → detach
                if s.groups.group_of(dragged_hwnd).is_some() {
                    s.groups.remove_from_group(dragged_hwnd);
                    update_source_overlay(s, drag.source_group);
                }
            }
        }
    });
}

fn update_source_overlay(s: &mut crate::state::AppState, source_group: GroupId) {
    s.overlays.refresh_overlay(source_group, &s.groups, &s.windows);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_not_exceeded_at_origin() {
        assert!(!exceeds_drag_threshold(0, 0));
    }

    #[test]
    fn threshold_not_exceeded_within_range() {
        assert!(!exceeds_drag_threshold(3, 4));
        assert!(!exceeds_drag_threshold(5, 5)); // exactly at threshold, not exceeded
        assert!(!exceeds_drag_threshold(-5, -5));
    }

    #[test]
    fn threshold_exceeded_x() {
        assert!(exceeds_drag_threshold(6, 0));
        assert!(exceeds_drag_threshold(-6, 0));
    }

    #[test]
    fn threshold_exceeded_y() {
        assert!(exceeds_drag_threshold(0, 6));
        assert!(exceeds_drag_threshold(0, -6));
    }

    #[test]
    fn threshold_exceeded_both() {
        assert!(exceeds_drag_threshold(10, 10));
    }
}
