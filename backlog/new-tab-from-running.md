# New Tab from Running Windows (Ctrl+Win+T)

## Problem

Adding a window to a group requires dragging its tab onto the group. For windows that are behind other windows or on different parts of the screen, this is awkward. A keyboard-triggered window picker (`Ctrl+Win+T`) shows a list of eligible ungrouped windows and lets the user select one to add to the focused group, without any drag-and-drop.

## Location

**Files modified:**

- `src/state.rs` — Add `show_window_picker()` method and `add_window_to_focused_group()`.
- `src/main.rs` — Register `Ctrl+Win+T` global hotkey, handle `WM_HOTKEY`.

**New files:**

- `src/picker.rs` — Window picker UI: custom popup listing eligible ungrouped windows with icons and titles, keyboard navigation, mouse selection.

## Requirements

- [ ] `Ctrl+Win+T` (configurable) opens a window picker popup.
- [ ] The picker lists all eligible ungrouped windows, each showing the window's icon and title.
- [ ] Selecting a window adds it to the currently focused group.
- [ ] If no group is focused (foreground window is ungrouped), the selected window creates a new group with the foreground window.
- [ ] The picker supports keyboard navigation: Up/Down to move, Enter to select, Escape to cancel.
- [ ] The picker supports mouse click selection.
- [ ] The picker closes after selection or on Escape / focus loss.
- [ ] The picker is centered on the screen or positioned near the focused window.

## Suggested Implementation

### Hotkey registration

```rust
// In main.rs startup or hotkey.rs:
const HOTKEY_NEW_TAB: i32 = 5100;
unsafe {
    RegisterHotKey(msg_hwnd, HOTKEY_NEW_TAB, MOD_CONTROL | MOD_WIN, 'T' as u32);
}

// In WM_HOTKEY handler:
HOTKEY_NEW_TAB => {
    state::with_state(|s| s.show_window_picker());
}
```

### Picker window

Create a custom popup window listing available windows:

```rust
pub struct PickerState {
    pub hwnd: HWND,
    pub items: Vec<PickerItem>,
    pub selected: usize,
    pub target_group: Option<GroupId>,
}

pub struct PickerItem {
    pub hwnd: HWND,
    pub title: String,
    pub icon: HICON,
    pub process_name: String,
}

pub fn show_picker(target_group: Option<GroupId>) {
    let items = enumerate_eligible_ungrouped();
    if items.is_empty() { return; }

    let hwnd = create_picker_window(&items);
    // Store state for the picker window proc
}
```

### Picker window creation

```rust
fn create_picker_window(items: &[PickerItem]) -> HWND {
    let item_height = 32; // icon (16) + padding
    let width = 350;
    let height = (items.len() as i32 * item_height).min(400); // cap at 400px

    // Center on screen or near the focused window
    let (x, y) = center_position(width, height);

    unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            picker_class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_VISIBLE | WS_BORDER,
            x, y, width, height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    }
}
```

### Rendering items

```rust
fn paint_picker(hwnd: HWND, items: &[PickerItem], selected: usize) {
    // BeginPaint / EndPaint
    for (i, item) in items.iter().enumerate() {
        let y = i as i32 * ITEM_HEIGHT;
        let bg = if i == selected { COLOR_HIGHLIGHT } else { COLOR_WINDOW };

        // Fill background
        // Draw icon at (4, y + 8) using DrawIconEx
        // Draw title text at (24, y + 4) using DrawTextW
        // Draw process name in smaller/dimmer text at (24, y + 18)
    }
}
```

### Keyboard handling

```rust
fn picker_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_KEYDOWN => {
            match wparam as u32 {
                VK_UP => { move_selection(-1); InvalidateRect(hwnd, ..); 0 }
                VK_DOWN => { move_selection(1); InvalidateRect(hwnd, ..); 0 }
                VK_RETURN => { confirm_selection(); DestroyWindow(hwnd); 0 }
                VK_ESCAPE => { DestroyWindow(hwnd); 0 }
                _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
        }
        WM_LBUTTONUP => {
            let y = get_y_lparam(lparam);
            let idx = y / ITEM_HEIGHT;
            select_and_confirm(idx as usize);
            unsafe { DestroyWindow(hwnd); }
            0
        }
        WM_KILLFOCUS => {
            unsafe { DestroyWindow(hwnd); }
            0
        }
        WM_PAINT => { paint_picker(hwnd, ...); 0 }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}
```

### Selection confirmation

```rust
fn confirm_selection(item: &PickerItem, target_group: Option<GroupId>) {
    state::with_state(|s| {
        match target_group {
            Some(gid) => {
                // Add to existing group
                if let Some(group) = s.groups.groups.get_mut(&gid) {
                    group.add(item.hwnd);
                    s.groups.window_to_group.insert(item.hwnd, gid);
                    s.overlays.refresh_overlay(gid, &s.groups, &s.windows);
                }
            }
            None => {
                // Create new group with foreground + selected window
                let fg = unsafe { GetForegroundWindow() };
                if fg != item.hwnd && s.windows.contains_key(&fg) {
                    s.create_group(fg, item.hwnd);
                }
            }
        }
    });
}
```

### Enumerating eligible ungrouped windows

```rust
fn enumerate_eligible_ungrouped() -> Vec<PickerItem> {
    state::with_state(|s| {
        s.windows.values()
            .filter(|w| !s.groups.window_to_group.contains_key(&w.hwnd))
            .filter(|w| window::is_window_valid(w.hwnd))
            .map(|w| PickerItem {
                hwnd: w.hwnd,
                title: w.title.clone(),
                icon: w.icon,
                process_name: w.process_name.clone(),
            })
            .collect()
    })
}
```

## Edge Cases

- **No ungrouped windows**: If all windows are in groups, show a brief tooltip or balloon "No available windows" and skip showing the picker.

- **No focused group**: If the foreground window is not in a group, the picker creates a new 2-tab group. If the foreground window is not managed by WinTab at all (e.g., Explorer, desktop), cancel silently.

- **Picker focus**: The picker must receive keyboard focus to handle Up/Down/Enter. Call `SetFocus(picker_hwnd)` after creation. Since it's `WS_EX_TOPMOST`, it should render above other windows.

- **Window closed during picker**: A window listed in the picker may close before the user selects it. Validate with `IsWindow()` before adding to a group.

- **Picker and overlays**: The picker is a separate window class (`WinTabPicker`). It should be excluded from `is_eligible()` — add it to the class name exclusion list.

- **Hotkey conflict**: If `Ctrl+Win+T` conflicts with another application (e.g., Windows Terminal), `RegisterHotKey` will fail. Fall back to no-shortcut and document the conflict. Allow the user to configure an alternative.

- **Many windows**: If dozens of ungrouped windows exist, cap the visible list at ~15 items and add scrolling, or use a virtual list. For v1, a simple capped list is sufficient.

- **DPI scaling**: Render the picker at the correct DPI for the monitor it appears on. Use `GetDpiForWindow` to scale item heights and font sizes.
