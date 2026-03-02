# Tab Context Menu

## Problem

Users have no right-click menu on tabs, so tab operations require either keyboard shortcuts (which don't exist yet) or drag-and-drop gestures. A context menu provides quick access to common operations — close, close others, rename, ungroup, and blacklist — in a familiar UI pattern.

## Location

**Files modified:**

- `src/overlay.rs` — Handle `WM_RBUTTONUP` in `overlay_wnd_proc`, build and show `TrackPopupMenu`, dispatch selected command via `WM_COMMAND`.
- `src/state.rs` — Add methods: `close_other_tabs()`, `close_all_tabs()`, `ungroup_tab()`, `ungroup_all()`, `blacklist_app()`.
- `src/group.rs` — Reuse existing `TabGroup::remove()` for ungroup operations.
- `src/config.rs` — Add blacklist entry writing (appending to `config.yaml` exceptions section).

**New files:** None.

## Requirements

- [ ] Right-clicking a tab shows a popup context menu at the cursor position.
- [ ] Menu items:
  - **Rename Tab** — enters inline edit mode (see `tab-renaming.md`)
  - separator
  - **Close Tab** — sends `WM_CLOSE` to the clicked tab's window
  - **Close Other Tabs** — sends `WM_CLOSE` to all tabs except the clicked one (greyed out for single-tab groups)
  - **Close All Tabs** — sends `WM_CLOSE` to all tabs in the group
  - separator
  - **Ungroup This Tab** — detaches the clicked tab from the group
  - **Ungroup All** — dissolves the entire group
  - separator
  - **Never Tab This Application** — adds the window's process to the blacklist
  - **Edit Group** — opens the group editor (greyed out for manual groups)
- [ ] The right-clicked tab is visually highlighted while the menu is open.
- [ ] Selecting a menu item performs the action; dismissing the menu does nothing.
- [ ] Context menu works on both active and inactive tabs.

## Suggested Implementation

### Menu item IDs

```rust
const IDM_CTX_RENAME: u32 = 2001;
const IDM_CTX_CLOSE: u32 = 2002;
const IDM_CTX_CLOSE_OTHERS: u32 = 2003;
const IDM_CTX_CLOSE_ALL: u32 = 2004;
const IDM_CTX_UNGROUP: u32 = 2005;
const IDM_CTX_UNGROUP_ALL: u32 = 2006;
const IDM_CTX_NEVER_TAB: u32 = 2007;
const IDM_CTX_EDIT_GROUP: u32 = 2008;
```

### `WM_RBUTTONUP` handler

```rust
WM_RBUTTONUP => {
    let x = get_x_lparam(lparam);
    if let Some((group_id, tab_index)) = hit_test_tab(hwnd, x) {
        show_tab_context_menu(hwnd, group_id, tab_index);
    }
    0
}
```

### Building and showing the menu

```rust
fn show_tab_context_menu(overlay_hwnd: HWND, group_id: GroupId, tab_index: usize) {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() { return; }

        let rename: Vec<u16> = "Rename Tab\0".encode_utf16().collect();
        AppendMenuW(menu, MF_STRING, IDM_CTX_RENAME as usize, rename.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

        let close: Vec<u16> = "Close Tab\0".encode_utf16().collect();
        AppendMenuW(menu, MF_STRING, IDM_CTX_CLOSE as usize, close.as_ptr());

        let tab_count = state::with_state(|s| {
            s.groups.groups.get(&group_id).map(|g| g.tabs.len()).unwrap_or(0)
        });
        let close_others: Vec<u16> = "Close Other Tabs\0".encode_utf16().collect();
        let flag = if tab_count <= 1 { MF_GRAYED } else { MF_STRING };
        AppendMenuW(menu, flag, IDM_CTX_CLOSE_OTHERS as usize, close_others.as_ptr());

        let close_all: Vec<u16> = "Close All Tabs\0".encode_utf16().collect();
        AppendMenuW(menu, MF_STRING, IDM_CTX_CLOSE_ALL as usize, close_all.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

        let ungroup: Vec<u16> = "Ungroup This Tab\0".encode_utf16().collect();
        AppendMenuW(menu, MF_STRING, IDM_CTX_UNGROUP as usize, ungroup.as_ptr());
        let ungroup_all: Vec<u16> = "Ungroup All\0".encode_utf16().collect();
        AppendMenuW(menu, MF_STRING, IDM_CTX_UNGROUP_ALL as usize, ungroup_all.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

        let never_tab: Vec<u16> = "Never Tab This Application\0".encode_utf16().collect();
        AppendMenuW(menu, MF_STRING, IDM_CTX_NEVER_TAB as usize, never_tab.as_ptr());

        set_context_menu_target(group_id, tab_index);
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(overlay_hwnd);
        TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, 0, overlay_hwnd, std::ptr::null());
        PostMessageW(overlay_hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }
}
```

### Command dispatch

Add `WM_COMMAND` handling to `overlay_wnd_proc`:

```rust
WM_COMMAND => {
    let cmd = (wparam & 0xFFFF) as u32;
    let (group_id, tab_index) = get_context_menu_target();
    match cmd {
        IDM_CTX_RENAME => start_inline_edit(hwnd, group_id, tab_index),
        IDM_CTX_CLOSE => state::with_state(|s| s.close_tab(group_id, tab_index)),
        IDM_CTX_CLOSE_OTHERS => state::with_state(|s| s.close_other_tabs(group_id, tab_index)),
        IDM_CTX_CLOSE_ALL => state::with_state(|s| s.close_all_tabs(group_id)),
        IDM_CTX_UNGROUP => state::with_state(|s| s.ungroup_tab(group_id, tab_index)),
        IDM_CTX_UNGROUP_ALL => state::with_state(|s| s.ungroup_all(group_id)),
        IDM_CTX_NEVER_TAB => state::with_state(|s| s.blacklist_app(group_id, tab_index)),
        _ => {}
    }
    0
}
```

### State methods

```rust
pub fn close_other_tabs(&mut self, group_id: GroupId, keep_index: usize) {
    if let Some(group) = self.groups.groups.get(&group_id) {
        let to_close: Vec<HWND> = group.tabs.iter().enumerate()
            .filter(|&(i, _)| i != keep_index)
            .map(|(_, &h)| h).collect();
        for hwnd in to_close {
            unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0); }
        }
    }
}

pub fn close_all_tabs(&mut self, group_id: GroupId) {
    if let Some(group) = self.groups.groups.get(&group_id) {
        for &hwnd in &group.tabs {
            unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0); }
        }
    }
}

pub fn ungroup_tab(&mut self, group_id: GroupId, tab_index: usize) {
    // Same as detach logic in drag.rs drop handler
    if let Some(group) = self.groups.groups.get_mut(&group_id) {
        if let Some(&hwnd) = group.tabs.get(tab_index) {
            let dissolve = group.remove(hwnd);
            self.groups.window_to_group.remove(&hwnd);
            if dissolve { /* dissolve group, remove overlay */ }
        }
    }
}

pub fn blacklist_app(&mut self, group_id: GroupId, tab_index: usize) {
    if let Some(group) = self.groups.groups.get(&group_id) {
        if let Some(&hwnd) = group.tabs.get(tab_index) {
            if let Some(info) = self.windows.get(&hwnd) {
                let process = info.process_name.clone();
                self.rules.add_blacklist_entry(&process);
            }
        }
    }
}
```

## Edge Cases

- **Menu popup and `WS_EX_NOACTIVATE`**: `TrackPopupMenu` requires the owner to be foreground. Call `SetForegroundWindow` before `TrackPopupMenu`. If the overlay's `WS_EX_NOACTIVATE` prevents this, use `msg_hwnd` as the popup owner and route `WM_COMMAND` there.

- **Stale context menu target**: Between showing the menu and the user clicking, tabs may have changed. Validate `group_id` and `tab_index` in each command handler.

- **Close Others with unsaved windows**: Each `PostMessageW(WM_CLOSE)` is independent. Windows with unsaved changes will show their own dialogs.

- **TrackPopupMenu blocks**: `TrackPopupMenu` runs a modal loop. This is safe because `overlay_wnd_proc` runs outside `with_state()`.

- **Right-click during drag**: If `drag::is_dragging()`, right-click should cancel the drag rather than show a context menu.

- **Ungroup last tab**: Ungrouping from a 2-tab group dissolves the group (existing `TabGroup::remove` logic handles this).
