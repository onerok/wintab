# Tab Close Button

## Problem

Users cannot close a tab directly from the tab bar. The only way to close a grouped window is via the window's own title bar close button (which requires switching to that tab first) or Alt+F4. A close button (X) on each tab provides a familiar, discoverable way to close any tab without first switching to it, matching browser UX conventions.

## Location

**Files modified:**

- `src/overlay.rs` — Add close button rendering in `paint_tabs()`, extend hit testing in `hit_test_tab()` / `calculate_tab_index()` to distinguish close-button clicks from tab clicks, handle close-button hover highlight.
- `src/state.rs` — Add `close_tab()` method that sends `WM_CLOSE` to the target window and handles group membership cleanup.
- `src/config.rs` — Add `show_close_button: bool` field to config (default: `true`).

**New files:** None.

## Requirements

- [ ] Each tab in the tab bar displays a small X button on the right side, inside the tab area.
- [ ] The X button is only visible when the mouse hovers over the specific tab (not the entire tab bar).
- [ ] Clicking the X button sends `WM_CLOSE` to the corresponding window's HWND.
- [ ] The X button has a distinct hover state (lighter/highlighted background) when the mouse is directly over it.
- [ ] Clicking the X button does NOT switch to that tab — it closes it directly.
- [ ] When the active tab is closed via X, the group switches to an adjacent tab (prefer left, fallback right).
- [ ] When the last tab in a group is closed, the group dissolves (handled by existing `TabGroup::remove` logic).
- [ ] The close button can be disabled via config (`show_close_button: false`), hiding it from all tabs.
- [ ] The X button is rendered at the correct DPI scale.

## Suggested Implementation

### Close button geometry

Define close button constants alongside existing tab constants in `overlay.rs`:

```rust
const CLOSE_BTN_SIZE: i32 = 14;       // clickable area
const CLOSE_ICON_SIZE: i32 = 8;       // X glyph size within the button
const CLOSE_BTN_MARGIN_RIGHT: i32 = 4; // gap from tab right edge
```

The close button is positioned inside each tab:

```rust
fn close_btn_rect(tab_x: i32, tab_width: i32) -> RECT {
    let right = tab_x + tab_width - CLOSE_BTN_MARGIN_RIGHT;
    let left = right - CLOSE_BTN_SIZE;
    let top = (TAB_HEIGHT - CLOSE_BTN_SIZE) / 2;
    let bottom = top + CLOSE_BTN_SIZE;
    RECT { left, top, right, bottom }
}
```

### Rendering in `paint_tabs()`

After drawing the tab icon and title text, draw the close button when the tab is hovered:

```rust
// In the per-tab rendering loop, after text drawing:
if show_close_button && hover_tab == i as i32 {
    let btn = close_btn_rect(tab_x, tab_width);
    let btn_color = if hover_close { COLOR_HOVER } else { COLOR_INACTIVE };
    fill_rect_alpha(bits, width, &btn, btn_color, 200);
    // Draw X glyph as two diagonal lines using MoveToEx/LineTo
    // or manually set pixels for a crisp 8x8 X pattern
    draw_close_glyph(bits, width, &btn, COLOR_TEXT);
}
```

When the close button is visible, reduce the available text width by `CLOSE_BTN_SIZE + CLOSE_BTN_MARGIN_RIGHT` to prevent title text from overlapping the X.

### Extended hit testing

Add a `HitTestResult` enum or extend the current `hit_test_tab` return type:

```rust
pub enum TabHitResult {
    Tab(GroupId, usize),          // clicked on tab body
    CloseButton(GroupId, usize),  // clicked on close button
    None,
}

pub fn hit_test_tab_ex(overlay_hwnd: HWND, x: i32, y: i32) -> TabHitResult {
    // First check which tab index the x coordinate falls in
    // Then check if (x, y) is within close_btn_rect for that tab
    // If show_close_button is false, always return Tab variant
}
```

### Mouse event handling in `overlay_wnd_proc`

In the `WM_LBUTTONDOWN` handler, use `hit_test_tab_ex` instead of `hit_test_tab`:

```rust
WM_LBUTTONDOWN => {
    let x = get_x_lparam(lparam);
    let y = get_y_lparam(lparam);
    match hit_test_tab_ex(hwnd, x, y) {
        TabHitResult::Tab(gid, idx) => {
            drag::on_mouse_down(hwnd, gid, idx, x, y);
        }
        TabHitResult::CloseButton(gid, idx) => {
            // Close directly, don't initiate drag
            state::with_state(|s| s.close_tab(gid, idx));
        }
        TabHitResult::None => {}
    }
    0
}
```

### Close button hover tracking

Track `hover_close: bool` in `OverlayData` alongside `hover_tab: i32`. Update in `WM_MOUSEMOVE`:

```rust
let hit = hit_test_tab_ex(hwnd, x, y);
let new_hover_close = matches!(hit, TabHitResult::CloseButton(..));
// If hover_close changed, trigger repaint
```

### `close_tab` in `state.rs`

```rust
pub fn close_tab(&mut self, group_id: GroupId, tab_index: usize) {
    if let Some(group) = self.groups.groups.get(&group_id) {
        if let Some(&hwnd) = group.tabs.get(tab_index) {
            // Send WM_CLOSE — the window's destroy event will trigger
            // on_window_destroyed → remove from group naturally
            unsafe {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
    }
}
```

Use `PostMessageW` rather than `SendMessageW` to avoid blocking the message loop if the target application shows a "Save changes?" dialog.

## Edge Cases

- **"Save changes?" dialogs**: The target window may intercept `WM_CLOSE` and show a confirmation dialog (e.g., unsaved Notepad). WinTab should not force-close — if the window survives `WM_CLOSE`, it remains in the group. The `EVENT_OBJECT_DESTROY` hook will fire only if the window actually closes.

- **Close button vs. drag threshold**: Clicking the close button must NOT initiate a drag. The `WM_LBUTTONDOWN` handler must check `hit_test_tab_ex` and only call `drag::on_mouse_down` for `TabHitResult::Tab`, not `TabHitResult::CloseButton`.

- **Narrow tabs**: When a tab is narrower than `MIN_TAB_WIDTH`, there may not be enough space for both icon + text + close button. Hide the close button (or the icon) when `tab_width < CLOSE_BTN_SIZE + ICON_SIZE + TAB_PADDING * 3`.

- **Close active tab**: When the currently active tab is closed, `TabGroup::remove` already handles switching to an adjacent tab and showing it. No special case needed — the existing logic adjusts `active` index correctly.

- **Rapid clicks**: If the user clicks X on multiple tabs quickly, each `PostMessageW(WM_CLOSE)` is asynchronous. The group state updates when each `EVENT_OBJECT_DESTROY` fires, which happens in sequence through the message loop. No race condition since everything is single-threaded.

- **DPI scaling**: The close button size should be scaled by the overlay window's DPI. Use `window::get_window_dpi()` to compute `CLOSE_BTN_SIZE * dpi / 96`.

- **Config toggle**: When `show_close_button` is `false`, `hit_test_tab_ex` should never return `CloseButton`, and `paint_tabs` should not render the X. The text area reclaims the space used by the button.
