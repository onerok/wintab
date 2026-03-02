# Prevent Tabs Hidden by Overlapping Windows

## Problem

Although overlays use `WS_EX_TOPMOST`, other `TOPMOST` windows (e.g., media players in "always on top" mode, other utility overlays) can still obscure the tab bar. Additionally, when rapidly switching between applications, z-order changes may temporarily place a non-topmost window above the overlay. Users need an option to aggressively enforce the overlay's z-order position.

## Location

**Files modified:**

- `src/overlay.rs` — Add periodic z-order enforcement via timer. Add `EVENT_OBJECT_REORDER` detection. Implement `bring_overlay_to_top()` method.
- `src/state.rs` — Wire z-order checks into focus change events and periodic timer.
- `src/hook.rs` — Optionally add `EVENT_OBJECT_REORDER` hook for z-order change notifications.
- `src/config.rs` — Add `prevent_hidden_tabs: bool` field (default: `false`).

**New files:** None.

## Requirements

- [ ] When enabled, the overlay maintains its position above all other windows (including other TOPMOST windows).
- [ ] Z-order is re-enforced when focus changes or window reorder events fire.
- [ ] A periodic timer (e.g., every 500ms) rechecks z-order as a fallback.
- [ ] The feature is off by default to avoid conflicts with other always-on-top applications.
- [ ] The overlay does not steal focus when re-asserting z-order (preserves `WS_EX_NOACTIVATE`).

## Suggested Implementation

### Z-order enforcement

```rust
fn bring_overlay_to_top(overlay_hwnd: HWND) {
    unsafe {
        SetWindowPos(
            overlay_hwnd,
            HWND_TOPMOST,
            0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}
```

### Detecting occlusion

Check if the overlay is obscured by another window:

```rust
fn is_overlay_obscured(overlay_hwnd: HWND) -> bool {
    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(overlay_hwnd, &mut rect);

        // Check center point of the overlay
        let pt = POINT {
            x: (rect.left + rect.right) / 2,
            y: (rect.top + rect.bottom) / 2,
        };
        let top_hwnd = WindowFromPoint(pt);

        // If the topmost window at the center isn't our overlay, we're obscured
        top_hwnd != overlay_hwnd && !top_hwnd.is_null()
    }
}
```

### Periodic timer

In `update_overlay()` or `OverlayManager::ensure_overlay()`, set a timer:

```rust
const TIMER_ZORDER_CHECK: usize = 200;
const ZORDER_CHECK_INTERVAL: u32 = 500; // ms

unsafe {
    SetTimer(overlay_hwnd, TIMER_ZORDER_CHECK, ZORDER_CHECK_INTERVAL, None);
}

// In overlay_wnd_proc WM_TIMER handler:
TIMER_ZORDER_CHECK => {
    if config.prevent_hidden_tabs && is_overlay_obscured(hwnd) {
        bring_overlay_to_top(hwnd);
    }
    0
}
```

### Hook-based detection

Optionally add `EVENT_OBJECT_REORDER` to the hooks in `hook.rs`:

```rust
// In register_hooks():
SetWinEventHook(
    EVENT_OBJECT_REORDER, EVENT_OBJECT_REORDER,
    0 as _, Some(win_event_proc),
    0, 0,
    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
);
```

On `EVENT_OBJECT_REORDER`, re-assert z-order for all visible overlays:

```rust
fn on_zorder_changed() {
    with_state(|s| {
        if s.config.prevent_hidden_tabs {
            for &overlay in s.overlays.overlays.values() {
                bring_overlay_to_top(overlay);
            }
        }
    });
}
```

## Edge Cases

- **Z-order wars**: If two applications both aggressively enforce TOPMOST, they will fight for the top position, causing flickering. The periodic timer approach limits the frequency of z-order assertions, but cannot fully prevent this.

- **Other WinTab overlays**: WinTab's own overlays may overlap each other (multiple groups near the same screen position). `bring_overlay_to_top` should only affect overlays that are actually obscured by non-WinTab windows.

- **Performance**: `WindowFromPoint` is cheap, but calling it every 500ms for every overlay adds up with many groups. Consider only checking the focused group's overlay, or using `EVENT_OBJECT_REORDER` as the primary trigger and the timer as a fallback.

- **Fullscreen applications**: When a fullscreen application (game, media player) is active, overlays should not assert z-order above it. Check `is_foreground_fullscreen()` and skip z-order enforcement.

- **Desktop switch**: When overlays are hidden due to a desktop switch (`desktop_hidden`), do not enforce z-order. The overlay is intentionally hidden.

- **`SetWindowPos` re-entrancy**: `SetWindowPos` with `SWP_NOACTIVATE` should not trigger recursive z-order events, but verify that `EVENT_OBJECT_REORDER` from our own `SetWindowPos` call is filtered by `WINEVENT_SKIPOWNPROCESS`.

- **Config toggle**: When `prevent_hidden_tabs` is toggled off, kill the periodic timer on all overlays to stop unnecessary z-order checks.
