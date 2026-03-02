# Keyboard Shortcuts

## Problem

All tab operations require mouse interaction — clicking tabs, dragging, hovering. Power users need keyboard shortcuts to cycle tabs, detach, close, and perform other operations without reaching for the mouse. The spec defines 15+ default shortcuts using `Ctrl+Win` modifier combinations.

## Location

**Files modified:**

- `src/main.rs` — Register global hotkeys via `RegisterHotKey` during startup. Handle `WM_HOTKEY` in the main message loop to dispatch actions.
- `src/state.rs` — Add action methods: `cycle_tab_forward()`, `cycle_tab_backward()`, `detach_active_tab()`, `close_active_tab()`, etc.
- `src/config.rs` — Add `shortcuts: HashMap<Action, HotkeyDef>` to config. Parse hotkey definitions from YAML.
- `src/group.rs` — Add `TabGroup::cycle_forward()` and `TabGroup::cycle_backward()` convenience methods.

**New files:**

- `src/hotkey.rs` — `HotkeyDef` struct, registration/unregistration lifecycle, `WM_HOTKEY` dispatch table.

## Requirements

- [ ] Global hotkeys registered via `RegisterHotKey` — work from any focused application.
- [ ] Default shortcuts (all configurable):
  - `Ctrl+Win+PgDown` — Next tab
  - `Ctrl+Win+PgUp` — Previous tab
  - `Ctrl+Win+Home` — First tab
  - `Ctrl+Win+End` — Last tab
  - `Ctrl+Win+W` — Close current tab
  - `Ctrl+Win+-` — Ungroup current tab
  - `Ctrl+Shift+Win+-` — Ungroup all tabs
  - `Ctrl+Win+T` — New tab from running windows
  - `Win+F2` — Rename current tab
- [ ] Hotkeys only affect the focused group (the group whose active window is in the foreground).
- [ ] Hotkeys are no-ops when WinTab is disabled or no groups exist.
- [ ] Conflicting hotkeys (already registered by another app) show a warning and skip registration.
- [ ] Hotkeys can be disabled individually by setting them to `None` in config.

## Suggested Implementation

### `HotkeyDef` struct

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HotkeyDef {
    pub modifiers: u32,  // MOD_CONTROL | MOD_WIN | MOD_SHIFT | MOD_ALT
    pub vk: u32,         // Virtual key code (VK_NEXT, VK_PRIOR, etc.)
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize, Serialize)]
pub enum HotkeyAction {
    NextTab,
    PrevTab,
    FirstTab,
    LastTab,
    CloseTab,
    UngroupTab,
    UngroupAll,
    NewTab,
    RenameTab,
}
```

### Registration lifecycle

```rust
static HOTKEY_ID_BASE: u32 = 5000;

pub fn register_hotkeys(msg_hwnd: HWND, shortcuts: &HashMap<HotkeyAction, HotkeyDef>) {
    for (i, (action, def)) in shortcuts.iter().enumerate() {
        let id = HOTKEY_ID_BASE + i as u32;
        let result = unsafe {
            RegisterHotKey(msg_hwnd, id as i32, def.modifiers, def.vk)
        };
        if result == 0 {
            // ERROR_HOTKEY_ALREADY_REGISTERED — log warning
            eprintln!("Hotkey conflict for {:?}: modifiers={}, vk={}", action, def.modifiers, def.vk);
        }
    }
}

pub fn unregister_hotkeys(msg_hwnd: HWND, count: usize) {
    for i in 0..count {
        unsafe { UnregisterHotKey(msg_hwnd, (HOTKEY_ID_BASE + i as u32) as i32); }
    }
}
```

### `WM_HOTKEY` dispatch in `main.rs`

```rust
WM_HOTKEY => {
    let id = wparam as u32;
    if let Some(action) = hotkey_id_to_action(id) {
        state::with_state(|s| {
            if !s.enabled { return; }
            match action {
                HotkeyAction::NextTab => s.cycle_tab(1),
                HotkeyAction::PrevTab => s.cycle_tab(-1),
                HotkeyAction::FirstTab => s.go_to_tab(0),
                HotkeyAction::LastTab => s.go_to_tab_last(),
                HotkeyAction::CloseTab => s.close_active_tab(),
                HotkeyAction::UngroupTab => s.ungroup_active_tab(),
                HotkeyAction::UngroupAll => s.ungroup_all_active(),
                _ => {}
            }
        });
    }
    0
}
```

### Identifying the focused group

```rust
pub fn cycle_tab(&mut self, direction: i32) {
    let fg = unsafe { GetForegroundWindow() };
    if let Some(&group_id) = self.groups.window_to_group.get(&fg) {
        if let Some(group) = self.groups.groups.get_mut(&group_id) {
            let new_idx = if direction > 0 {
                (group.active + 1) % group.tabs.len()
            } else {
                (group.active + group.tabs.len() - 1) % group.tabs.len()
            };
            group.switch_to(new_idx);
            self.overlays.refresh_overlay(group_id, &self.groups, &self.windows);
        }
    }
}
```

### YAML config format

```yaml
shortcuts:
  next_tab: "Ctrl+Win+PgDown"
  prev_tab: "Ctrl+Win+PgUp"
  close_tab: "Ctrl+Win+W"
  ungroup_tab: "Ctrl+Win+Minus"
```

Parse with a `parse_hotkey_string()` function that splits on `+` and maps modifier/key names to Win32 constants.

## Edge Cases

- **Hotkey conflicts**: `RegisterHotKey` fails with `ERROR_HOTKEY_ALREADY_REGISTERED` if another application has the same hotkey. Log a warning but continue — don't crash. Optionally surface the conflict in a config UI.

- **Admin privilege hotkeys**: Some `Win+` combinations are reserved by the system (e.g., `Win+L` for lock). `RegisterHotKey` will fail for these. Validate against known reserved combinations.

- **No focused group**: If the foreground window is not in any group, hotkeys should be silently ignored — do not cycle other groups.

- **Single-tab group**: Cycling in a single-tab group is a no-op. Tab index doesn't change.

- **Re-registration on config change**: When hotkeys are changed in config, unregister all old hotkeys and register the new ones. Use `unregister_hotkeys()` followed by `register_hotkeys()`.

- **WinTab disabled**: When WinTab is disabled via the tray menu, hotkeys should still be registered but their handlers should be no-ops. Alternatively, unregister hotkeys on disable and re-register on enable.

- **Cleanup on exit**: Call `unregister_hotkeys()` in `shutdown()` to clean up. If the process crashes, Windows automatically cleans up registered hotkeys.
