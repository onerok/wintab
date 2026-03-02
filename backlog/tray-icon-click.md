# Tray Icon Left-Click / Double-Click Opens Config Dialog

## Problem

The tray icon only responds to right-click (context menu). Left-click and double-click are ignored. Following Windows conventions, left-clicking or double-clicking a tray icon should open the application's primary UI — in WinTab's case, the Settings/Config dialog (see `config-ui.md`).

## Location

**Files modified:**

- `src/tray.rs` — Handle `WM_LBUTTONUP` and `WM_LBUTTONDBLCLK` in `handle_tray_message()` to open the config dialog.

**New files:** None (depends on `config-ui.md` for the dialog implementation).

## Requirements

- [ ] Left-clicking (`WM_LBUTTONUP`) the tray icon opens the Settings dialog.
- [ ] Double-clicking (`WM_LBUTTONDBLCLK`) the tray icon opens the Settings dialog.
- [ ] If the Settings dialog is already open, focus is brought to it rather than opening a second instance.
- [ ] Single left-click behavior is consistent — no delay waiting for a potential double-click.
- [ ] This feature depends on the Config UI feature being implemented first.

## Suggested Implementation

### Handle click events in `handle_tray_message`

```rust
pub fn handle_tray_message(msg_hwnd: HWND, lparam: LPARAM) -> bool {
    let event = (lparam & 0xFFFF) as u32;

    match event {
        WM_RBUTTONUP | WM_CONTEXTMENU => {
            show_context_menu(msg_hwnd);
            true
        }
        WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
            open_settings_dialog(msg_hwnd);
            true
        }
        _ => false,
    }
}
```

### Settings dialog singleton

The Config UI PBI defines a `DIALOG_HWND` static to track the open dialog. Use it here:

```rust
fn open_settings_dialog(parent: HWND) {
    // If dialog module exists (config-ui implemented):
    if let Some(existing) = dialog::get_settings_hwnd() {
        unsafe { SetForegroundWindow(existing); }
    } else {
        dialog::open_settings(parent);
    }
}
```

### Interim behavior (before Config UI is implemented)

Before the Config UI is implemented, left-click could:
- Do nothing (silently ignore)
- Show the About dialog as a placeholder
- Show a balloon tip: "Settings UI coming soon"

```rust
WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
    // Placeholder until config-ui is implemented:
    // show_about();
    true
}
```

## Edge Cases

- **Single vs. double click**: Windows sends both `WM_LBUTTONUP` (on single click) and `WM_LBUTTONDBLCLK` (on double click). Both should open the dialog. The first click opens it; the second click (double-click) sees it's already open and brings it to front. No special timing logic needed.

- **Dialog from tray focus**: Opening a dialog from a tray icon click requires `SetForegroundWindow` on the dialog. Call `SetForegroundWindow(msg_hwnd)` before creating the dialog, as Windows only allows foreground window changes from the foreground process.

- **Config UI not yet implemented**: This PBI has a dependency on `config-ui.md`. If implemented before Config UI, the click handler should be a no-op or show the About dialog.

- **Tray icon message version**: `NOTIFYICON_VERSION_4` (already set in `add_tray_icon`) changes how tray messages are delivered. With this version, the low word of lParam is the event (e.g., `WM_LBUTTONUP`), which is how `handle_tray_message` already parses it.

- **Rapid clicks**: Multiple rapid left-clicks should not open multiple dialogs. The singleton check (`get_settings_hwnd()`) prevents this.
