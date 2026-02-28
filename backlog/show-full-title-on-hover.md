# Show Full Title on Hover

## Problem

Tab titles are rendered with `DT_END_ELLIPSIS`, so any window whose title is
longer than the available tab width is silently truncated (e.g.
"Visual Studio Code - my-very-long-project-na…"). There is no way for the user
to discover the full title without clicking through to the window itself.
This is particularly painful in narrow-tab situations — five or more tabs on a
1080p monitor gives each tab ~200 px, which holds only about 15–20 characters
— and in the peek overlay, which renders a single tab whose width is constrained
to `MAX_TAB_WIDTH` (200 px) regardless of the window's actual width.

## Location

| File | Relevant area |
|------|---------------|
| `src/overlay.rs` | `paint_tabs()` — per-tab `DrawTextW` call with `DT_END_ELLIPSIS` (line ~504); `paint_peek_tab()` — same call (line ~292); `WM_MOUSEMOVE` / `WM_MOUSELEAVE` handlers in `overlay_wnd_proc`; `OverlayData` struct; `set_hover_tab()` |
| `src/state.rs` | `AppState` — tooltip HWND would be stored here if it lives globally, or it can be per-overlay inside `OverlayData` |
| `src/window.rs` | `WindowInfo.title` — the full title string that must be displayed |

## Requirements

- [ ] When the mouse rests over a tab for at least 500 ms, a tooltip appears
      showing the complete, untruncated window title.
- [ ] The tooltip is only shown when the title is actually truncated; if the
      full title fits within the rendered tab width no tooltip appears.
- [ ] The tooltip appears near the hovered tab (directly below or above the tab
      bar, clear of the 28 px overlay strip).
- [ ] The tooltip disappears immediately when the mouse moves to a different tab
      or leaves the overlay entirely (`WM_MOUSELEAVE`).
- [ ] The tooltip disappears when the overlay is hidden (minimise, disable,
      group dissolved).
- [ ] Tooltip text updates live: if the window title changes while the tooltip
      is visible it reflects the new title.
- [ ] The tooltip does not steal focus or interfere with click / drag handling.
- [ ] The feature works for both the regular grouped tab overlay and the peek
      overlay (single-tab bar above unmanaged windows).

## Suggested Implementation

### Option A — Win32 `TOOLTIPS_CLASS` control (recommended)

Create one `TOOLTIPS_CLASS` window per overlay HWND, stored in `OverlayData`:

```rust
struct OverlayData {
    group_id: GroupId,
    hover_tab: i32,
    peek_hwnd: HWND,
    tooltip_hwnd: HWND,   // new field
    tooltip_tab: i32,     // tab index the tooltip is currently showing, -1 = none
}
```

**Creation** — in `create_overlay()` and `create_peek_overlay()`, after the
overlay window is created:

```rust
use windows_sys::Win32::UI::Controls::TOOLTIPS_CLASS;

let tt = CreateWindowExW(
    0,
    TOOLTIPS_CLASS_UTF16.as_ptr(),   // "tooltips_class32\0" encoded as UTF-16
    std::ptr::null(),
    WS_POPUP | TTS_NOPREFIX | TTS_ALWAYSTIP,
    CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT,
    hwnd,           // parent = the overlay
    0 as _, instance, std::ptr::null(),
);
// Set the maximum tooltip width so long titles word-wrap at ~400 px
SendMessageW(tt, TTM_SETMAXTIPWIDTH, 0, 400);
// Register the entire overlay client area as the tool region
let mut ti = TOOLINFOW {
    cbSize: std::mem::size_of::<TOOLINFOW>() as u32,
    uFlags: TTF_SUBCLASS | TTF_IDISHWND,
    hwnd,
    uId: hwnd as usize,
    rect: RECT { left: 0, top: 0, right: 0, bottom: 0 },
    hinst: 0 as _,
    lpszText: LPSTR_TEXTCALLBACKW,   // we supply text on TTN_NEEDTEXT
    lParam: 0,
    lpReserved: std::ptr::null_mut(),
};
SendMessageW(tt, TTM_ADDTOOLW, 0, &ti as *const _ as isize);
SendMessageW(tt, TTM_SETDELAYTIME, TTDT_INITIAL as usize, 500); // 500 ms show delay
SendMessageW(tt, TTM_SETDELAYTIME, TTDT_AUTOPOP as usize, 8000); // 8 s auto-hide
SendMessageW(tt, TTM_SETDELAYTIME, TTDT_RESHOW as usize, 100);
```

**Text supply** — handle `WM_NOTIFY` with `TTN_NEEDTEXTW` in
`overlay_wnd_proc`:

```rust
WM_NOTIFY => {
    let hdr = &*(lparam as *const NMHDR);
    if hdr.code == TTN_NEEDTEXTW {
        let nmtt = &mut *(lparam as *mut NMTTDISPINFOW);
        // Determine which tab is hovered from OverlayData.hover_tab
        // then read the full title from AppState.windows
        // Write a pointer to a UTF-16 string into nmtt.lpszText or nmtt.szText
        // Only supply text if the title is truncated (measure with GetTextExtentPoint32W)
        fill_tooltip_text(hwnd, nmtt);
    }
    0
}
```

**Truncation detection** — inside `fill_tooltip_text`, use
`GetTextExtentPoint32W` on a temporary `HDC` (or the cached one from the last
paint) to measure whether the rendered title string fits inside
`tab_width - 2 * TAB_PADDING - ICON_SIZE - 4` pixels. If it fits, set
`lpszText` to `""` or `LPSTR_TEXTCALLBACKW` returning empty, suppressing the
tooltip.

**Tear-down** — `destroy_overlay()` already calls `DestroyWindow(hwnd)`;
because the tooltip is a child of the overlay it is destroyed automatically.
No additional cleanup is needed.

### Option B — Custom rendered tooltip (alternative)

Create a second `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE` popup
rendered with the same GDI pipeline already used for the tab bar. Position it
with `SetWindowPos` on a 500 ms `WM_TIMER` started in `WM_MOUSEMOVE` when
hover changes. Measure text with `GetTextExtentPoint32W` to size the window.
Destroy or hide it on `WM_MOUSELEAVE` or hover-tab change.

This option gives full visual control (matching the tab bar color scheme) but
requires more code and careful timer management to avoid conflicts with the
existing peek timer in `AppState`.

**Recommendation**: Use Option A (`TOOLTIPS_CLASS`) for minimal code surface
and correct OS-provided delay/dismiss behavior. The visual style mismatch
(system tooltip vs. custom tab bar) is acceptable. If visual consistency
becomes a requirement later, migrate to Option B.

### Threading / borrow safety note

`TTN_NEEDTEXTW` is dispatched synchronously inside `SendMessageW` called by
the tooltip control; it arrives while the message loop is idle (no active
`with_state()` borrow), so reading `AppState` from the `WM_NOTIFY` handler via
`try_with_state()` is safe.

## Edge Cases

- **Title fits — no tooltip**: measure text before showing; skip the tooltip
  entirely when `text_width <= available_width` to avoid redundant UI noise.
- **Title changes while tooltip is visible**: re-query the title in
  `TTN_NEEDTEXTW` every time it fires; the tooltip control calls it on every
  show. The existing `on_title_changed` path in `state.rs` already repaints
  the tab bar; it should also call `TTM_UPDATE` on the overlay's tooltip HWND
  if `hover_tab` matches the changed window's tab index.
- **Peek overlay (single tab, variable width)**: the peek tab width is
  `clamp(window_width, MIN_TAB_WIDTH, MAX_TAB_WIDTH)` = at most 200 px, so
  almost all titles will be truncated; the tooltip should fire here too.
- **Many tabs, very narrow tabs**: at `MIN_TAB_WIDTH` (40 px) even short titles
  truncate. The tooltip rect registered with `TTM_ADDTOOLW` covers the whole
  overlay strip; map the cursor X to a tab index in `TTN_NEEDTEXTW` using the
  existing `calculate_tab_index()` helper to select the right title.
- **Overlay hidden (`SW_HIDE`)**: the tooltip control is a child window and is
  hidden automatically. No extra work needed.
- **DPI / high-DPI monitors**: `GetTextExtentPoint32W` returns values in
  logical pixels that match `DrawTextW`, so the truncation check is consistent
  as long as both use the same `HDC`/font. Avoid mixing physical and logical
  pixel measurements.
- **Re-entrancy**: `WM_NOTIFY` must not call `with_state()` — use
  `try_with_state()` and treat a failed borrow as "no text" to stay safe.
- **Tooltip z-order**: call `SetWindowPos(tt, HWND_TOPMOST, ...)` after
  creation so the tooltip floats above the already-`WS_EX_TOPMOST` overlay;
  without this the tooltip can be hidden behind the overlay strip itself.
- **Drag in progress**: suppress the tooltip during active drag operations
  (check `crate::drag::is_dragging()` in `TTN_NEEDTEXTW` and return empty text)
  to avoid a tooltip appearing mid-drag.
