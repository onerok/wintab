# Tab Preview on Hover

## Problem

When a group has multiple tabs, inactive tabs are completely hidden windows — `SW_HIDE` is called on all but the active one in `TabGroup::switch_to`. Users have no way to see what content an inactive tab contains without clicking on it. This creates friction when deciding which tab to switch to, especially for tabs with similar titles or generic icons (e.g. multiple browser windows, multiple terminals).

Browser-style tab bars solve this with thumbnail previews on hover. WinTab needs the same affordance: a scaled-down live preview of the hidden window shown transiently when the user pauses over an inactive tab.

## Location

**Files to modify:**

- `src/overlay.rs` — add hover delay tracking per tab, show/hide the preview window in `WM_MOUSEMOVE` and `WM_MOUSELEAVE` handlers, implement preview painting via DWM thumbnail
- `src/state.rs` — extend `AppState` to hold preview state (active DWM thumbnail handle, show-delay timer ID), expose `on_preview_timer()` handler for the delay tick

**New file:**

- `src/preview.rs` — all DWM thumbnail lifecycle logic: `register_thumbnail`, `unregister_thumbnail`, `update_thumbnail_properties`, `create_preview_window`, `destroy_preview_window`, `position_preview`

**Constants (in `src/preview.rs` or a shared config module):**

- `PREVIEW_WIDTH_DEFAULT: i32 = 300`
- `PREVIEW_OPACITY_DEFAULT: u8 = 128` (50% of 255)
- `PREVIEW_DELAY_MS: u32 = 500`

## Requirements

- [ ] Hovering over an inactive tab for at least `PREVIEW_DELAY_MS` (default 500 ms) triggers the preview. Quick passes over a tab do not show the preview.
- [ ] The preview is not shown for the currently active tab (the active window is already visible).
- [ ] The preview window is a layered, tool, no-activate, topmost popup (`WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST`) so it cannot steal focus or appear in Alt+Tab.
- [ ] Preview content is rendered using the DWM Thumbnail API (`DwmRegisterThumbnail` / `DwmUpdateThumbnailProperties`). The source window is the hidden inactive tab HWND. The destination window is the preview popup.
- [ ] Preview width defaults to 300 px. Height is calculated to preserve the source window's aspect ratio using `DWM_THUMBNAIL_PROPERTIES.rcSource` (the source client or window rect) or the `SIZE` returned by `DwmQueryThumbnailSourceSize`.
- [ ] Preview opacity defaults to 50% (`SourceConstantAlpha = 128` in `DWM_THUMBNAIL_PROPERTIES`), applied via the `opacity` field. The opacity value is configurable per-session without requiring a rebuild (read from a config struct or constants that can later be exposed through settings UI).
- [ ] The preview window is positioned directly below the hovered tab, flush with the bottom edge of the tab bar (i.e. `y = overlay_top + TAB_HEIGHT`), horizontally centered on the hovered tab's midpoint, clamped to remain within the monitor's work area.
- [ ] The preview disappears immediately when:
  - the mouse leaves the tab bar overlay (`WM_MOUSELEAVE`)
  - the mouse moves to a different tab (the old preview is hidden, delay restarts for the new tab)
  - the hovered tab becomes the active tab (user clicks it)
  - the target window is destroyed or minimized
- [ ] Only one preview is shown at a time. Creating a new preview automatically destroys the previous one.
- [ ] The DWM thumbnail handle is unregistered (`DwmUnregisterThumbnail`) when the preview is hidden or destroyed to avoid handle leaks.
- [ ] No GDI bitmap capture (`PrintWindow`, `BitBlt`) is used. The DWM live thumbnail path is the only capture method.
- [ ] The preview feature does not degrade rendering of the existing tab bar overlay — the preview window is a separate HWND from the tab bar overlay.

## Suggested Implementation

### Capture method: DWM Thumbnail API

Use `DwmRegisterThumbnail(dest_hwnd, src_hwnd, &mut hthumbnail)` where:
- `dest_hwnd` is the preview popup window
- `src_hwnd` is the inactive tab's HWND (hidden via `SW_HIDE` — DWM retains a live snapshot of hidden windows)

Call `DwmUpdateThumbnailProperties` with `DWM_THUMBNAIL_PROPERTIES`:

```
flags: DWM_TNP_RECTDESTINATION | DWM_TNP_OPACITY | DWM_TNP_VISIBLE | DWM_TNP_SOURCECLIENTAREAONLY
rcDestination: RECT { left:0, top:0, right: preview_width, bottom: preview_height }
opacity: PREVIEW_OPACITY_DEFAULT   // u8, 0–255
fVisible: TRUE
fSourceClientAreaOnly: FALSE       // include window chrome for recognizability
```

Query actual source dimensions with `DwmQueryThumbnailSourceSize(hthumbnail, &mut size)` to compute the aspect-correct `preview_height`.

Do NOT use `PrintWindow` / `BitBlt` because they require the source window to be unoccluded or cooperate with WM_PRINT, and they capture a static snapshot rather than a live-updating thumbnail.

### Preview window creation (`src/preview.rs`)

```rust
pub struct PreviewWindow {
    pub hwnd: HWND,
    pub thumbnail: HTHUMBNAIL,  // windows_sys DWM handle type
}

pub fn create_preview_window() -> HWND { ... }  // WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST, WS_POPUP
pub fn destroy_preview_window(hwnd: HWND) { ... }

pub fn show_preview(dest_hwnd: HWND, src_hwnd: HWND, tab_rect_screen: RECT) -> Option<HTHUMBNAIL> { ... }
pub fn hide_preview(dest_hwnd: HWND, thumbnail: HTHUMBNAIL) { ... }
```

### State changes (`src/state.rs`)

Extend `AppState` with:

```rust
pub struct TabPreviewState {
    pub preview_hwnd: HWND,
    pub thumbnail: HTHUMBNAIL,
    pub hovered_tab_hwnd: HWND,  // the inactive window being previewed
}

pub struct HoverDelayState {
    pub overlay_hwnd: HWND,
    pub tab_hwnd: HWND,
    pub timer_id: usize,
}
```

Add `tab_preview: Option<TabPreviewState>` and `hover_delay: Option<HoverDelayState>` to `AppState`.

The preview popup HWND is created once at startup (alongside `drop_preview`) and reused, showing/hiding as needed rather than being created and destroyed on each hover.

### Delay timer

In `overlay_wnd_proc` on `WM_MOUSEMOVE`, when hit-testing reveals a newly hovered inactive tab:

1. Cancel any pending delay timer (`KillTimer`).
2. Record the hovered tab HWND in `HoverDelayState`.
3. Start a one-shot timer: `SetTimer(overlay_hwnd, PREVIEW_TIMER_ID, PREVIEW_DELAY_MS, None)`.

On `WM_TIMER` with `PREVIEW_TIMER_ID`:

1. Kill the timer.
2. Call `state::with_state` to invoke `AppState::on_preview_timer()`.

`on_preview_timer()` checks that the hovered tab is still inactive and not minimized, then calls `show_preview`.

On `WM_MOUSELEAVE` or when the mouse moves to a different tab, call `KillTimer` and `hide_preview`.

### Positioning logic

```
tab_x_start = tab_index * tab_width          // in overlay-local coords
tab_x_mid   = tab_x_start + tab_width / 2    // midpoint of hovered tab
screen_x_mid = overlay_screen_left + tab_x_mid

preview_left = screen_x_mid - preview_width / 2
preview_top  = overlay_screen_top + TAB_HEIGHT   // just below tab bar

// Clamp to monitor work area using MonitorFromPoint + GetMonitorInfo
```

## Edge Cases

- **Hidden windows and DWM snapshots**: `DwmRegisterThumbnail` works on windows hidden via `SW_HIDE` because DWM retains a compositor snapshot. Verify this holds for windows hidden by WinTab's `TabGroup::switch_to` — it does, since DWM compositing is independent of window visibility.

- **DWM disabled**: On Windows 10/11, DWM cannot be disabled by the user (it is always on). The `DwmRegisterThumbnail` call will still fail with `E_INVALIDARG` if `src_hwnd` is not a top-level window or is invalid — handle the `HRESULT` return and fall back to hiding the preview silently.

- **High-DPI displays**: `DwmQueryThumbnailSourceSize` returns physical pixels. The overlay rect coordinates from `window::get_window_rect` use `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)` which also returns physical pixels on all DPI modes. No scaling conversion is needed as long as both sides use the same coordinate space. Verify the preview popup is created with `SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)` consistent with the rest of the process.

- **Very large source windows**: A maximized 4K window could have a source size of 3840x2160. Clamping preview width to 300 px and computing aspect-correct height gives ~169 px — acceptable. No special handling needed, but ensure `preview_height` is capped at a reasonable maximum (e.g. 400 px) to prevent tall previews on ultra-wide portrait windows.

- **Very small source windows**: If `DwmQueryThumbnailSourceSize` returns a zero or near-zero dimension (source window collapsed or in transition), skip showing the preview for that frame and retry on the next timer tick.

- **Tab switching during delay**: If the user clicks the hovered tab before `PREVIEW_DELAY_MS` elapses, the timer fires and `on_preview_timer` must check that the tab HWND is still inactive (`tab_hwnd != group.active_hwnd()`). If it is now active, do nothing.

- **Group dissolution during delay**: If the group containing the hovered tab is dissolved between delay start and timer fire, `on_preview_timer` must check that the HWND is still in a group via `groups.group_of(tab_hwnd)`. If not, discard.

- **Re-entrancy**: `on_preview_timer` is called from `WM_TIMER` dispatch inside the message loop, which runs outside any `with_state` borrow. Use `with_state` (not `try_with_state`) — it is safe here because no other borrow is held during message dispatch.

- **Preview window z-order vs overlay**: The preview popup must be placed above the grouped window's overlay. Use `HWND_TOPMOST` in `SetWindowPos` when positioning the preview. Because the overlay itself is also `HWND_TOPMOST`, call `SetWindowPos(preview_hwnd, overlay_hwnd, ...)` (relative insertion) to ensure preview renders above the tab bar, not below it.

- **Multiple monitors**: Use `MonitorFromPoint` on the overlay's top-left corner and `GetMonitorInfo` to retrieve `rcWork` before clamping the preview position. Do this at show time, not at creation time, so roaming windows are handled correctly.
