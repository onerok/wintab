# Tab Reordering

## Problem

Tabs within a group are ordered by insertion time (the order windows were added to the group). Users cannot rearrange tabs to match their preferred workflow order. Browser-style drag-to-reorder within the tab bar is the expected interaction pattern.

## Location

**Files modified:**

- `src/drag.rs` — Extend `DragState` to track intra-bar reorder drags (distinguish from cross-group/detach drags). Add reorder drop logic.
- `src/overlay.rs` — Render drop indicator (insertion line) during reorder drag. Update `paint_tabs()` to show visual feedback.
- `src/group.rs` — Add `TabGroup::move_tab(from: usize, to: usize)` method to swap tab positions.
- `src/config.rs` — Add `allow_tab_reordering: bool` field (default: `true`).

**New files:** None.

## Requirements

- [ ] Dragging a tab left or right within the same tab bar reorders it.
- [ ] A visual drop indicator (vertical line or gap) shows where the tab will land during the drag.
- [ ] The existing 5px drag threshold (`DRAG_THRESHOLD`) applies — small mouse movements remain clicks, not drags.
- [ ] Dropping a tab at its current position is a no-op (no flicker or state change).
- [ ] Reordering preserves the active tab — if the active tab is moved, `active` index updates accordingly.
- [ ] Reordering works with any number of tabs (2 to many).
- [ ] If the drag exits the tab bar vertically, it transitions to a normal detach drag (existing behavior).
- [ ] Can be disabled via config (`allow_tab_reordering: false`).

## Suggested Implementation

### Distinguishing reorder from detach

The current `DragState` in `drag.rs` tracks:

```rust
struct DragState {
    source_overlay: HWND,
    source_group: GroupId,
    source_tab: usize,
    start_x: i32,
    start_y: i32,
    dragging: bool,
}
```

Extend with a reorder mode:

```rust
struct DragState {
    source_overlay: HWND,
    source_group: GroupId,
    source_tab: usize,
    start_x: i32,
    start_y: i32,
    dragging: bool,
    reorder_target: Option<usize>,  // target index within same group
}
```

In `on_mouse_move`, after the drag threshold is exceeded, check whether the cursor is still within the vertical bounds of the source overlay:

```rust
if dragging {
    let mut overlay_rect = RECT::default();
    GetWindowRect(drag.source_overlay, &mut overlay_rect);

    if y >= overlay_rect.top && y <= overlay_rect.bottom {
        // Still within the tab bar — this is a reorder drag
        let target_index = calculate_tab_index(x - overlay_rect.left, overlay_width, tab_count);
        drag.reorder_target = target_index;
        // Update drop indicator position
        repaint_overlay_with_indicator(drag.source_overlay, target_index);
    } else {
        // Left the tab bar vertically — switch to detach drag
        drag.reorder_target = None;
        // Existing detach/merge drag logic
    }
}
```

### `TabGroup::move_tab`

```rust
pub fn move_tab(&mut self, from: usize, to: usize) {
    if from == to || from >= self.tabs.len() || to >= self.tabs.len() {
        return;
    }

    let hwnd = self.tabs.remove(from);
    self.tabs.insert(to, hwnd);

    // Update active index to follow the active tab
    if self.active == from {
        self.active = to;
    } else if from < self.active && to >= self.active {
        self.active -= 1;
    } else if from > self.active && to <= self.active {
        self.active += 1;
    }
}
```

### Drop indicator rendering

During a reorder drag, paint a 2px-wide vertical line at the target insertion point:

```rust
fn paint_drop_indicator(bits: &mut [u32], width: i32, insert_x: i32) {
    let indicator_color = 0x00FFFFFF; // white line
    let indicator_width = 2;
    for dy in 0..TAB_HEIGHT {
        for dx in 0..indicator_width {
            let px = insert_x + dx;
            if px >= 0 && px < width {
                let idx = (dy * width + px) as usize;
                bits[idx] = premultiply_pixel(indicator_color, 255);
            }
        }
    }
}
```

### Drop handling in `on_mouse_up`

```rust
if let Some(target_idx) = drag.reorder_target {
    // Reorder within the same group
    state::with_state(|s| {
        if let Some(group) = s.groups.groups.get_mut(&drag.source_group) {
            group.move_tab(drag.source_tab, target_idx);
        }
        s.overlays.refresh_overlay(drag.source_group, &s.groups, &s.windows);
    });
} else {
    // Existing detach/merge drop logic
}
```

## Edge Cases

- **Reorder vs. detach transition**: When the user drags a tab horizontally then moves the mouse up/down out of the tab bar, the drag must seamlessly transition from reorder mode to detach mode. Clear `reorder_target` and show the normal drop preview overlay. Conversely, if the mouse re-enters the tab bar vertically, switch back to reorder mode.

- **Two-tab group**: Reordering in a two-tab group simply swaps the tabs. Ensure `move_tab(0, 1)` and `move_tab(1, 0)` both work correctly and update `active` properly.

- **Active index consistency**: After `move_tab`, the active tab's HWND must remain the same — only the index changes. Verify that `active_hwnd()` returns the same window before and after the move.

- **Drag threshold and click**: If the mouse moves less than 5px, it's a tab click (switch), not a reorder. The existing `DRAG_THRESHOLD` in `drag.rs` already handles this — reorder only activates when `dragging` becomes `true`.

- **Overlay repaint during drag**: Repainting the overlay with a drop indicator while the user drags creates a smooth animation. Use `InvalidateRect` + the existing `paint_tabs` path, passing the indicator position as an additional parameter (or store it in `OverlayData`).

- **Position store**: Tab order is not persisted to `positions.yaml`. Reordering is session-only. If persistence is desired later, extend the position store format to include tab order.

- **Config toggle**: When `allow_tab_reordering` is `false`, `on_mouse_move` should not enter reorder mode. The drag always goes to the detach/merge path.
