# Drag File Over Tab to Switch

## Problem

When dragging a file from Explorer to a grouped window, the user cannot switch to a hidden tab without releasing the file first. Browser-style "drag file over tab to switch" enables dragging a file over an inactive tab to auto-switch after a short delay, allowing the file to be dropped onto the now-visible window.

## Location

**Files modified:**

- `src/overlay.rs` — Register overlay windows as OLE drop targets. Implement `IDropTarget` interface. Add timer-based tab switch on hover during external drag.
- `src/state.rs` — Wire tab switch logic for external drag events.
- `src/config.rs` — Add `drag_file_switch: bool` (default: `true`) and `drag_file_switch_delay_ms: u32` (default: `500`).

**New files:** None.

## Requirements

- [ ] When a file (or other OLE drag object) is dragged over an inactive tab, that tab becomes active after a configurable delay (default 500ms).
- [ ] The delay resets if the cursor moves to a different tab.
- [ ] Moving the cursor off the tab bar cancels the pending switch.
- [ ] After the switch, the file can be dropped onto the now-visible window.
- [ ] The feature does not interfere with WinTab's own tab drag-and-drop (which uses `SetCapture`, not OLE drag-and-drop).
- [ ] Can be disabled via config (`drag_file_switch: false`).

## Suggested Implementation

### OLE Drag-and-Drop architecture

WinTab's own tab dragging uses `SetCapture`/`ReleaseCapture` (custom drag in `drag.rs`). File dragging from Explorer uses OLE drag-and-drop (`IDropSource`/`IDropTarget`). These are completely independent systems.

To detect file drags over the overlay, register the overlay as an OLE drop target:

```rust
use windows_sys::Win32::System::Ole::RegisterDragDrop;
use windows_sys::Win32::System::Com::IDropTarget;

// After creating each overlay window:
let drop_target = create_drop_target(overlay_hwnd);
unsafe { RegisterDragDrop(overlay_hwnd, drop_target); }
```

### IDropTarget implementation

Implement a minimal `IDropTarget` COM object:

```rust
struct TabDropTarget {
    ref_count: u32,
    overlay_hwnd: HWND,
    hover_tab: Option<usize>,
    timer_active: bool,
}

impl IDropTarget for TabDropTarget {
    fn DragEnter(&mut self, _data: *mut IDataObject, _key_state: u32,
                 pt: POINTL, effect: *mut u32) -> HRESULT {
        // Don't accept the drop — we just want to detect the hover
        unsafe { *effect = DROPEFFECT_NONE; }
        self.update_hover(pt);
        S_OK
    }

    fn DragOver(&mut self, _key_state: u32, pt: POINTL, effect: *mut u32) -> HRESULT {
        unsafe { *effect = DROPEFFECT_NONE; }
        self.update_hover(pt);
        S_OK
    }

    fn DragLeave(&mut self) -> HRESULT {
        self.cancel_switch();
        S_OK
    }

    fn Drop(&mut self, _data: *mut IDataObject, _key_state: u32,
            _pt: POINTL, effect: *mut u32) -> HRESULT {
        // Reject the drop — let it fall through to the window below
        unsafe { *effect = DROPEFFECT_NONE; }
        DRAGDROP_S_CANCEL
    }
}
```

### Hover-based tab switch

```rust
impl TabDropTarget {
    fn update_hover(&mut self, pt: POINTL) {
        let local_x = pt.x - overlay_rect.left;
        let tab_idx = calculate_tab_index(local_x, overlay_width, tab_count);

        if tab_idx != self.hover_tab {
            self.cancel_switch();
            self.hover_tab = tab_idx;
            if let Some(idx) = tab_idx {
                // Start timer for delayed switch
                unsafe {
                    SetTimer(self.overlay_hwnd, TIMER_FILE_DRAG_SWITCH, delay_ms, None);
                }
                self.timer_active = true;
            }
        }
    }

    fn cancel_switch(&mut self) {
        if self.timer_active {
            unsafe { KillTimer(self.overlay_hwnd, TIMER_FILE_DRAG_SWITCH); }
            self.timer_active = false;
        }
        self.hover_tab = None;
    }
}
```

### Timer handler in `overlay_wnd_proc`

```rust
TIMER_FILE_DRAG_SWITCH => {
    KillTimer(hwnd, TIMER_FILE_DRAG_SWITCH);
    // Switch to the hovered tab
    if let Some((group_id, tab_index)) = get_file_drag_hover_target(hwnd) {
        state::try_with_state(|s| {
            if let Some(group) = s.groups.groups.get_mut(&group_id) {
                group.switch_to(tab_index);
            }
            s.overlays.refresh_overlay(group_id, &s.groups, &s.windows);
        });
    }
    0
}
```

### COM initialization

`CoInitializeEx` is already called in `main()`. OLE drag-and-drop requires COM to be initialized on the thread, which is already the case.

## Edge Cases

- **Drop rejection**: The overlay's `IDropTarget::Drop` returns `DRAGDROP_S_CANCEL` to reject the drop. This causes the drag to continue — the file "falls through" to the window underneath. However, some drag sources may not handle this gracefully. An alternative is to return `DROPEFFECT_NONE` and let the user re-drop.

- **Overlay `WS_EX_TRANSPARENT`**: If the overlay has `WS_EX_TRANSPARENT`, OLE drag-and-drop may not detect it as a target. Overlays currently do NOT use `WS_EX_TRANSPARENT`, so this should work.

- **Tab switch during drag**: `TabGroup::switch_to()` calls `ShowWindow` and `SetForegroundWindow`, which may interfere with the ongoing OLE drag. Test that the drag continues after the tab switch.

- **Interaction with WinTab tab drag**: WinTab's own tab drag uses `SetCapture`, not OLE. If a tab drag is in progress (`drag::is_dragging()`), ignore `IDropTarget` callbacks.

- **COM ref counting**: The `IDropTarget` implementation must properly handle `AddRef`/`Release`. Use a simple ref-counted wrapper to prevent premature deallocation.

- **RevokeDragDrop on cleanup**: Call `RevokeDragDrop(overlay_hwnd)` when destroying the overlay to prevent dangling references.
