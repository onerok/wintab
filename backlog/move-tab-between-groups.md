# Move Tab Between Groups

## Problem

Users can drag tabs to create or dissolve groups, but cannot move a tab from one existing group to another. Moving a tab between groups requires ungrouping it first (drag to empty space) and then re-grouping it (drag onto the target group). A direct drag from Group A's overlay to Group B's overlay would streamline multi-group workflows.

## Location

**Files modified:**

- `src/drag.rs` — Extend `on_mouse_move` drop target detection to check for overlay windows under the cursor. Add cross-group drop handling in `on_mouse_up`.
- `src/group.rs` — Add `TabGroup::transfer_out()` that removes by index without showing the window (for transfers where the target group will manage visibility).
- `src/state.rs` — Add `move_tab_to_group()` method that removes from source and adds to target atomically.
- `src/overlay.rs` — Update drop preview to highlight target overlay as a valid drop zone.

**New files:** None.

## Requirements

- [ ] Dragging a tab from Group A's overlay and dropping it on Group B's overlay moves the tab to Group B.
- [ ] The moved tab becomes the active tab in Group B.
- [ ] If Group A is left with fewer than 2 tabs, it dissolves (existing `remove` behavior).
- [ ] The drop preview overlay highlights Group B's tab bar as a valid drop target during the drag.
- [ ] The source tab's position in Group B matches the drop location within the tab bar (insert at the dropped index).
- [ ] Moving the last tab out of a group dissolves the group and shows the remaining window.

## Suggested Implementation

### Drop target detection

Extend `on_mouse_move` in `drag.rs` to detect overlay windows under the cursor:

```rust
fn find_drop_target(screen_x: i32, screen_y: i32, source_overlay: HWND) -> DropTarget {
    let pt = POINT { x: screen_x, y: screen_y };
    let hwnd_under = unsafe { WindowFromPoint(pt) };
    if hwnd_under.is_null() { return DropTarget::Empty; }

    // Check if target is a different overlay window
    if hwnd_under != source_overlay && is_overlay_window(hwnd_under) {
        if let Some(group_id) = get_overlay_group_id(hwnd_under) {
            let tab_index = calculate_drop_index(hwnd_under, screen_x);
            return DropTarget::Overlay(group_id, tab_index);
        }
    }

    // Existing: check for managed windows
    let root = unsafe { GetAncestor(hwnd_under, GA_ROOT) };
    if state::try_with_state_ret(|s| s.groups.window_to_group.contains_key(&root)) == Some(true) {
        return DropTarget::ManagedWindow(root);
    }
    DropTarget::Empty
}

enum DropTarget {
    Overlay(GroupId, usize),
    ManagedWindow(HWND),
    Empty,
}
```

### Overlay window identification

```rust
fn is_overlay_window(hwnd: HWND) -> bool {
    window::get_class_name(hwnd) == "WinTabOverlay"
}

fn get_overlay_group_id(overlay_hwnd: HWND) -> Option<GroupId> {
    let ptr = unsafe { GetWindowLongPtrW(overlay_hwnd, GWLP_USERDATA) } as *mut OverlayData;
    if ptr.is_null() { return None; }
    Some(unsafe { (*ptr).group_id })
}
```

### Cross-group drop in `on_mouse_up`

```rust
DropTarget::Overlay(target_group, insert_index) => {
    if target_group == drag.source_group {
        // Same group — delegate to reorder logic
        return;
    }
    state::with_state(|s| {
        s.move_tab_to_group(drag.source_group, drag.source_tab, target_group, insert_index);
    });
}
```

### `move_tab_to_group` in `state.rs`

```rust
pub fn move_tab_to_group(
    &mut self, source_gid: GroupId, source_idx: usize,
    target_gid: GroupId, target_idx: usize,
) {
    let hwnd = match self.groups.groups.get(&source_gid) {
        Some(g) => match g.tabs.get(source_idx) { Some(&h) => h, None => return },
        None => return,
    };

    // Remove from source (without showing — target will manage)
    let source_dissolved = if let Some(g) = self.groups.groups.get_mut(&source_gid) {
        g.tabs.retain(|&h| h != hwnd);
        if g.active >= g.tabs.len() && !g.tabs.is_empty() {
            g.active = g.tabs.len() - 1;
        }
        g.tabs.len() <= 1
    } else { return };

    self.groups.window_to_group.remove(&hwnd);

    // Add to target
    if let Some(g) = self.groups.groups.get_mut(&target_gid) {
        unsafe { ShowWindow(hwnd, SW_HIDE); }
        let at = target_idx.min(g.tabs.len());
        g.tabs.insert(at, hwnd);
        self.groups.window_to_group.insert(hwnd, target_gid);
        g.switch_to(at);
    }

    // Handle source dissolution
    if source_dissolved {
        if let Some(g) = self.groups.groups.get(&source_gid) {
            if let Some(&rem) = g.tabs.first() {
                unsafe { ShowWindow(rem, SW_SHOW); }
                self.groups.window_to_group.remove(&rem);
            }
        }
        self.groups.groups.remove(&source_gid);
        self.overlays.remove_overlay(source_gid);
    } else {
        self.overlays.refresh_overlay(source_gid, &self.groups, &self.windows);
    }
    self.overlays.refresh_overlay(target_gid, &self.groups, &self.windows);
}
```

## Edge Cases

- **Dragging to same group**: If `target_group == source_group`, treat as reorder (see `tab-reordering.md`), not a cross-group move.

- **Source group dissolves**: If the source had exactly 2 tabs, removing one dissolves it. Show the remaining window and clean up the overlay.

- **Position sync**: The moved window should adopt the target group's position/size via `SetWindowPos` before being shown.

- **Named groups (rule-created)**: Moving a tab out of a rule-created group doesn't change the rule. Clear `named_groups` only if the group dissolves.

- **Drag over own overlay**: Exclude `source_overlay` from drop target detection to prevent self-drops.

- **Preview cleanup**: If the source group had an active DWM thumbnail preview, cancel it when the tab moves.

- **Tab at capacity**: No max tab count exists, so insertion must handle groups with many narrow tabs.
