# New Tab Button

## Problem

There is no in-overlay mechanism to add windows to an existing group. Users must drag a window's tab onto the group's tab bar, which requires the source window to already have a visible peek overlay. A (+) button at the end of the tab bar provides a quick way to add ungrouped windows to the current group, either via a window picker popup or by automatically absorbing nearby eligible ungrouped windows.

## Location

**Files modified:**

- `src/overlay.rs` — Render (+) button after the last tab in `paint_tabs()`, adjust tab bar width calculation, handle hit testing for the button area, add hover highlight.
- `src/state.rs` — Add `add_tab_to_group()` method or window picker trigger.
- `src/config.rs` — Add `show_new_tab_button: bool` field to config (default: `false`).

**New files:** None initially. A window picker UI (if chosen) would warrant a `src/picker.rs` module — see the `new-tab-from-running.md` PBI for that approach.

## Requirements

- [ ] A (+) button is rendered after the last tab in the tab bar, with the same height as tabs.
- [ ] The (+) button has a hover highlight matching the tab hover style (`COLOR_HOVER`).
- [ ] Clicking the (+) button opens a popup or picker showing eligible ungrouped windows.
- [ ] Selecting a window from the picker adds it to the group as a new tab.
- [ ] The tab bar width calculation accounts for the (+) button width.
- [ ] The (+) button is hidden when `show_new_tab_button` is `false` in config.
- [ ] The (+) button does not initiate a drag — clicks are consumed immediately.

## Suggested Implementation

### Button geometry

```rust
const NEW_TAB_BTN_WIDTH: i32 = 28; // same as TAB_HEIGHT for a square button

fn new_tab_btn_rect(tabs_end_x: i32) -> RECT {
    RECT {
        left: tabs_end_x,
        top: 0,
        right: tabs_end_x + NEW_TAB_BTN_WIDTH,
        bottom: TAB_HEIGHT,
    }
}
```

### Tab bar width adjustment

In the overlay width calculation (inside `update_overlay`), add space for the button:

```rust
let total_width = tab_count * tab_width + if show_new_tab_button { NEW_TAB_BTN_WIDTH } else { 0 };
```

### Rendering in `paint_tabs()`

After the tab loop, draw the (+) button:

```rust
if show_new_tab_button {
    let btn = new_tab_btn_rect(tabs_end_x);
    let btn_color = if hover_new_tab { COLOR_HOVER } else { COLOR_INACTIVE };
    fill_rect_alpha(bits, width, &btn, btn_color, 200);
    // Draw "+" glyph: vertical and horizontal centered lines
    draw_plus_glyph(bits, width, &btn, COLOR_TEXT);
}
```

### Hit testing

Extend `hit_test_tab_ex` (or the existing `hit_test_tab`) to detect clicks on the new-tab button area:

```rust
pub enum TabHitResult {
    Tab(GroupId, usize),
    CloseButton(GroupId, usize),
    NewTabButton(GroupId),
    None,
}
```

### Click handling — simple approach

The simplest initial approach: clicking (+) shows a `TrackPopupMenu` listing eligible ungrouped windows by title:

```rust
TabHitResult::NewTabButton(gid) => {
    let ungrouped = state::with_state(|s| {
        s.windows.values()
            .filter(|w| !s.groups.window_to_group.contains_key(&w.hwnd))
            .map(|w| (w.hwnd, w.title.clone()))
            .collect::<Vec<_>>()
    });
    if ungrouped.is_empty() { return 0; }

    let menu = unsafe { CreatePopupMenu() };
    for (i, (_, title)) in ungrouped.iter().enumerate() {
        let label: Vec<u16> = format!("{}\0", title).encode_utf16().collect();
        unsafe { AppendMenuW(menu, MF_STRING, i + 1, label.as_ptr()); }
    }
    // TrackPopupMenu + handle selection → add chosen HWND to group
}
```

### Click handling — picker approach

A richer approach would open a dedicated window picker UI (see `new-tab-from-running.md`). The (+) button would invoke the same picker but pre-targeted at the clicked group.

## Edge Cases

- **No ungrouped windows**: If all managed windows are already in groups, the (+) click should show an empty-state message or do nothing. A `TrackPopupMenu` with zero items looks broken — instead show a single greyed-out "No available windows" entry.

- **Window becomes ineligible**: Between showing the popup and the user clicking, the target window may have closed. Validate the HWND with `IsWindow()` before calling `TabGroup::add()`.

- **Popup interaction with overlay**: `TrackPopupMenu` is modal and pumps its own message loop. This is safe because it runs inside `overlay_wnd_proc` which is outside `with_state()`. However, the popup will steal focus from the grouped window — call `SetForegroundWindow` on the active tab after the popup closes.

- **Overlay resize**: Adding a tab increases the tab count, which changes the overlay width. The `refresh_overlay()` call after adding the window handles this automatically.

- **Button position with many tabs**: When tabs fill the available width (capped at `MAX_TAB_WIDTH` each), the (+) button may push the overlay wider than the grouped window. Consider capping the overlay width at the window width and hiding the (+) button when space is insufficient.

- **Config toggle**: When `show_new_tab_button` is toggled from the config UI, all existing overlays must be repainted. This happens naturally through `overlays.update_all()` if the config change triggers a full refresh.
