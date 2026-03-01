# Feature Implementation Rules

## Win32 Event Safety

- `switch_to()` calls `ShowWindow` + `SetForegroundWindow` on external windows, which queues `EVENT_SYSTEM_FOREGROUND`, `EVENT_OBJECT_LOCATIONCHANGE`, `EVENT_OBJECT_SHOW` etc. via hooks
- Hooks use `WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS` — events are posted to the message queue, not delivered synchronously, and only for other-process windows
- Never assume COM APIs return fresh data after `ShowWindow` transitions — `IsWindowOnCurrentVirtualDesktop` can lag behind actual window state
- The foreground window is always on the current desktop — use this as ground truth over COM queries

## RefCell Re-entrancy

- `with_state()` borrows `AppState` mutably. Any Win32 call that pumps messages (e.g., `SendMessage`, modal dialogs) while holding this borrow will panic
- `try_with_state()` / `try_with_state_ret()` exist for code called from wndproc callbacks where `with_state()` may already be borrowed
- Overlay wndproc handlers (`overlay_wnd_proc`) run outside `with_state` — they must use `update_overlay_standalone()` or `try_with_state()` to access state
- The `suppress_events` flag prevents recursive event handling during `sync_positions()` and `minimize_all()`

## Overlay Windows

- Style: `WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST`
- Rendered via 32-bit ARGB DIB + `UpdateLayeredWindow` with `ULW_ALPHA`
- GDI text/icon rendering zeros the alpha channel — `fix_gdi_alpha()` patches it back to 0xFF for non-zero RGB pixels
- Overlays are per-group, managed by `OverlayManager`, created lazily via `ensure_overlay()`

## Tab Switching

- `TabGroup::switch_to(index)` does: get old rect → hide old → position+show new → `SetForegroundWindow(new)`
- After switch, `EVENT_SYSTEM_FOREGROUND` fires → `on_focus_changed()` → `sync_desktop_visibility()` — must not re-hide the just-shown overlay
- `on_focus_changed` checks `idx != group.active` to avoid redundant re-switches

## Virtual Desktop Integration

- `sync_desktop_visibility()` is called from `on_focus_changed()` as a fallback since `EVENT_SYSTEM_DESKTOPSWITCH` is unreliable via `WINEVENT_OUTOFCONTEXT`
- Groups with overlays hidden due to desktop switch are tracked in `OverlayManager::desktop_hidden`
- `on_desktop_switch()` and `sync_desktop_visibility()` both check active window only (not all tabs) due to COM inconsistency with hidden windows

## Drag-and-Drop

- 5px threshold before drag activates (prevents accidental drags on click)
- Non-drag mouse up = tab click → `switch_to()` in `on_mouse_up`
- Drop targets: overlay (merge), managed window (group or add), empty space (detach)
- `SetCapture` / `ReleaseCapture` used to track mouse outside overlay bounds during drag
