# Middle-Click to Close Tab

## Problem

Browser users expect middle-clicking a tab to close it. WinTab currently ignores middle mouse button events on the overlay. Adding middle-click-to-close provides a fast, keyboard-free way to close tabs that matches established browser conventions, complementing the close button (X) feature.

## Location

**Files modified:**

- `src/overlay.rs` — Handle `WM_MBUTTONUP` in `overlay_wnd_proc`, perform hit test to identify the clicked tab, dispatch close action.
- `src/state.rs` — Reuse `close_tab()` method (shared with tab close button feature).
- `src/config.rs` — Add `middle_click_close: bool` field (default: `true`).

**New files:** None.

## Requirements

- [ ] Middle-clicking (`WM_MBUTTONUP`) on a tab in the overlay sends `WM_CLOSE` to that tab's window.
- [ ] Middle-clicking on the active tab closes it and switches to an adjacent tab.
- [ ] Middle-clicking on an inactive tab closes it without switching to it first.
- [ ] Middle-clicking on empty overlay area (past the last tab) does nothing.
- [ ] The feature can be disabled via config (`middle_click_close: false`).
- [ ] Middle-click does NOT initiate a drag operation.

## Suggested Implementation

### `WM_MBUTTONUP` handler in `overlay_wnd_proc`

Add a new match arm in the overlay window procedure:

```rust
WM_MBUTTONUP => {
    let x = get_x_lparam(lparam);

    // Check config
    // let enabled = state::with_state(|s| s.config.middle_click_close);
    // if !enabled { return 0; }

    if let Some((group_id, tab_index)) = hit_test_tab(hwnd, x) {
        state::with_state(|s| s.close_tab(group_id, tab_index));
    }
    0
}
```

This is simpler than the close button because:
1. No hover state tracking needed — middle-click is immediate.
2. No rendering changes — the cursor is the only feedback.
3. No drag interaction — middle button is not used for dragging.

### Shared `close_tab` logic

The `close_tab()` method in `state.rs` is shared with the tab close button feature:

```rust
pub fn close_tab(&mut self, group_id: GroupId, tab_index: usize) {
    if let Some(group) = self.groups.groups.get(&group_id) {
        if let Some(&hwnd) = group.tabs.get(tab_index) {
            unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0); }
        }
    }
}
```

### Peek overlay middle-click

Optionally, middle-clicking the peek overlay (single-tab hover overlay) could also close that window:

```rust
// In peek_overlay_wnd_proc:
WM_MBUTTONUP => {
    let target = peek_target(hwnd);
    if !target.is_null() {
        unsafe { PostMessageW(target, WM_CLOSE, 0, 0); }
    }
    0
}
```

## Edge Cases

- **Middle-click during drag**: If a left-button drag is in progress (`drag::is_dragging()` returns true), middle-click should be ignored to avoid closing a tab mid-drag.

- **"Save changes?" dialogs**: Same as tab close button — `WM_CLOSE` is a request, not a force-close. If the window shows a dialog and the user cancels, the tab remains in the group.

- **Double middle-click**: Rapid middle-clicks on the same tab position are safe because `PostMessageW` is asynchronous. The first click triggers `WM_CLOSE`; by the time the second `WM_MBUTTONUP` arrives, the window may already be destroyed and `hit_test_tab` will return `None` for an empty position, or the tab count will have changed.

- **Middle-click on close button area**: When both features are enabled, a middle-click landing on the close button area should still close the tab (the close button is a left-click target; middle-click closes regardless of where on the tab it lands).

- **Scroll wheel**: `WM_MBUTTONDOWN` without movement followed by `WM_MBUTTONUP` is a click. Be careful not to confuse this with scroll events (`WM_MOUSEWHEEL`) which use a different message. Only handle `WM_MBUTTONUP`, not `WM_MBUTTONDOWN`, to avoid accidental closes when the user is scrolling.
