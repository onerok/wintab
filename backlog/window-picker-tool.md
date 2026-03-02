# Window Picker Tool

## Problem

When creating auto-grouping rules in the config, users must manually determine a window's process name, class name, and title. This requires external tools like Spy++ or Task Manager. A crosshair picker tool — inspired by Spy++ and color picker tools — lets the user click on any window to auto-fill rule fields, making rule creation much faster.

## Location

**Files modified:**

- `src/state.rs` — Add `start_picker()` and `finish_picker()` methods. Store picker state.

**New files:**

- `src/picker.rs` — Picker window (fullscreen transparent overlay with crosshair cursor), `WindowFromPoint` target detection, window metadata extraction, visual feedback (highlight target window border).

## Requirements

- [ ] A crosshair tool that can be activated from the config UI's rule editor.
- [ ] While active, the cursor changes to a crosshair and a semi-transparent overlay covers the screen.
- [ ] As the user moves the mouse, the window under the cursor is highlighted (border drawn around it).
- [ ] Clicking a window captures its metadata: `process_name`, `class_name`, `title`.
- [ ] The captured metadata is returned to the caller (rule editor) to populate fields.
- [ ] Pressing Escape cancels the picker without selecting a window.
- [ ] The picker does not interfere with WinTab's own overlays (excludes WinTab windows from selection).

## Suggested Implementation

### Picker window

Create a fullscreen transparent window that captures all input:

```rust
pub fn start_picker(callback: Box<dyn FnOnce(Option<PickedWindow>)>) {
    // Create a fullscreen, topmost, transparent window
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            picker_class.as_ptr(),
            std::ptr::null(),
            WS_POPUP | WS_VISIBLE,
            0, 0,
            GetSystemMetrics(SM_CXSCREEN),
            GetSystemMetrics(SM_CYSCREEN),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    };

    // Make nearly transparent (alpha=1) so it captures mouse but shows desktop
    unsafe {
        SetLayeredWindowAttributes(hwnd, 0, 1, LWA_ALPHA);
        SetCapture(hwnd);
    }
    // Set crosshair cursor
    unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_CROSS)); }
}
```

### PickedWindow result

```rust
pub struct PickedWindow {
    pub hwnd: HWND,
    pub process_name: String,
    pub class_name: String,
    pub title: String,
    pub rect: RECT,
}
```

### Target detection

```rust
fn get_target_at(screen_x: i32, screen_y: i32, picker_hwnd: HWND) -> Option<HWND> {
    // Temporarily hide picker window to get the real window underneath
    unsafe { ShowWindow(picker_hwnd, SW_HIDE); }
    let pt = POINT { x: screen_x, y: screen_y };
    let target = unsafe { WindowFromPoint(pt) };
    unsafe { ShowWindow(picker_hwnd, SW_SHOW); }

    if target.is_null() { return None; }

    // Get root owner window
    let root = unsafe { GetAncestor(target, GA_ROOT) };
    if root.is_null() { return None; }

    // Exclude WinTab's own windows
    let class = window::get_class_name(root);
    if class == "WinTabOverlay" || class == "WinTabPrv" || class == "WinTabMsg" {
        return None;
    }

    Some(root)
}
```

### Visual feedback — highlight target window

Draw a colored border around the currently targeted window:

```rust
fn highlight_window(hwnd: HWND) {
    unsafe {
        let hdc = GetWindowDC(hwnd);
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect);

        // Draw a 3px red border
        let pen = CreatePen(PS_SOLID as i32, 3, 0x000000FF); // RGB red
        let old_pen = SelectObject(hdc, pen as _);
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH) as _);

        Rectangle(hdc, 0, 0, rect.right - rect.left, rect.bottom - rect.top);

        SelectObject(hdc, old_pen as _);
        SelectObject(hdc, old_brush as _);
        DeleteObject(pen as _);
        ReleaseDC(hwnd, hdc);
    }
}

fn unhighlight_window(hwnd: HWND) {
    // Force the window to repaint its non-client area to remove the border
    unsafe {
        RedrawWindow(hwnd, std::ptr::null(), std::ptr::null_mut(),
            RDW_INVALIDATE | RDW_FRAME | RDW_UPDATENOW);
    }
}
```

### Picker window proc

```rust
fn picker_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => {
            let x = get_x_lparam(lparam);
            let y = get_y_lparam(lparam);
            let target = get_target_at(x, y, hwnd);
            update_highlight(target);
            0
        }
        WM_LBUTTONUP => {
            let x = get_x_lparam(lparam);
            let y = get_y_lparam(lparam);
            let target = get_target_at(x, y, hwnd);
            finish_picker(hwnd, target);
            0
        }
        WM_KEYDOWN if wparam as u32 == VK_ESCAPE => {
            cancel_picker(hwnd);
            0
        }
        WM_RBUTTONUP => {
            cancel_picker(hwnd);
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}
```

### Extracting metadata on selection

```rust
fn finish_picker(picker_hwnd: HWND, target: Option<HWND>) {
    unhighlight_current();
    unsafe {
        ReleaseCapture();
        DestroyWindow(picker_hwnd);
    }

    let result = target.map(|hwnd| PickedWindow {
        hwnd,
        process_name: window::get_process_name(hwnd),
        class_name: window::get_class_name(hwnd),
        title: window::get_window_title(hwnd),
        rect: window::get_window_rect(hwnd),
    });

    // Invoke callback with result
    if let Some(cb) = take_picker_callback() {
        cb(result);
    }
}
```

## Edge Cases

- **Multi-monitor**: The picker window must span all monitors, not just the primary. Use `GetSystemMetrics(SM_XVIRTUALSCREEN)`, `SM_YVIRTUALSCREEN`, `SM_CXVIRTUALSCREEN`, `SM_CYSVIRTUALSCREEN` for the fullscreen rect.

- **DPI per monitor**: On multi-monitor setups with different DPIs, `WindowFromPoint` uses logical coordinates. Ensure the picker window's coordinate system matches.

- **Admin windows**: If WinTab is not running as admin, it cannot interact with admin-elevation windows. `WindowFromPoint` may return the admin window's HWND, but `GetWindowText` and `GetProcessImageFileName` may fail. Handle gracefully by returning empty strings for inaccessible fields.

- **UWP/Store apps**: UWP windows have different class names (e.g., `ApplicationFrameWindow`). The picker should work for these windows; metadata extraction uses the same Win32 APIs.

- **Flicker during highlight**: Repeatedly calling `GetWindowDC` + `Rectangle` + `RedrawWindow` may cause flicker. Consider using a separate transparent overlay window positioned over the target instead of drawing directly on the target's DC.

- **Picker and overlays**: During picking, WinTab overlays should remain visible but be excluded from selection. Check the window class in `get_target_at()` to skip WinTab windows.

- **SetCapture and other apps**: `SetCapture` on the picker window ensures all mouse messages go to it, even over other windows. This is released on click or cancel.
