# Reset to Defaults

## Problem

As WinTab adds more configuration options (colors, shortcuts, rules, positions), users may end up with a broken or undesirable configuration. There is no quick way to restore all settings to their factory defaults without manually editing or deleting config files. A "Reset to Defaults" action provides a safety net.

## Location

**Files modified:**

- `src/config.rs` — Add `reset_to_defaults()` function that overwrites config and position files with defaults.
- `src/state.rs` — Add `apply_defaults()` method that resets live state and repaints all overlays.
- `src/tray.rs` — Optionally add a "Reset to Defaults" menu item (or surface it through the Settings dialog).

**New files:** None.

## Requirements

- [ ] A "Reset to Defaults" action accessible from the Settings dialog or tray menu.
- [ ] Confirmation dialog before resetting ("Are you sure? This will reset all settings to defaults.").
- [ ] Resets `config.yaml` to default values (preserving the file, not deleting it).
- [ ] Optionally resets `positions.yaml` (can be a separate checkbox: "Also reset saved positions").
- [ ] Live state is updated immediately — overlays repaint with default colors/sizes.
- [ ] Hotkeys are re-registered with default bindings.
- [ ] Auto-grouping rules are cleared.
- [ ] Blacklist/whitelist exceptions are cleared.

## Suggested Implementation

### Reset function in `config.rs`

```rust
pub fn reset_to_defaults() -> RulesEngine {
    let default_config = ConfigFile::default();
    let path = appdata::config_path();

    // Write default config to disk
    if let Ok(yaml) = serde_yaml::to_string(&default_config) {
        let _ = std::fs::write(&path, yaml);
    }

    // Parse back into RulesEngine (same as load path)
    parse_config_file(default_config)
}
```

### Position store reset

```rust
pub fn reset_positions() {
    let path = appdata::positions_path();
    let default = PositionStore::empty();
    if let Ok(yaml) = serde_yaml::to_string(&default) {
        let _ = std::fs::write(&path, yaml);
    }
}
```

### Applying defaults to live state

```rust
pub fn apply_defaults(&mut self) {
    // Reset rules engine
    self.rules = config::reset_to_defaults();

    // Reset position store
    position_store::reset_positions();
    self.position_store = PositionStore::empty();

    // Ungroup all existing groups
    let group_ids: Vec<GroupId> = self.groups.groups.keys().copied().collect();
    for gid in group_ids {
        self.ungroup_all(gid);
    }

    // Re-register hotkeys with defaults (if hotkey feature exists)
    // hotkey::unregister_all(self.msg_hwnd);
    // hotkey::register_defaults(self.msg_hwnd);

    // Repaint all overlays with default colors/sizes
    self.overlays.update_all(&self.groups, &self.windows);
}
```

### Confirmation dialog

```rust
fn confirm_reset(parent: HWND) -> bool {
    let text: Vec<u16> = "Are you sure you want to reset all settings to defaults?\n\nThis will clear all auto-grouping rules, exceptions, and custom settings.\0"
        .encode_utf16().collect();
    let caption: Vec<u16> = "Reset to Defaults\0".encode_utf16().collect();

    let result = unsafe {
        MessageBoxW(parent, text.as_ptr(), caption.as_ptr(),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2)
    };
    result == IDYES
}
```

### Tray menu integration

```rust
const IDM_RESET: u32 = 1005;

// In show_context_menu():
let reset: Vec<u16> = "Reset to Defaults\0".encode_utf16().collect();
AppendMenuW(menu, MF_STRING, IDM_RESET as usize, reset.as_ptr());

// In handle_command():
IDM_RESET => {
    if confirm_reset(msg_hwnd) {
        state::with_state(|s| s.apply_defaults());
    }
    true
}
```

## Edge Cases

- **Confirmation prevents accidents**: The `MessageBoxW` confirmation with `MB_DEFBUTTON2` (No is default) prevents accidental resets. The user must explicitly click Yes.

- **MessageBoxW blocks**: `MessageBoxW` is modal and blocks the main thread. This is acceptable for a rare, destructive action.

- **Partial reset**: Consider offering granular reset options: "Reset appearance only", "Reset shortcuts only", "Reset rules only". For v1, a full reset is simpler.

- **Config file write failure**: If writing the default config fails (permissions, disk full), show an error via `MessageBoxW` and do not modify live state.

- **Active groups during reset**: Ungrouping all active groups shows all hidden windows. This is correct — the user is resetting to a clean state. However, window positions may not match their pre-group positions (they'll have the group's position).

- **Start-with-Windows flag**: Reset should also disable the autostart registry key if `start_with_windows` defaults to `false`.

- **Config hot reload**: If config hot-reload is enabled, writing the default config file triggers a reload event. Coordinate with the hot-reload watcher to avoid double-applying defaults.
