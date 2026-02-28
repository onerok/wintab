# Hide Tabs When Switching Virtual Desktops

## Problem

When the user switches to a different virtual desktop (Win+Ctrl+Arrow or Task View), tab overlay windows remain visible on the new desktop. Because overlays are created with `WS_EX_TOPMOST` and never explicitly hidden on a desktop transition, Windows keeps them in the global topmost layer — making them bleed through onto whichever desktop becomes active. The user sees floating tab bars above unrelated windows on a desktop where none of the grouped windows actually reside.

The root cause is that no code path reacts to `EVENT_SYSTEM_DESKTOPSWITCH`. The hook in `src/hook.rs` registers nine event ranges (create, destroy, name-change, location-change, foreground, show, hide, minimize-start, minimize-end) but omits this event entirely. When the desktop changes, `AppState` is never notified, so `OverlayManager` never hides any overlay windows.

## Location

| File | Relevance |
|------|-----------|
| `src/hook.rs:18-28` | Event registration table — `EVENT_SYSTEM_DESKTOPSWITCH` is missing |
| `src/hook.rs:78-93` | `win_event_proc` dispatch — no handler for the desktop-switch event |
| `src/state.rs:60-374` | `AppState` — needs a new `on_desktop_switch` handler |
| `src/overlay.rs:332-368` | `update_overlay` — already contains the `ShowWindow(SW_HIDE)` path used for minimized windows; the same mechanism can be reused |
| `src/overlay.rs:833-891` | `OverlayManager` — needs a method to hide all overlays whose active window is not on the current virtual desktop |

## Requirements

- [ ] When the user switches virtual desktops, all overlay windows whose associated group's active window is **not** on the newly active desktop are hidden with `ShowWindow(SW_HIDE)`.
- [ ] When the user switches back to a desktop where grouped windows reside, the overlay windows for those groups are shown and repositioned correctly.
- [ ] A grouped window that is moved to a different virtual desktop (via Task View drag or right-click "Move to") causes its overlay to be hidden on the source desktop and shown on the destination desktop when that desktop is active.
- [ ] The fix does not introduce a visible flicker — overlays should be hidden before or at the moment the new desktop appears, not after.
- [ ] Overlays for groups whose active window **is** on the current desktop continue to render normally after a desktop switch.
- [ ] The `peek` overlay (single-window hover preview) is also hidden when the target window is not on the current desktop.
- [ ] Behavior is correct on both Windows 10 (20H2+) and Windows 11.
- [ ] All 38 existing unit tests continue to pass (`just test`).

## Suggested Fix

### 1. Add the hook event

In `src/hook.rs`, extend the `events` slice with `EVENT_SYSTEM_DESKTOPSWITCH`:

```rust
(EVENT_SYSTEM_DESKTOPSWITCH, EVENT_SYSTEM_DESKTOPSWITCH),
```

The constant is already available through `windows_sys::Win32::UI::WindowsAndMessaging::*`.

In `win_event_proc`, add a dispatch arm. Note that for `EVENT_SYSTEM_DESKTOPSWITCH` the `hwnd` parameter is `NULL`, so the existing early-return guard `if hwnd.is_null() { return; }` must be relaxed or moved below the desktop-switch check:

```rust
if event == EVENT_SYSTEM_DESKTOPSWITCH {
    s.on_desktop_switch();
    return;
}
if hwnd.is_null() {
    return;
}
// ... existing arms
```

### 2. Implement `AppState::on_desktop_switch`

Add a method to `src/state.rs` that iterates every overlay and hides or shows it based on whether the group's active window is currently on the active virtual desktop:

```rust
pub fn on_desktop_switch(&mut self) {
    if !self.enabled {
        return;
    }
    self.hide_peek();

    let vdm = VirtualDesktopManager::new(); // see §3 below
    for (&gid, &ov) in &self.overlays.overlays {
        let Some(group) = self.groups.groups.get(&gid) else { continue };
        let active_hwnd = group.active_hwnd();
        let on_current = vdm.is_on_current_desktop(active_hwnd);
        unsafe {
            if on_current {
                // re-paint; update_overlay will call SWP_SHOWWINDOW internally
                overlay::update_overlay(ov, gid, &self.groups, &self.windows);
            } else {
                ShowWindow(ov, SW_HIDE);
            }
        }
    }
}
```

### 3. Wrap `IVirtualDesktopManager`

The COM interface `IVirtualDesktopManager` (shell32 / twinapi, CLSID `{aa509086-5ca9-4c25-8f95-589d3c07b48a}`) exposes `IsWindowOnCurrentVirtualDesktop(HWND) -> BOOL`. This is the only public, documented API for querying virtual desktop membership without Shell private APIs.

Because `windows-sys` does not ship COM interface bindings, the call must be made via raw vtable dispatch or a thin helper module. A minimal wrapper:

```rust
// src/vdesktop.rs
use windows_sys::Win32::System::Com::*;
use windows_sys::core::HRESULT;

// IVirtualDesktopManager vtable layout (IUnknown + 3 methods)
// IsWindowOnCurrentVirtualDesktop is the first non-IUnknown method (index 3).
// GetWindowDesktopId and MoveWindowToDesktop are at indices 4 and 5.

pub struct VirtualDesktopManager(*mut *const IVirtualDesktopManagerVtbl);

impl VirtualDesktopManager {
    pub fn new() -> Option<Self> { /* CoCreateInstance */ }
    pub fn is_on_current_desktop(&self, hwnd: HWND) -> bool { /* vtable call */ }
}

impl Drop for VirtualDesktopManager { fn drop(&mut self) { /* Release */ } }
```

Because `CoCreateInstance` and COM teardown are synchronous and do not pump messages, calling this inside `with_state()` is safe under the project's re-entrancy rules.

If `CoCreateInstance` fails (e.g., running in a restricted environment), `is_on_current_desktop` should return `true` as a safe default (show the overlay rather than wrongly hide it).

### 4. Feature-gate the COM dependency

Add the necessary `windows-sys` feature flags to `Cargo.toml` for the COM APIs needed (`Win32_System_Com`). Confirm whether `CoInitializeEx` is already called in `main.rs`; if not, it must be called (with `COINIT_APARTMENTTHREADED` to match the single-threaded message loop) before the first `CoCreateInstance`.

## Edge Cases

**NULL hwnd for `EVENT_SYSTEM_DESKTOPSWITCH`**: Unlike most accessibility events, this event fires with a `NULL` hwnd. The existing early-return guard in `win_event_proc` must be adjusted so it does not filter out this event before dispatching it.

**Window moved between desktops mid-session**: When a user drags a window to a different desktop via Task View, Windows fires `EVENT_OBJECT_LOCATIONCHANGE` but does not reliably fire `EVENT_SYSTEM_DESKTOPSWITCH`. The overlay for that group will only be corrected the next time the desktop switches. Consider also calling `is_on_current_desktop` inside `update_overlay` — if the active window is not on the current desktop, fall through to `ShowWindow(SW_HIDE)` the same way the minimized check does. This makes all overlay updates desktop-aware without requiring a dedicated event.

**Windows spanning multiple monitors but same desktop**: `IVirtualDesktopManager::IsWindowOnCurrentVirtualDesktop` only cares about virtual desktops, not monitor assignment. No special handling needed here.

**Groups with all-minimized windows**: The minimized check in `update_overlay` already returns early and calls `SW_HIDE`. The desktop-switch check should be inserted before the minimized check so that a window that is both minimized and on another desktop does not accidentally get shown when switching back to its desktop (the minimized check will still fire second and hide it correctly).

**`IVirtualDesktopManager` availability**: The interface was introduced in Windows 10 (build 10240) and is present on all supported targets for this project. No runtime version guard is needed, but the `CoCreateInstance` failure path (returning `true`) covers edge cases like sandboxed or restricted user accounts.

**Windows 11 multi-desktop taskbar**: Windows 11 can show taskbar buttons only on the desktop the window belongs to. WinTab overlays are `WS_EX_TOOLWINDOW` (excluded from the taskbar) so there is no taskbar interaction to worry about.

**Re-entrancy during `on_desktop_switch`**: `update_overlay` calls `SetWindowPos` with `SWP_SHOWWINDOW`, which can fire `EVENT_OBJECT_SHOW` synchronously with `WINEVENT_OUTOFCONTEXT`. However, because the hook uses `WINEVENT_OUTOFCONTEXT`, callbacks are always delivered via the message queue, not inline — so there is no re-entrancy risk from within `on_desktop_switch` itself.

**Performance**: `IVirtualDesktopManager::IsWindowOnCurrentVirtualDesktop` involves a cross-process COM call for each query. Instantiate `VirtualDesktopManager` once per `on_desktop_switch` call (not once per group) and reuse it across all overlay iterations in that call. Do not cache the result across separate events.
