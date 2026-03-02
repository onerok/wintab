# Tab Renaming

## Problem

Tabs always display the window's title bar text, which may be long, unhelpful (e.g., "Document1 - Notepad"), or change unexpectedly (e.g., browser tabs updating on navigation). Users need a way to assign custom short names to tabs that persist for the lifetime of the window, making it easier to identify tabs at a glance.

## Location

**Files modified:**

- `src/overlay.rs` — Handle `WM_LBUTTONDBLCLK` in `overlay_wnd_proc` to enter inline edit mode. Create and manage an `EDIT` control over the tab. Use custom name in `paint_tabs()` when rendering tab text.
- `src/group.rs` — Add `custom_names: HashMap<HWND, String>` to `TabGroup` for per-window custom names.
- `src/state.rs` — Add `rename_tab()` and `get_tab_display_name()` methods.

**New files:** None.

## Requirements

- [ ] Double-clicking a tab enters inline edit mode with a text input field positioned over the tab.
- [ ] The edit field is pre-populated with the current display name (custom name if set, else window title).
- [ ] Pressing Enter commits the new name.
- [ ] Pressing Escape cancels the rename (reverts to previous name).
- [ ] Clicking outside the edit field commits the new name (focus loss = commit).
- [ ] Custom names persist until the window is closed or the user clears the custom name.
- [ ] An empty custom name reverts to showing the window's actual title.
- [ ] Tab rendering uses the custom name when available, falling back to `WindowInfo.title`.
- [ ] The rename action is also available via the tab context menu (see `tab-context-menu.md`).
- [ ] Tooltip shows the custom name (if set) as primary, with original title as secondary.

## Suggested Implementation

### Custom name storage in `TabGroup`

```rust
pub struct TabGroup {
    pub id: GroupId,
    pub tabs: Vec<HWND>,
    pub active: usize,
    pub custom_names: HashMap<HWND, String>,
}

impl TabGroup {
    pub fn display_name(&self, hwnd: HWND, window_title: &str) -> &str {
        self.custom_names
            .get(&hwnd)
            .map(|s| s.as_str())
            .unwrap_or(window_title)
    }

    pub fn set_custom_name(&mut self, hwnd: HWND, name: String) {
        if name.is_empty() {
            self.custom_names.remove(&hwnd);
        } else {
            self.custom_names.insert(hwnd, name);
        }
    }
}
```

### Inline edit control

On `WM_LBUTTONDBLCLK`, create a Win32 `EDIT` control over the clicked tab:

```rust
WM_LBUTTONDBLCLK => {
    let x = get_x_lparam(lparam);
    if let Some((group_id, tab_index)) = hit_test_tab(hwnd, x) {
        start_inline_edit(hwnd, group_id, tab_index);
    }
    0
}
```

```rust
fn start_inline_edit(overlay_hwnd: HWND, group_id: GroupId, tab_index: usize) {
    let tab_rect = get_tab_rect(overlay_hwnd, tab_index);
    let current_name = state::with_state(|s| { /* get display name */ });

    unsafe {
        let edit_class: Vec<u16> = "EDIT\0".encode_utf16().collect();
        let text: Vec<u16> = format!("{}\0", current_name).encode_utf16().collect();

        let edit = CreateWindowExW(
            0,
            edit_class.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER | ES_AUTOHSCROLL,
            tab_rect.left + ICON_SIZE + TAB_PADDING,
            0,
            tab_rect.right - tab_rect.left - ICON_SIZE - TAB_PADDING * 2,
            TAB_HEIGHT,
            overlay_hwnd,
            0 as _,
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );

        SendMessageW(edit, EM_SETSEL, 0, -1isize as LPARAM);
        SetFocus(edit);
    }
}
```

### Edit control lifecycle

Store the active edit HWND in `OverlayData`:

```rust
struct OverlayData {
    group_id: GroupId,
    hover_tab: i32,
    peek_hwnd: HWND,
    edit_hwnd: HWND,         // null when not editing
    edit_group: GroupId,
    edit_tab: usize,
}
```

Subclass the `EDIT` control to intercept Enter and Escape:

```rust
// In the subclassed edit proc:
WM_KEYDOWN if wparam == VK_RETURN => { commit_rename(parent_overlay); 0 }
WM_KEYDOWN if wparam == VK_ESCAPE => { cancel_rename(parent_overlay); 0 }
WM_KILLFOCUS => { commit_rename(parent_overlay); CallWindowProcW(...) }
```

### Commit and cancel

```rust
fn commit_rename(overlay_hwnd: HWND) {
    let data = get_overlay_data(overlay_hwnd);
    if data.edit_hwnd.is_null() { return; }

    let text = get_edit_text(data.edit_hwnd);
    state::try_with_state(|s| {
        if let Some(group) = s.groups.groups.get_mut(&data.edit_group) {
            if let Some(&hwnd) = group.tabs.get(data.edit_tab) {
                group.set_custom_name(hwnd, text);
            }
        }
        s.overlays.refresh_overlay(data.edit_group, &s.groups, &s.windows);
    });

    unsafe { DestroyWindow(data.edit_hwnd); }
    data.edit_hwnd = std::ptr::null_mut();
}
```

### Rendering with custom names

In `paint_tabs()`, replace `info.title` with the display name:

```rust
let display_name = group.display_name(hwnd, &info.title);
// Use display_name for DrawTextW call
```

## Edge Cases

- **Overlay is `WS_EX_NOACTIVATE`**: The overlay has `WS_EX_NOACTIVATE`, which may prevent the `EDIT` control from receiving focus. Temporarily remove `WS_EX_NOACTIVATE` while the edit is active, or use `SetFocus` explicitly.

- **Double-click vs. drag**: `WM_LBUTTONDBLCLK` is delivered after a `WM_LBUTTONDOWN` + `WM_LBUTTONUP` + `WM_LBUTTONDOWN` sequence. The first click may start a drag state. Clear any pending drag state before entering edit mode.

- **Window title changes during edit**: If the window's title changes (via `EVENT_OBJECT_NAMECHANGE`) while the edit is active, ignore the title change — the edit control shows the user's input.

- **Overlay repaint during edit**: The `EDIT` control is a child of the overlay. When the overlay repaints via `UpdateLayeredWindow`, the edit may be covered. Skip the edited tab's background paint, or create the edit as a separate top-level window positioned over the tab.

- **Custom name cleanup**: When a window is removed from a group (`TabGroup::remove`), also remove its `custom_names` entry. When a group dissolves, all custom names are lost.

- **`WS_EX_LAYERED` interaction**: Child windows on layered windows have rendering quirks. The `EDIT` control may not be visible unless the edit is created as a separate top-level popup window positioned over the tab, rather than a child of the layered overlay.

- **Long custom names**: The edit allows any length, but rendering truncates with ellipsis. The tooltip shows the full custom name.
