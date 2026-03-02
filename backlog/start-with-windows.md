# Start with Windows

## Problem

Users must manually launch WinTab after every system restart. There is no way to configure WinTab to start automatically with Windows. A "Start with Windows" toggle in the tray menu or config adds a registry run key to launch WinTab at login.

## Location

**Files modified:**

- `src/tray.rs` — Add "Start with Windows" checkbox menu item to the context menu, toggling the registry key. Check current state on menu display.
- `src/config.rs` — Add `start_with_windows: bool` field (default: `false`).

**New files:** None.

## Requirements

- [ ] A "Start with Windows" option in the tray context menu, displayed as a checkmarked menu item.
- [ ] When enabled, a registry value is created at `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` with the key `"WinTab"` and the value set to the current executable's full path.
- [ ] When disabled, the registry value is removed.
- [ ] The menu item reflects the current state (checked if the registry key exists, unchecked if not).
- [ ] The registry key uses the absolute path from `std::env::current_exe()`.
- [ ] No admin privileges required (uses `HKCU`, not `HKLM`).

## Suggested Implementation

### Menu item in `tray.rs`

Add a new menu item ID and modify `show_context_menu()`:

```rust
const IDM_AUTOSTART: u32 = 1003;

fn show_context_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() { return; }

        // Check current autostart state
        let autostart_enabled = is_autostart_enabled();
        let autostart_label: Vec<u16> = "Start with Windows\0".encode_utf16().collect();
        let autostart_flags = MF_STRING | if autostart_enabled { MF_CHECKED } else { 0 };
        AppendMenuW(menu, autostart_flags, IDM_AUTOSTART as usize, autostart_label.as_ptr());

        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

        // existing Disable/Enable + Exit items...
    }
}
```

### Registry operations

```rust
use windows_sys::Win32::System::Registry::*;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "WinTab";

fn set_autostart(enable: bool) {
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let subkey: Vec<u16> = RUN_KEY.encode_utf16().chain(std::iter::once(0)).collect();

        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut key,
        );
        if result != 0 { return; }

        let name: Vec<u16> = VALUE_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        if enable {
            let exe_path = std::env::current_exe().unwrap_or_default();
            let path_str = exe_path.to_string_lossy();
            let path_wide: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                path_wide.as_ptr() as *const u8,
                (path_wide.len() * 2) as u32,
            );
        } else {
            RegDeleteValueW(key, name.as_ptr());
        }

        RegCloseKey(key);
    }
}

fn is_autostart_enabled() -> bool {
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let subkey: Vec<u16> = RUN_KEY.encode_utf16().chain(std::iter::once(0)).collect();

        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_READ,
            &mut key,
        );
        if result != 0 { return false; }

        let name: Vec<u16> = VALUE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        let mut value_type: u32 = 0;
        let mut size: u32 = 0;
        let exists = RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut value_type,
            std::ptr::null_mut(),
            &mut size,
        ) == 0;

        RegCloseKey(key);
        exists
    }
}
```

### Command handling in `handle_command`

```rust
IDM_AUTOSTART => {
    let currently_enabled = is_autostart_enabled();
    set_autostart(!currently_enabled);
    true
}
```

### Cargo.toml feature gate

Add `Win32_System_Registry` to the `windows-sys` features list:

```toml
[dependencies.windows-sys]
features = [
    # ...existing features...
    "Win32_System_Registry",
]
```

## Edge Cases

- **Executable moved**: If the user moves the WinTab executable after enabling autostart, the registry path becomes stale. Windows will show an error on login. Consider validating the path on startup and updating the registry if `current_exe()` differs from the stored path.

- **Portable mode**: If WinTab is run from a USB drive, the drive letter may change. The registry path will be incorrect. Document this limitation.

- **Multiple instances**: If autostart is enabled and the user also manually launches WinTab, two instances may run. The existing single-instance check (if any) should prevent this. If no single-instance guard exists, add one via a named mutex.

- **UAC and permissions**: `HKCU` writes do not require admin privileges. This works correctly for standard users.

- **Registry write failure**: If the registry write fails (unlikely for HKCU), log the error via `OutputDebugStringW` and do not update the menu state. The menu should reflect the actual registry state, not the intended state.

- **Uninstall cleanup**: If WinTab is uninstalled without disabling autostart first, the registry key will remain as an orphan entry. An installer should remove it, or document manual cleanup.
