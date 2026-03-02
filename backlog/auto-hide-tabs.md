# Auto-Hide Tabs

## Problem

Tab bars are always visible on grouped windows, even when the window is maximized or snapped to the top screen edge. In these cases, top-positioned tabs extend above the screen boundary and become inaccessible. Additionally, tabs on unfocused windows add visual clutter. Auto-hiding tabs when they would be obscured or when the window is not focused improves both usability and aesthetics.

## Location

**Files modified:**

- `src/overlay.rs` — Add show/hide logic based on window state and mouse position. Implement `TrackMouseEvent` with `TME_LEAVE` for hover-based reveal. Add timer-based delay before hiding.
- `src/state.rs` — Track auto-hide state per group. Add `should_auto_hide()` evaluation method. Wire into `on_focus_changed()`.
- `src/config.rs` — Add auto-hide config fields: `auto_hide_maximized`, `auto_hide_edge`, `auto_hide_unfocused`, `auto_hide_delay_ms`.
- `src/window.rs` — Add `is_maximized()` helper (supplement existing `is_minimized()`).

**New files:** None.

## Requirements

- [ ] Tab bar auto-hides when the active window is maximized (`WS_MAXIMIZE` style).
- [ ] Tab bar auto-hides when the active window is snapped to the top screen edge (top of window touches monitor top).
- [ ] Optionally, tab bar auto-hides when the grouped window is not focused (configurable).
- [ ] Hidden tab bar reveals on mouse hover over the window's top edge (or bottom edge for bottom-positioned tabs).
- [ ] A configurable delay (default 500ms) before the tab bar hides after the mouse leaves.
- [ ] Tab bar stays visible while the mouse is over it (no premature hiding).
- [ ] Tab bar stays visible during drag operations.

## Suggested Implementation

### Config fields

```rust
pub auto_hide_maximized: bool,    // default: true
pub auto_hide_edge: bool,         // default: true
pub auto_hide_unfocused: bool,    // default: false
pub auto_hide_delay_ms: u32,      // default: 500
```

### Auto-hide state tracking

Add to `OverlayManager`:

```rust
pub struct OverlayManager {
    pub overlays: HashMap<GroupId, HWND>,
    pub desktop_hidden: HashSet<GroupId>,
    pub auto_hidden: HashSet<GroupId>,        // groups hidden by auto-hide
}
```

### Determining if auto-hide should apply

```rust
fn should_auto_hide(hwnd: HWND, config: &Config) -> bool {
    if config.auto_hide_maximized && is_maximized(hwnd) {
        return true;
    }
    if config.auto_hide_edge {
        let rect = window::get_window_rect(hwnd);
        let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        let mut mi: MONITORINFO = unsafe { std::mem::zeroed() };
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        unsafe { GetMonitorInfoW(monitor, &mut mi); }
        if rect.top <= mi.rcWork.top {
            return true;
        }
    }
    false
}

fn is_maximized(hwnd: HWND) -> bool {
    unsafe { (GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_MAXIMIZE) != 0 }
}
```

### Mouse hover reveal

Use a hot zone at the window's top edge to detect hover intent:

```rust
const AUTO_HIDE_HOT_ZONE: i32 = 4; // pixels from top edge

// In overlay_wnd_proc or via a transparent detection window:
// When mouse enters the hot zone, start a reveal timer
// When reveal timer fires, show the overlay with animation
// When mouse leaves the overlay, start a hide timer
// When hide timer fires, hide the overlay
```

### Timer-based show/hide

Use `SetTimer` on the overlay window:

```rust
const TIMER_AUTO_HIDE: usize = 100;
const TIMER_AUTO_SHOW: usize = 101;

// On mouse enter hot zone:
unsafe { SetTimer(overlay_hwnd, TIMER_AUTO_SHOW, 200, None); }

// On mouse leave overlay:
unsafe { SetTimer(overlay_hwnd, TIMER_AUTO_HIDE, config.auto_hide_delay_ms, None); }

// In WM_TIMER handler:
TIMER_AUTO_HIDE => {
    KillTimer(overlay_hwnd, TIMER_AUTO_HIDE);
    hide_overlay_animated(overlay_hwnd);
}
TIMER_AUTO_SHOW => {
    KillTimer(overlay_hwnd, TIMER_AUTO_SHOW);
    show_overlay_animated(overlay_hwnd);
}
```

### Integration with focus changes

In `on_focus_changed()`, evaluate auto-hide:

```rust
if config.auto_hide_unfocused && !is_group_focused {
    overlays.auto_hide(group_id);
} else if should_auto_hide(active_hwnd, &config) {
    overlays.auto_hide(group_id);
} else {
    overlays.auto_show(group_id);
}
```

## Edge Cases

- **Auto-hide during drag**: Do not hide tabs while a drag operation is in progress (`drag::is_dragging()`). The user needs to see tabs to complete the drag.

- **Preview popup**: If the preview popup is visible (hovering over a tab), do not hide the tab bar. Check `preview.is_active()` before starting the hide timer.

- **Mouse leave vs. child windows**: `WM_MOUSELEAVE` fires when the mouse moves to a child window (like an edit control during rename). Use `TrackMouseEvent` with `TME_LEAVE` and check if the mouse is still within the overlay bounds before hiding.

- **Multiple monitors**: A window snapped to the top of a secondary monitor should trigger edge auto-hide based on that monitor's work area, not the primary monitor.

- **Window unmaximize**: When a maximized window is restored, the overlay should re-appear immediately. Handle `EVENT_OBJECT_LOCATIONCHANGE` to re-evaluate auto-hide state.

- **Desktop switch**: Auto-hidden overlays that are also desktop-hidden should not flash when switching desktops. Check `desktop_hidden` before toggling auto-hide visibility.

- **Interaction with `suppress_events`**: During `sync_positions()` with `suppress_events` active, do not evaluate auto-hide state to avoid unnecessary overlay flickering.
