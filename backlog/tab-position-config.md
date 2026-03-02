# Tab Position Configuration

## Problem

Tab bars are always rendered above the window's title bar. Some users prefer tabs below the title bar (or at the bottom of the window), especially when using windows snapped to the top of the screen where top-positioned tabs may be clipped. A configurable tab position (Top / Bottom) provides flexibility.

## Location

**Files modified:**

- `src/overlay.rs` — Modify `update_overlay()` to position the overlay above or below the window based on config. Adjust all coordinate calculations that assume tabs are above the window.
- `src/config.rs` — Add `tab_position: TabPosition` field (enum: Top, Bottom; default: Top).
- `src/preview.rs` — Adjust preview popup position to appear on the correct side of the tab bar.
- `src/drag.rs` — Adjust drop target detection for bottom-positioned tabs.

**New files:** None.

## Requirements

- [ ] Config option to place the tab bar at Top (default) or Bottom of the managed window.
- [ ] Top: overlay is positioned above the window's title bar (current behavior).
- [ ] Bottom: overlay is positioned below the window's bottom edge.
- [ ] Changing the position triggers an immediate reposition of all overlays (live apply).
- [ ] Hit testing, drag-and-drop, preview popups, and tooltips all work correctly in both positions.
- [ ] The overlay does not overlap the window's client area in either position.

## Suggested Implementation

### Config field

```rust
#[derive(Clone, Deserialize, Serialize)]
pub enum TabPosition {
    Top,
    Bottom,
}

// In config:
pub tab_position: TabPosition, // default: Top
```

### Overlay positioning in `update_overlay()`

Currently the overlay is positioned at:

```rust
// Current (Top):
let y = rect.top - TAB_HEIGHT;
```

Change to:

```rust
let y = match tab_position {
    TabPosition::Top => rect.top - TAB_HEIGHT,
    TabPosition::Bottom => rect.bottom,
};
```

Where `rect` is the DWM extended frame bounds of the active window.

### Preview popup positioning

In `preview.rs`, the preview appears below the tab bar. Adjust:

```rust
let preview_y = match tab_position {
    TabPosition::Top => overlay_rect.bottom,     // below the top tab bar
    TabPosition::Bottom => overlay_rect.top - preview_height, // above the bottom tab bar
};
```

### Tooltip positioning

The tooltip control uses `TTM_SETDELAYTIME` and auto-positions. For bottom tabs, set the tooltip to appear above the tab bar using `TTF_TRACK` positioning or by adjusting the tooltip's rect.

### Drop target detection

In `drag.rs`, the `find_drop_target` logic uses screen coordinates. The overlay position changes, but `WindowFromPoint` still correctly identifies the overlay regardless of position. No code change needed for basic hit testing.

### Peek overlay positioning

In `update_peek_overlay()`, the peek tab appears above the window. For bottom position, move it below the window.

## Edge Cases

- **Bottom tabs + taskbar**: If the window is near the bottom of the screen, bottom-positioned tabs may overlap with the Windows taskbar. Clamp the overlay position to the monitor work area using `MonitorFromWindow` + `GetMonitorInfoW`.

- **Top tabs + screen edge**: Top-positioned tabs on a window snapped to the top screen edge have negative Y coordinates. This is already handled by the overlay's `WS_EX_TOPMOST` style and Windows' clipping behavior, but the tabs may be partially or fully off-screen. Consider auto-hiding in this case (see `auto-hide-tabs.md`).

- **Mixed-position groups**: All tabs in a group share the same position. The config is global, not per-group.

- **Maximize/restore**: When a window is maximized, the top edge aligns with the screen edge. Top tabs would be off-screen. Bottom tabs avoid this issue. The `auto-hide-tabs.md` feature addresses this separately.

- **DWM extended frame bounds**: The overlay uses `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)` for positioning, which accounts for the invisible resize border. This works correctly for both top and bottom positioning.

- **Live apply**: When the user changes tab position in config, call `overlays.update_all()` to reposition all overlays immediately. No overlay recreation needed — just `SetWindowPos` with new coordinates.
