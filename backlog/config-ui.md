# Configuration UI

## Problem

WinTab currently has no user-facing configuration. All rendering constants (tab height, colors, opacity, auto-hide timing) are compile-time values in `overlay.rs`. The tray menu offers only Enable/Disable and Exit — no Settings entry exists. Users cannot change tab appearance, adjust behavior, or control start-up options without rebuilding the binary. A persistent configuration system with a proper settings dialog is needed before the application can be considered production-ready.

## Location

**Files modified:**

- `src/tray.rs` — Add Settings and About menu items; wire left-click / double-click to open dialog; add `IDM_SETTINGS` and `IDM_ABOUT` command IDs.
- `src/state.rs` — Add `pub config: Config` field to `AppState`; load config during init; pass relevant fields into overlay rendering calls.
- `src/overlay.rs` — Replace all hard-coded color/opacity/size constants with values read from a `Config` reference passed at render time (or a module-level accessor that reads from `AppState`).

**New files:**

- `src/config.rs` — `Config` struct with `Default` impl; `load()` / `save()` functions using `%APPDATA%\WinTab\config.json`; `apply_to_overlays()` helper that triggers a full overlay repaint.
- `src/dialog.rs` — Win32 property-sheet or tabbed custom dialog window; message handler; per-tab child dialog procedures.

## Requirements

### Tray menu

- [ ] Right-click context menu contains: Settings, separator, Enable/Disable (existing), separator, About, Exit.
- [ ] Left-click (WM_LBUTTONUP) and double-click (WM_LBUTTONDBLCLK) on the tray icon open the Settings dialog.
- [ ] If the Settings dialog is already open, focus is brought to it rather than opening a second instance.

### Settings dialog — General tab

- [ ] Checkbox: "Start WinTab with Windows" (reads/writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
- [ ] Radio group: Tab bar position — Top / Bottom (default: Top).
- [ ] Checkbox: "Show close button on tabs" (default: true).
- [ ] Checkbox: "Show new-tab button" (default: false).

### Settings dialog — Appearance tab

- [ ] Slider: Active-tab opacity (0–100%, default 90%).
- [ ] Slider: Inactive-tab opacity (0–100%, default 70%).
- [ ] Color picker or dropdown: Tab color mode — System accent / Custom / Dark / Light (default: Dark).
- [ ] Color swatch + button: Custom active-tab color (enabled only when color mode is Custom).
- [ ] Color swatch + button: Custom inactive-tab color (enabled only when color mode is Custom).
- [ ] Numeric spinner: Tab height in pixels (20–48 px, default 28 px).

### Settings dialog — Behavior tab

- [ ] Checkbox: "Auto-hide tab bar when window is not focused" (default: false).
- [ ] Slider: Auto-hide delay in milliseconds (250–2000 ms, default 500 ms).
- [ ] Checkbox: "Show peek preview on hover" (default: true).
- [ ] Slider: Peek delay in milliseconds (200–2000 ms, default 400 ms).
- [ ] Checkbox: "Keep groups across restarts" (default: false, future feature — can be greyed out).

### Keyboard shortcuts tab (Appearance tab or separate)

- [ ] Hotkey picker: "Cycle tabs forward" (default: none).
- [ ] Hotkey picker: "Cycle tabs backward" (default: none).
- [ ] Hotkey picker: "Detach current tab" (default: none).

### Persistence

- [ ] Settings are stored at `%APPDATA%\WinTab\config.json` using JSON (hand-rolled or `serde_json` if added as a dependency).
- [ ] The directory is created on first save if it does not exist.
- [ ] Settings are loaded at startup before `AppState::init()` runs so overlays are created with the correct values.
- [ ] Clicking OK or Apply saves immediately; Cancel reverts in-memory state to the last persisted snapshot.

### Live apply

- [ ] Changes to any Appearance setting trigger an immediate repaint of all existing overlays without closing the dialog.
- [ ] Changing tab position (top/bottom) repositions all active overlays immediately.
- [ ] Changing opacity sliders updates `UpdateLayeredWindow` alpha on all overlays within one render cycle.
- [ ] Enabling/disabling auto-hide restarts the relevant timer logic in `state.rs`.

### About dialog

- [ ] Displays application name, version (read from `Cargo.toml` via `env!("CARGO_PKG_VERSION")`), and a one-line description.
- [ ] Contains a single OK button to dismiss.

## Suggested Implementation

### Config struct (`src/config.rs`)

```rust
#[derive(Clone)]
pub struct Config {
    pub start_with_windows: bool,
    pub tab_position: TabPosition,   // enum: Top, Bottom
    pub show_close_button: bool,
    pub show_new_tab_button: bool,

    pub active_opacity: u8,          // 0–255
    pub inactive_opacity: u8,
    pub color_mode: ColorMode,       // enum: SystemAccent, Custom, Dark, Light
    pub custom_active_color: u32,    // 0x00RRGGBB
    pub custom_inactive_color: u32,

    pub tab_height: i32,             // pixels

    pub auto_hide: bool,
    pub auto_hide_delay_ms: u32,
    pub peek_enabled: bool,
    pub peek_delay_ms: u32,

    pub hotkey_cycle_forward: Option<HotkeyDef>,
    pub hotkey_cycle_backward: Option<HotkeyDef>,
    pub hotkey_detach: Option<HotkeyDef>,
}

impl Default for Config { ... }

pub fn load() -> Config { ... }   // reads JSON; falls back to Default on any error
pub fn save(cfg: &Config) { ... } // writes JSON; silently ignores write errors
```

JSON serialization should be hand-written (a simple key/value flat object) to avoid pulling in `serde` unless the team decides to add it. The format should be stable across minor version bumps, with all fields optional in parsing so that missing keys fall back to defaults.

### AppState integration

Add `pub config: Config` to `AppState` and initialize it before `init()`:

```rust
// In thread_local! initializer or main() before init():
let config = config::load();
STATE.with(|cell| {
    let mut s = cell.borrow_mut();
    s.config = config;
    s.init();
});
```

Pass `&state.config` into `overlay::update_overlay()` and `overlay::update_peek_overlay()` so rendering reads dynamic values instead of module-level constants. Alternatively, expose a module-level `fn current_config() -> Config` that calls `state::with_state(|s| s.config.clone())` — simpler to thread through the existing call sites.

### Dialog window (`src/dialog.rs`)

Use a Win32 property sheet (`PropertySheetW`) with three `PROPSHEETPAGEW` entries (General, Appearance, Behavior). Each page is a dialog resource defined inline via `DLGTEMPLATE` built in Rust (no `.rc` file needed — see the pattern already used in `overlay.rs` for window creation without resources). Alternatively, use a single `WS_TABSTOP` custom window with a tab control (`WC_TABCONTROL`) and manually swap child panels — this avoids the complexity of property-sheet activation.

Track the dialog HWND in a module-level `static` (wrapped in a `Mutex<Option<HWND>>`) so `tray.rs` can check whether the dialog is already open:

```rust
static DIALOG_HWND: std::sync::Mutex<Option<HWND>> = std::sync::Mutex::new(None);

pub fn open_settings(parent: HWND) {
    let guard = DIALOG_HWND.lock().unwrap();
    if let Some(hwnd) = *guard {
        // Bring existing window to front
        unsafe { SetForegroundWindow(hwnd); }
        return;
    }
    drop(guard);
    // Create dialog on a separate thread if modeless,
    // or call DialogBoxW for a modal dialog
}
```

A modeless dialog (`CreateDialogW`) is preferred because it lets the tray message loop keep running. The dialog's message loop is handled by `IsDialogMessage` inserted into the main message loop in `main.rs`.

### Overlay rendering changes

Replace the constants at the top of `overlay.rs`:

```rust
// Before
const COLOR_ACTIVE: u32 = 0x00A06030;
const TAB_HEIGHT: i32 = 28;

// After — resolved at render time
fn resolved_color_active(cfg: &Config) -> u32 { ... }
fn resolved_tab_height(cfg: &Config) -> i32 { cfg.tab_height }
```

Pass a `Config` snapshot into `draw_tabs()` so a single render call is always consistent. Store a `Config` snapshot taken at dialog open time for the Cancel revert path.

### Startup registry entry

```rust
fn set_autostart(enable: bool) {
    // HKCU\Software\Microsoft\Windows\CurrentVersion\Run
    // Value name: "WinTab"
    // Value data: path to current executable (std::env::current_exe())
}
```

Call `set_autostart(config.start_with_windows)` on every Save.

### Apply on change

For live preview in the dialog, post a custom `WM_APP + 1` message to the main message HWND whenever a control changes. The main window proc calls `state::with_state(|s| s.overlays.update_all(...))`. This keeps all overlay mutation on the main thread and avoids any cross-thread state access.

## Edge Cases

- **Dialog already open**: The `DIALOG_HWND` static must be cleared when the dialog is destroyed (`WM_DESTROY` handler sets it to `None`). If the process crashes between opening and closing the dialog, the static is in-process so it resets on restart automatically.

- **Config file corruption**: `load()` must treat any parse error (malformed JSON, unexpected types, values out of range) as a full reset to `Default::default()`. Log the error to `OutputDebugStringW` but never surface it as a fatal error. Write a clean default file after corruption is detected so the next launch is clean.

- **Out-of-range values**: Clamp all numeric fields on load. `tab_height` below 20 or above 48 should be clamped silently. Opacity values outside 0–255 should be clamped.

- **Missing `%APPDATA%` directory**: `%APPDATA%\WinTab\` may not exist. Call `CreateDirectoryW` before any write; check `ERROR_ALREADY_EXISTS` is acceptable.

- **Version migration**: When new fields are added in a later version, missing JSON keys must fall back to the field's default value, not cause a parse failure. Document the current schema version in the JSON as a `"version": 1` key so future migrations can be gated.

- **Cancel after Apply**: If the user clicks Apply (which persists to disk and repaints overlays) and then Cancel, only the in-memory state is reverted from the pre-dialog snapshot. The file on disk is already updated. A true Cancel-after-Apply should either re-save the snapshot or warn the user. Simplest safe behavior: disable Apply until a control changes, and treat Apply as a commit that prevents Cancel from reverting past it.

- **Config save failure**: A read-only filesystem, full disk, or permission error on `%APPDATA%` should not crash the process. Wrap `save()` in error handling and optionally show a `MessageBoxW` warning once.

- **Multiple monitors / DPI**: The dialog should be DPI-aware. Call `SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)` before creating the dialog window so controls scale correctly on high-DPI displays. The tab height slider preview should show pixel values relative to the display's DPI.

- **Hotkey conflicts**: When registering global hotkeys via `RegisterHotKey`, check the return value. If `ERROR_HOTKEY_ALREADY_REGISTERED` is returned, show an inline error in the dialog and clear the hotkey field rather than silently failing.

- **Enable/Disable state during dialog**: If the user disables WinTab via the tray menu while the Settings dialog is open, the dialog should remain functional. Settings apply once WinTab is re-enabled. The live-apply path should check `state.enabled` before repainting overlays.
