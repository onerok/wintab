# Remember Last Position and Size

## Problem

WinTab currently has no memory between sessions. When a user closes and relaunches WinTab, or when a managed window closes and is reopened, all windows reappear wherever the OS placed them — none of the deliberate positioning users established while WinTab was running is preserved.

This is particularly disruptive for users who have arranged specific windows or groups to specific monitor quadrants, or who use virtual desktops to separate workspaces. Every WinTab restart requires manually re-positioning all windows and re-creating all groups. The more monitors and virtual desktops a user works with, the worse this gets.

For groups the problem compounds: because `TabGroup::add` and `GroupManager::create_group` move all tabs to the active window's position, the moment a grouped window is closed and reopened it defaults to the OS-chosen position, which can be on the wrong monitor or buried behind other windows.

## Location

**Existing files to modify:**

- `src/state.rs` — `AppState`, `on_window_created`, `on_window_moved`, `shutdown`
- `src/window.rs` — `WindowInfo`, `is_eligible`
- `src/group.rs` — `GroupManager::create_group`, `TabGroup::add`

**New files to create:**

- `src/position_store.rs` — persistence layer: key derivation, load/save, matching logic
- `data/positions.json` — runtime-generated storage file (not checked in; lives beside the binary or in `%APPDATA%\WinTab\`)

## Requirements

- [ ] When a managed window moves or resizes (`on_window_moved`), its current DWM extended frame rect is saved to the position store, keyed by a stable identity derived from the window (see Suggested Implementation).
- [ ] When a managed window is created (`on_window_created`) and successfully passes `is_eligible`, the store is queried for a matching entry; if found, the window is repositioned to the stored rect before being added to `AppState::windows`.
- [ ] Position data is persisted to disk so it survives WinTab restarts. The store is loaded once at startup (`AppState::init`) and flushed to disk on clean shutdown (`AppState::shutdown`) and periodically (every N move events, e.g. every 20).
- [ ] The persistence file is stored in `%APPDATA%\WinTab\positions.json`, created on first write with any intermediate directories.
- [ ] Saved rects use logical (DPI-unscaled) coordinates consistent with what `get_window_rect` already returns via `DWMWA_EXTENDED_FRAME_BOUNDS`.
- [ ] For grouped windows, the group's shared rect is also saved — keyed by a stable group identity (sorted list of member window keys). On WinTab startup, if all windows that formed a prior group are present, they are repositioned as a unit to the stored group rect.
- [ ] Virtual desktop tracking: the virtual desktop GUID of the active window is recorded alongside each position entry using `IVirtualDesktopManager::GetWindowDesktopId`. On restore, if the target desktop can be identified and differs from the window's current desktop, WinTab attempts to move the window to the correct desktop via `IVirtualDesktopManager::MoveWindowToDesktopById`.
- [ ] Virtual desktop support degrades gracefully: if `IVirtualDesktopManager` is unavailable (COM init failure, future OS changes) or returns `E_INVALIDARG`, the desktop GUID field is omitted and position-only restore proceeds normally.
- [ ] Multi-monitor: stored coordinates are monitor-relative. On restore, the code checks whether the stored rect's monitor still exists (`MonitorFromRect` with `MONITOR_DEFAULTTONULL`). If the monitor is absent, the window is not repositioned (OS default placement is accepted).
- [ ] Stale entries are evicted: any entry not matched by a window during a session is retained for up to 30 days, after which it is pruned on next save.
- [ ] The position store is capped at 500 entries to prevent unbounded growth. If the cap is exceeded, the oldest entries (by `last_seen` timestamp) are evicted first.
- [ ] Matching is fuzzy on title: an exact process-name + window-class match with a title edit distance within a configurable threshold (default: ≤ 20% of the shorter title length) is accepted. Exact-title matches are preferred over fuzzy matches.
- [ ] No unsafe code is introduced in `position_store.rs` itself; all Win32 calls are isolated to thin helper functions that return `Option`/`Result`.
- [ ] Unit tests cover: key derivation, fuzzy title matching, entry eviction by age and cap, monitor-presence check logic (mocked), and JSON round-trip serialization.

## Suggested Implementation

### Matching Key

Each saved position entry is identified by a `WindowKey`:

```rust
struct WindowKey {
    process_name: String,   // e.g. "code.exe"  — from GetModuleFileNameExW on window PID
    class_name: String,     // e.g. "Chrome_WidgetWin_1"  — from GetClassNameW
    title_normalized: String, // lowercase, whitespace-collapsed title at time of save
}
```

Derive a stable string key by joining fields with `\x1f` (ASCII unit separator):

```
"code.exe\x1fChrome_WidgetWin_1\x1fvisual studio code"
```

On restore, a candidate `WindowKey` from the newly created window is looked up with the following priority:
1. Exact match on all three fields.
2. Exact `process_name` + `class_name`, with Levenshtein distance on `title_normalized` ≤ 20% of `min(stored_len, candidate_len)`.
3. No match — skip restore.

`process_name` is extracted by calling `GetWindowThreadProcessId` to get the PID, then `OpenProcess` + `GetModuleFileNameExW` to get the executable path, then taking the file name component.

### Storage Format

`%APPDATA%\WinTab\positions.json`:

```json
{
  "version": 1,
  "entries": [
    {
      "key": "code.exe\u001fChrome_WidgetWin_1\u001fvisual studio code",
      "rect": { "left": 100, "top": 50, "right": 1820, "bottom": 1030 },
      "virtual_desktop_id": "550e8400-e29b-41d4-a716-446655440000",
      "last_seen": "2026-02-28T09:15:00Z",
      "hit_count": 12
    }
  ],
  "group_entries": [
    {
      "member_keys": [
        "code.exe\u001fChrome_WidgetWin_1\u001fvisual studio code",
        "firefox.exe\u001fMozillaWindowClass\u001fmdn web docs"
      ],
      "rect": { "left": 100, "top": 50, "right": 1820, "bottom": 1030 },
      "virtual_desktop_id": "550e8400-e29b-41d4-a716-446655440000",
      "last_seen": "2026-02-28T09:15:00Z"
    }
  ]
}
```

Parse and emit this with `serde_json`. Add `serde` and `serde_json` to `Cargo.toml` (both are pure Rust, no build script needed). Keep `windows-sys` as the only native dependency.

### When to Save

- **On move/resize**: in `AppState::on_window_moved`, after `info.refresh_rect()` succeeds, call `position_store.record(key, rect, desktop_guid)`. Batch dirty writes — flush to disk only when `dirty_count >= 20` or on shutdown.
- **On group position change**: in `TabGroup::sync_positions` (or the `on_window_moved` path that calls it), also record the group entry.
- **On shutdown**: `AppState::shutdown` calls `position_store.flush()` unconditionally.

### When to Restore

In `AppState::on_window_created`, after `WindowInfo::from_hwnd(hwnd)` returns `Some(info)` and before inserting into `self.windows`:

```rust
if let Some(saved) = self.position_store.lookup(&key_for(&info)) {
    if monitor_exists(saved.rect) {
        reposition(hwnd, saved.rect);  // SetWindowPos
    }
    if let Some(desktop_id) = saved.virtual_desktop_id {
        move_to_desktop(hwnd, desktop_id);  // IVirtualDesktopManager
    }
}
```

### Virtual Desktop API

Use COM via `windows-sys`. `IVirtualDesktopManager` is in `Windows.Win32.UI.Shell`:

```rust
// Feature gates needed in Cargo.toml:
// "Win32_UI_Shell", "Win32_System_Com"

CoInitializeEx(null(), COINIT_APARTMENTTHREADED);
let vdm: IVirtualDesktopManager = CoCreateInstance(&VirtualDesktopManager, ...);
let mut guid = GUID::default();
vdm.GetWindowDesktopId(hwnd, &mut guid);  // returns S_OK or E_INVALIDARG
vdm.MoveWindowToDesktopById(hwnd, &guid);
```

Wrap this in a `VirtualDesktopHelper` struct that holds the COM object behind an `Option`; construction failures leave it `None` and all callers degrade gracefully.

### New Module Skeleton

```
src/position_store.rs
  pub struct PositionStore { entries: HashMap<String, PositionEntry>, dirty: usize }
  pub struct PositionEntry { rect: RECT, virtual_desktop_id: Option<GUID>, last_seen: SystemTime, hit_count: u32 }
  impl PositionStore
    pub fn load(path: &Path) -> Self
    pub fn flush(&mut self, path: &Path) -> io::Result<()>
    pub fn record(&mut self, key: &str, rect: RECT, desktop: Option<GUID>)
    pub fn lookup(&self, key: &str, title: &str) -> Option<&PositionEntry>
    fn evict_old_and_cap(&mut self)
  pub fn key_for(process_name: &str, class_name: &str, title: &str) -> String
  pub fn fuzzy_title_match(stored: &str, candidate: &str) -> bool
```

Add `pub position_store: PositionStore` to `AppState`.

## Edge Cases

- **Monitor configuration changes between sessions**: Stored coordinates may fall entirely off-screen if a monitor is disconnected. The `MonitorFromRect(..., MONITOR_DEFAULTTONULL)` check returns `NULL` for out-of-bounds rects; treat `NULL` as "monitor absent" and skip restore. Do not use `MONITOR_DEFAULTTONEAREST` here, as silently snapping to the wrong monitor is more confusing than no restore.

- **DPI scaling changes**: `DWMWA_EXTENDED_FRAME_BOUNDS` returns physical pixels on high-DPI displays. If the user changes display scaling between sessions, stored pixel coordinates may map to the wrong logical position. Store DPI alongside the rect (`GetDpiForWindow`) and, on restore, scale the stored rect by `new_dpi / stored_dpi` before applying it.

- **Title churn in dynamic apps**: Browser titles change on every page navigation; a tab that was "MDN Web Docs" becomes "GitHub". The fuzzy match threshold of 20% handles minor changes but not complete replacements. Consider also matching on `process_name` + `class_name` alone when no title match is found, accepting the most-recently-seen entry for that process/class pair. Make this opt-in via a config flag to avoid false positives for multi-window apps like VS Code.

- **Multiple instances of the same app**: Two VS Code windows produce identical `WindowKey` values. The store must disambiguate: when multiple windows match the same key, assign saved positions in the order windows appear (first match gets the closest-rect saved entry by Euclidean distance from current position, or most recently saved if no current rect is meaningful yet).

- **`IVirtualDesktopManager` reliability**: The COM interface is undocumented and has changed across Windows builds. `GetWindowDesktopId` returns `E_INVALIDARG` for windows on other desktops; `MoveWindowToDesktopById` is not available on all builds. Wrap every COM call in a result check; never propagate COM errors to the caller.

- **Hidden group members**: In a `TabGroup`, only the active window is visible; the rest have `SW_HIDE`. When `on_window_moved` fires for the active window and `sync_positions` moves hidden windows, those hidden windows will also trigger `WM_WINDOWPOSCHANGED` events re-entrantly (filtered by `suppress_events`). The position save must also be suppressed during `sync_positions` to avoid recording the programmatic moves as user intent. Gate the save inside `on_window_moved` behind `!self.suppress_events`.

- **Race between restore and first paint**: `SetWindowPos` is called before the application finishes initializing. Some applications (notably Electron apps) reposition themselves during startup after receiving `WM_CREATE`. Defer the restore by one message-loop iteration using `PostMessage(hwnd, WM_APP_RESTORE_POSITION, ...)` with a custom message, handled in the main message loop after the app has settled.

- **Stale group entries**: If only some members of a saved group are present (e.g., one was uninstalled), do not partially restore the group rect — apply it only if all member keys resolve to live windows. Partial matches should fall back to individual window position restore.

- **File I/O failures**: The persistence file path (`%APPDATA%\WinTab\`) may be unavailable on locked-down systems. All load/flush operations must be infallible from the caller's perspective: wrap in `Result`, log errors to `OutputDebugStringW`, and continue without crashing. A failed load starts with an empty store; a failed flush is retried on the next dirty threshold or shutdown.
