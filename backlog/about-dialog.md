# About Dialog

## Problem

There is no way for users to check which version of WinTab they are running. An About dialog accessible from the tray menu displays the application name, version, and a brief description, following standard Windows application conventions.

## Location

**Files modified:**

- `src/tray.rs` — Add "About" menu item (`IDM_ABOUT`) to the context menu. Handle the command to show the dialog.

**New files:** None (implementation is simple enough to live in `tray.rs`).

## Requirements

- [ ] An "About" menu item is added to the tray icon's right-click context menu, between "Disable/Enable" and "Exit".
- [ ] Clicking "About" shows a dialog with:
  - Application name: "WinTab"
  - Version: read from `env!("CARGO_PKG_VERSION")`
  - Description: "Browser-style tab grouping for Windows"
  - An OK button to dismiss
- [ ] Only one About dialog can be open at a time.
- [ ] The dialog is modal or modeless (modeless preferred to keep the tray menu responsive).

## Suggested Implementation

### Simple `MessageBoxW` approach

The simplest implementation uses `MessageBoxW`:

```rust
const IDM_ABOUT: u32 = 1004;

fn show_about() {
    let version = env!("CARGO_PKG_VERSION");
    let text = format!(
        "WinTab v{}\n\nBrowser-style tab grouping for Windows.\0",
        version
    );
    let caption = "About WinTab\0";

    let text_wide: Vec<u16> = text.encode_utf16().collect();
    let caption_wide: Vec<u16> = caption.encode_utf16().collect();

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text_wide.as_ptr(),
            caption_wide.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
```

### Tray menu integration

In `show_context_menu()`, add the About item:

```rust
// After Disable/Enable item:
AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

let about: Vec<u16> = "About\0".encode_utf16().collect();
AppendMenuW(menu, MF_STRING, IDM_ABOUT as usize, about.as_ptr());

// Before Exit item
```

In `handle_command()`:

```rust
IDM_ABOUT => {
    show_about();
    true
}
```

### Custom dialog approach (alternative)

For a richer About dialog with an icon and links, create a custom window:

```rust
fn show_about_dialog(parent: HWND) {
    // Create a small fixed-size window (300x200)
    // WS_CAPTION | WS_SYSMENU | DS_MODALFRAME
    // Render: app icon (LoadIconW), version text (Static controls),
    // OK button (BS_DEFPUSHBUTTON)
    // Handle WM_COMMAND for OK button → DestroyWindow
}
```

### Menu order

The tray menu should now be ordered:

1. Start with Windows (if implemented)
2. separator
3. Disable/Enable
4. separator
5. About
6. Exit

## Edge Cases

- **`MessageBoxW` blocks**: `MessageBoxW` is modal — it blocks the calling thread until the user clicks OK. Since WinTab is single-threaded, this blocks the main message loop. This is acceptable for a simple About dialog, but means no tab operations work while the dialog is open. For a non-blocking alternative, use `CreateDialogW` or `CreateWindowExW` with a custom dialog.

- **Version string**: `env!("CARGO_PKG_VERSION")` is a compile-time constant from `Cargo.toml`. It's always available and doesn't need error handling.

- **DPI awareness**: `MessageBoxW` is automatically DPI-aware on Windows 10+. A custom dialog would need explicit DPI handling.

- **Multiple clicks**: If using `MessageBoxW`, clicking "About" while the dialog is already open has no effect (MessageBox is modal, so the menu is blocked). If using a custom dialog, track the dialog HWND and bring it to front on re-click (same pattern as Config UI's `DIALOG_HWND` static).
