# WinTab MVP Implementation Plan

## Goal
Minimal viable product: group windows into tabs, switch between them by clicking tabs. No configuration UI, no rules engine, no previews, no polish.

---

## What's IN the MVP

| Feature | Scope |
|---|---|
| Window discovery | EnumWindows on startup + SetWinEventHook for new/destroyed windows |
| Window filtering | Exclude popups, tooltips, tool windows, child windows, tiny windows |
| Tab overlay | One layered window per tab group, rendered with GDI (not Direct2D) |
| Tab display | App icon + truncated title per tab, active tab highlighted |
| Tab click | Click a tab to switch active window in the group |
| Tab grouping | Drag a tab onto another tab to create/join a group |
| Tab ungrouping | Drag a tab away from the group to detach |
| Group sync | All grouped windows share position/size; move/resize propagates |
| Show/hide | Only the active tab's window is visible; others are hidden |
| System tray | Icon with right-click menu: Disable / Exit |
| Single-tab hide | Ungrouped windows have no visible tab (hover to reveal) |

## What's NOT in the MVP

- Configuration dialog / settings UI
- Tab preview thumbnails (DWM Thumbnail API)
- Automatic grouping rules engine
- Whitelist / blacklist
- Tab renaming, coloring, close button, new tab button
- Tab context menu (right-click)
- Keyboard shortcuts
- Tab reordering within a group
- Drag file over tab to switch
- Opacity configuration (use hardcoded values)
- Start with Windows / auto-update
- Config file persistence

---

## Architecture

```
wintab.exe (single process, no DLL injection)
│
├── main.rs          — Entry point, message loop, tray icon
├── hook.rs          — SetWinEventHook setup + event dispatch
├── window.rs        — Window enumeration, filtering, metadata (HWND, title, icon, rect)
├── group.rs         — Tab group data model + show/hide/sync logic
├── overlay.rs       — Layered window creation, GDI tab rendering, hit testing
├── drag.rs          — Tab drag-and-drop (SetCapture-based)
└── tray.rs          — System tray icon + context menu
```

**State management:** Single `AppState` struct behind `RefCell` (single-threaded Win32 message loop — no need for `Mutex`). Stored in a static or thread-local, accessed from window procs and hook callbacks.

---

## Implementation Phases

### Phase 0: Project Scaffold
- `cargo init --name wintab`
- Set up `Cargo.toml` with `windows` crate and required features
- `build.rs` for Windows manifest (DPI-aware, admin not required)
- Verify builds and runs as a no-op Windows GUI app (`#![windows_subsystem = "windows"]`)
- Set up `.gitignore`

### Phase 1: Window Discovery & Tracking
**Files:** `main.rs`, `window.rs`, `hook.rs`

1. **`window.rs`** — Window metadata and filtering
   - Struct `WindowInfo { hwnd, title, icon, exe_name, rect, class_name }`
   - `enumerate_windows()` → `Vec<WindowInfo>` using `EnumWindows`
   - `is_eligible(hwnd)` → `bool` — filter function:
     - Must be visible (`IsWindowVisible`)
     - Must be top-level (no owner, or owner is desktop)
     - Must have `WS_CAPTION` or a title bar
     - Exclude `WS_EX_TOOLWINDOW` without `WS_EX_APPWINDOW`
     - Exclude windows smaller than 100x50
     - Exclude WinTab's own windows
   - `get_window_icon(hwnd)` → `HICON` (send `WM_GETICON`, fallback to `GetClassLongPtr`)
   - `get_window_title(hwnd)` → `String`
   - `get_window_rect(hwnd)` → `RECT` (use `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`)

2. **`hook.rs`** — Window event monitoring
   - Call `SetWinEventHook` with `WINEVENT_OUTOFCONTEXT` for:
     - `EVENT_OBJECT_CREATE` — new window appeared
     - `EVENT_OBJECT_DESTROY` — window closed
     - `EVENT_OBJECT_NAMECHANGE` — title changed
     - `EVENT_OBJECT_LOCATIONCHANGE` — moved/resized
     - `EVENT_SYSTEM_FOREGROUND` — focus changed
     - `EVENT_OBJECT_SHOW` / `EVENT_OBJECT_HIDE` — visibility changed
   - Each event calls into `AppState` to update tracking

3. **`main.rs`** — Win32 message loop
   - Create hidden message-only window for dispatching
   - Initialize hooks
   - Enumerate existing windows
   - Run `GetMessage` / `DispatchMessage` loop

**Milestone:** App runs, logs discovered windows to debug output, tracks new/closed windows.

### Phase 2: Tab Overlay Rendering
**Files:** `overlay.rs`

1. **Overlay window creation**
   - One overlay window per managed window (or per group)
   - Window style: `WS_POPUP | WS_VISIBLE` with `WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT` (pass-through clicks on transparent areas)
   - Positioned directly above the managed window's title bar
   - Sized to fit the tab strip

2. **GDI rendering**
   - Use `UpdateLayeredWindow` with a 32-bit ARGB `HBITMAP`
   - Draw tab background (rounded rect, semi-transparent)
   - Draw app icon (16x16) using `DrawIconEx`
   - Draw title text using `DrawText` (truncated with ellipsis)
   - Active tab: solid background; inactive tabs: dimmer background

3. **Positioning logic**
   - On `EVENT_OBJECT_LOCATIONCHANGE` for the managed window, reposition the overlay
   - Calculate overlay position from managed window's `DWMWA_EXTENDED_FRAME_BOUNDS`
   - Tab bar height: 28px, positioned above the window's top edge

4. **Hit testing**
   - On `WM_LBUTTONDOWN` / `WM_LBUTTONUP` in the overlay, determine which tab was clicked
   - Map mouse position to tab index based on rendered tab widths

5. **Visibility**
   - Ungrouped windows: overlay hidden by default, shown on `WM_MOUSEMOVE` when cursor is near the window's top edge (use a tracking area / `TrackMouseEvent`)
   - Grouped windows: overlay always visible when the group's active window is in foreground
   - Opacity: hardcoded 75% active, 25% inactive, 100% on hover

**Milestone:** Tabs render above windows. They reposition correctly. Clicking has no effect yet.

### Phase 3: Tab Groups & Switching
**Files:** `group.rs`

1. **Data model**
   ```rust
   struct TabGroup {
       id: u64,
       tabs: Vec<HWND>,       // ordered list of windows
       active_index: usize,   // which tab is showing
   }
   ```
   - `AppState` holds: `HashMap<u64, TabGroup>` (groups) + `HashMap<HWND, u64>` (window → group mapping)

2. **Creating a group**
   - When two windows are merged (via drag, Phase 4), create a `TabGroup`
   - Hide all windows except the active one using `ShowWindow(SW_HIDE)`
   - Position all hidden windows to match the active window's rect

3. **Switching tabs**
   - On tab click: `ShowWindow(old_active, SW_HIDE)`, then `ShowWindow(new_active, SW_SHOW)`
   - Use `DeferWindowPos` to atomically position the newly shown window
   - Call `SetForegroundWindow` on the new active window

4. **Group sync (position/size)**
   - On `EVENT_OBJECT_LOCATIONCHANGE` for the active window in a group:
     - Get active window's current rect
     - Apply same rect to all hidden windows via `SetWindowPos` (with `SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOREDRAW`)
   - On minimize: minimize all in group. On restore: restore active, keep others hidden.

5. **Window destruction in a group**
   - On `EVENT_OBJECT_DESTROY` for a grouped window:
     - Remove from group
     - If it was active, switch to the next tab
     - If group has 1 window left, dissolve the group
   - Show the remaining window

6. **Overlay update**
   - After any group change, re-render the overlay for that group

**Milestone:** Can programmatically group windows (hardcode two HWNDs for testing). Click tabs to switch. Position syncs.

### Phase 4: Tab Drag & Drop
**Files:** `drag.rs`

1. **Drag initiation**
   - On `WM_LBUTTONDOWN` in overlay, record the tab index and start position
   - If mouse moves more than 5px before `WM_LBUTTONUP`, begin drag: `SetCapture`
   - Create a small floating "drag preview" window showing the dragged tab

2. **Drag feedback**
   - On `WM_MOUSEMOVE` during drag, move the drag preview window to follow the cursor
   - Check if cursor is over another tab overlay → highlight the drop target

3. **Drop handling**
   - **Drop on another tab/overlay:** Merge the dragged window into the target group (or create a new group if the target is ungrouped)
   - **Drop on empty space (no overlay under cursor):** Detach the window from its group (ungroup)
   - Release capture, destroy drag preview

4. **Drop target detection**
   - Use `WindowFromPoint` to find the window under cursor
   - Check if it's one of our overlay windows
   - If it's a managed (non-overlay) window, check if it's eligible and near its top edge

**Milestone:** Can drag tabs to group/ungroup windows. Full core loop works.

### Phase 5: System Tray
**Files:** `tray.rs`

1. **Tray icon**
   - Use `Shell_NotifyIcon` with `NIM_ADD` to add a tray icon
   - Use an embedded icon resource (simple "T" icon or similar)
   - Handle `WM_APP + 1` (or similar) for tray icon messages

2. **Context menu**
   - On right-click: `CreatePopupMenu` with:
     - "Disable" / "Enable" (toggle — pauses all hooking and hides overlays)
     - Separator
     - "Exit"
   - On "Exit": ungroup all windows (show all hidden), remove hooks, clean up, `PostQuitMessage`

3. **Graceful shutdown**
   - On exit or crash: ensure all hidden windows are shown (`ShowWindow(SW_SHOW)`)
   - Unhook all event hooks

**Milestone:** Full MVP functional. App runs from tray, can group/ungroup/switch tabs, exits cleanly.

---

## Cargo.toml

```toml
[package]
name = "wintab"
version = "0.1.0"
edition = "2021"

[dependencies.windows]
version = "0.61"
features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Accessibility",
    "Win32_UI_Shell",
    "Win32_Graphics_Gdi",
    "Win32_Graphics_Dwm",
    "Win32_System_LibraryLoader",
    "Win32_System_Threading",
]

[profile.release]
opt-level = "z"
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"
```

## Build & Run

```bash
cargo build                  # debug build
cargo run                    # run debug
cargo build --release        # optimized release build (~500KB-1MB)
```

---

## Key Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Rendering | GDI + `UpdateLayeredWindow` | Simpler than Direct2D; no COM init needed; sufficient for flat tab UI |
| Threading | Single-threaded message loop | Win32 hooks deliver to the installing thread; avoids all sync issues |
| State storage | `RefCell<AppState>` in thread-local | Safe for single-threaded; no `unsafe` needed for interior mutability |
| Overlay per... | Per managed window (ungrouped) or per group | Avoids creating overlays for all windows upfront; create on demand |
| Grouping mechanism | Drag-and-drop only | Simplest UX that matches the spec's core flow |
| No Direct2D | Yes | Saves ~200 lines of COM boilerplate; GDI handles icons + text + rounded rects fine |
| No config file | Yes | MVP uses hardcoded defaults; config comes in a later milestone |

---

## Risk Mitigations

| Risk | Mitigation |
|---|---|
| Hidden windows lost on crash | Register `SetUnhandledExceptionFilter` to show all hidden windows before crash |
| Window flicker on tab switch | Use `DeferWindowPos` for atomic positioning; `LockWindowUpdate` if needed |
| Admin windows can't be managed | Don't attempt to manage elevated windows; skip them in filtering |
| Per-monitor DPI | Use `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)` which returns physical pixels; mark app DPI-aware in manifest |
| AV false positive from hooking | Using `WINEVENT_OUTOFCONTEXT` (no DLL injection) avoids this entirely |
