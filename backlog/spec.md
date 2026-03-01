# WinTab - Window Tabbing Manager for Windows

## Overview

WinTab is a Windows desktop utility that adds browser-style tabs to any application window. It allows users to group multiple windows into tabbed interfaces, reducing desktop clutter and improving multitasking workflows. Windows from any application can be dragged together into tab groups and managed as a single unit.

**Inspired by:** [TidyTabs by Nurgo Software](https://help.nurgo-software.com/category/51-getting-started)

**Target Platform:** Windows 10/11 (x64)

---

## Core Concepts

### Tab
A tab is a small clickable UI element rendered above (or at a configurable edge of) a window's title bar. Each managed window gets one tab. Tabs display the window's title text and application icon.

### Tab Group
A tab group is a collection of two or more windows merged into a single tabbed container. Only one window in the group is visible at a time (the "active tab"). Switching tabs shows/hides the corresponding windows. Grouped windows share the same screen position and size.

### Managed Window
Any top-level application window that WinTab attaches a tab to. Not all windows are eligible (e.g., tooltips, menus, system dialogs are excluded).

---

## Features

### 1. Tab Rendering & Display

#### 1.1 Tab Bar
- Render a horizontal tab bar above (default) the title bar of managed windows.
- Each tab shows: application icon + window title text (truncated with ellipsis if too long).
- The active tab is visually distinct (highlighted/raised).
- Tab bar auto-sizes based on the number of tabs.

#### 1.2 Transparency / Visibility States
- **Active window tabs:** Configurable opacity (default: 75%).
- **Inactive window tabs:** Configurable opacity (default: 25%).
- **Mouse hover:** Full opacity (100%).
- Single (ungrouped) tabs are hidden by default; they appear on mouse hover over the window's top edge.
- Tabs for windows snapped to the top screen edge or maximized are auto-hidden to avoid obscuring the title bar.

#### 1.3 Tab Buttons
- Optional **close button** (X) on each tab.
- Optional **new tab button** (+) at the end of the tab bar.

#### 1.4 Tab Colors
- Default: follow the Windows system accent color.
- Option: per-application color derived from the app icon's dominant color.
- Option: user-specified custom color.
- Option: no coloring (neutral/white).

---

### 2. Tab Interactions

#### 2.1 Creating a Tab Group
1. User hovers over an ungrouped window to reveal its tab.
2. User drags the tab onto another window's tab.
3. The two windows merge into a tab group. The dropped window becomes the active tab.

#### 2.2 Adding to an Existing Group
- Drag an ungrouped window's tab onto the tab bar of an existing group.
- The window joins the group and becomes the active tab.

#### 2.3 Removing a Tab from a Group
- Drag a tab away from the group and drop it on an empty area of the desktop.
- The window detaches and becomes an independent window again.

#### 2.4 Moving a Tab Between Groups
- Drag a tab from Group A and drop it onto the tab bar of Group B.
- The window leaves Group A and joins Group B.

#### 2.5 Reordering Tabs
- Drag a tab left or right within the tab bar.
- Other tabs shift to accommodate.

#### 2.6 Switching Tabs
- Click a tab to make it the active (visible) window.
- Keyboard shortcuts (see Section 7).

#### 2.7 Renaming a Tab
- Double-click a tab to enter inline edit mode.
- Alternatively, right-click > "Rename Tab".
- Custom names persist until the window is closed.

#### 2.8 Closing a Tab
- Click the close button (X) on the tab.
- Middle-click a tab to close it.
- Right-click context menu: "Close Tab", "Close Other Tabs", "Close All Tabs".
- Closing a tab closes its corresponding window.
- When a window is closed normally (e.g., Alt+F4), its tab is automatically removed.

#### 2.9 Tab Preview on Hover
- Hovering over an inactive tab shows a thumbnail preview of that window's content.
- Preview size is configurable (default: 300px wide).
- Preview opacity is configurable (default: 50%).

#### 2.10 Drag File Over Tab
- Dragging a file over an inactive tab switches to that tab after a short delay.
- Enables drag-and-drop file operations across grouped windows.

---

### 3. Window Management

#### 3.1 Synchronized Positioning
- All windows in a group share the same position and size.
- Moving or resizing the active window applies to all windows in the group.
- Minimizing/maximizing the active window affects the entire group.

#### 3.2 Z-Order
- Tabs render above adjacent windows to remain accessible.
- Option to prevent tabs from being hidden by overlapping windows.

#### 3.3 Focus Behavior
- Clicking any tab in a group brings the entire group to the foreground.
- The clicked tab becomes the active/visible window.

---

### 4. Automatic Grouping (Rules Engine)

#### 4.1 Group Definitions
- Users can define named groups with inclusion rules.
- When a new window opens and matches a group's rules, it is automatically added to that group.

#### 4.2 Rule Editor
Each rule matches windows based on one or more criteria:

| Field | Description | Example |
|---|---|---|
| **Process name** | Executable file name | `notepad.exe` |
| **Arguments** | Command-line arguments | `--profile=dev` |
| **Window title** | Title bar text | `*GitHub*` |
| **Class name** | Win32 window class | `ConsoleWindowClass` |

Each field supports comparison operators:
- Equals / Not Equals
- Starts With / Not Starts With
- Ends With / Not Ends With
- Contains / Not Contains
- Matches Regex / Not Matches Regex

Leave a field blank to match any value.

#### 4.3 Window Picker Tool
- A crosshair/target tool that the user can drag over any window.
- Automatically fills in the rule fields (process name, window title, class name) from the targeted window.

#### 4.4 Group Properties
- **Group name:** Display label.
- **Enabled:** Toggle on/off without deleting.
- **Default position:** Optional X, Y, Width, Height for where the group spawns.
- **Inclusion rules:** Ordered list of matching rules. Order determines tab sort order.
- Groups are evaluated in order; a window joins the first matching group.
- Groups can be reordered by dragging.

---

### 5. Exceptions (Whitelist / Blacklist)

#### 5.1 Blacklist
- Windows matching blacklist rules are never tabbed, even if WinTab would normally manage them.
- Quick blacklist: right-click a tab > "Never tab this application".

#### 5.2 Whitelist
- Windows matching whitelist rules are always tabbed, even if WinTab would normally skip them.
- Useful for forcing tabs on non-standard or custom windows.

#### 5.3 Auto-Detection
- WinTab should automatically exclude unsuitable windows:
  - Tool windows, tooltips, menus, popups.
  - Windows without a title bar.
  - Very small windows (below a configurable threshold).
  - Child/owned windows.

---

### 6. Configuration

#### 6.1 General
| Setting | Default | Description |
|---|---|---|
| Start with Windows | On | Launch WinTab at system startup |
| Check for updates | On | Periodic update checks |
| Show system tray icon | On | Display icon in notification area |
| Reset to defaults | - | Restore all settings to factory |

#### 6.2 Appearance
| Setting | Default | Description |
|---|---|---|
| Tab position | Top | Where tabs render (Top / Bottom) |
| Active tab opacity | 75% | Opacity for the focused group's tabs |
| Inactive tab opacity | 25% | Opacity for unfocused groups' tabs |
| Hover opacity | 100% | Opacity when mouse is over tab bar |
| Tab color mode | System | System / Per-App / Custom / None |
| Show close button | On | Display X on tabs |
| Show new tab button | Off | Display + at end of tab bar |
| Preview size | 300px | Hover preview thumbnail width |
| Preview opacity | 50% | Hover preview transparency |

#### 6.3 Behavior
| Setting | Default | Description |
|---|---|---|
| Auto-hide single tabs | On | Hide tabs for ungrouped windows |
| Auto-hide at screen edge | On | Hide tabs for top-snapped/maximized windows |
| Prevent tabs hidden by overlapping windows | Off | Force tabs to render above adjacent windows |
| Show tooltip for long names | On | Full title tooltip on truncated tabs |
| Show preview on hover | On | Thumbnail preview on tab hover |
| Allow tab reordering | On | Drag to reorder tabs |
| Double-click to rename | On | Inline rename on double-click |
| Middle-click to close | On | Close tab/window on middle-click |
| Drag file to select tab | On | Switch tabs when dragging files over them |

---

### 7. Keyboard Shortcuts

All shortcuts are configurable. Defaults:

| Action | Default Shortcut |
|---|---|
| Previous tab | `Ctrl+Win+PgUp` |
| Next tab | `Ctrl+Win+PgDown` |
| First tab | `Ctrl+Win+Home` |
| Last tab | `Ctrl+Win+End` |
| Rename current tab | `Win+F2` |
| New tab (from running windows) | `Ctrl+Win+T` |
| Close current tab | `Ctrl+Win+W` |
| Close other tabs | `Ctrl+Shift+Win+W` |
| Close all tabs | `Ctrl+Alt+Win+W` |
| Add ungrouped windows to group | `Ctrl+Win+=` |
| Ungroup current tab | `Ctrl+Win+-` |
| Ungroup all tabs | `Ctrl+Shift+Win+-` |
| Move tab left | `Ctrl+Alt+Win+PgUp` |
| Move tab right | `Ctrl+Alt+Win+PgDown` |
| Move group to default position | `Ctrl+Win+I` |

---

### 8. System Tray

- WinTab runs as a background process with a system tray icon.
- Left-click or double-click the tray icon opens the configuration dialog.
- Right-click the tray icon shows a context menu:
  - **Settings** - open configuration dialog.
  - **Disable** - temporarily stop all tabbing.
  - **About** - version info.
  - **Exit** - quit WinTab.

---

### 9. Tab Context Menu

Right-clicking any tab shows:

| Item | Description |
|---|---|
| Rename Tab | Enter custom name for this tab |
| Close Tab | Close this tab and its window |
| Close Other Tabs | Close all tabs except this one |
| Close All Tabs | Close all tabs in the group |
| Ungroup This Tab | Detach from the group |
| Ungroup All | Dissolve the entire group |
| Never Tab This Application | Add to blacklist |
| Edit Group | Open group editor (for auto-groups) |

---

## Technical Architecture

### Technology Stack
- **Language:** Rust
- **UI Framework:** Raw Win32 API (`windows-sys 0.59` crate) for tab rendering (lightweight, no heavy UI framework)
- **Window Management:** Win32 API (`SetWinEventHook` with `WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS` — no DLL injection)
- **Configuration Storage:** YAML files in `%APPDATA%\WinTab\` (`serde_yaml`)
- **Build System:** Cargo

### Key Implementation Details

#### Window Hooking
- Use `SetWinEventHook` to monitor window creation, destruction, focus changes, title changes, and move/resize events.
- Enumerate existing windows on startup to attach tabs to already-open windows.

#### Tab Rendering
- Create a small layered window (`WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST`) positioned above each managed window's title bar.
- The tab window is owned by WinTab, not the target application.
- Use `WS_EX_TOOLWINDOW` to keep tab windows out of the taskbar and Alt+Tab.
- Render tabs using 32-bit ARGB DIB + `UpdateLayeredWindow` with `ULW_ALPHA`. GDI text/icon rendering with `fix_gdi_alpha()` to patch alpha channel.

#### Window Grouping
- When windows are grouped, hide all but the active tab's window using `ShowWindow(SW_HIDE)`.
- Synchronize position/size by listening to `EVENT_OBJECT_LOCATIONCHANGE` on the active window and applying the same bounds to hidden windows before showing them.
- Handle `WM_ACTIVATE` and `WM_SETFOCUS` to manage group focus behavior.

#### Drag and Drop
- Implement custom drag-and-drop for tab rearrangement using mouse capture (`SetCapture`).
- Show a semi-transparent preview of the dragged tab during the drag operation.
- Detect drop targets (other tab bars, empty desktop areas) using hit-testing.

#### Performance
- Minimal CPU usage: only respond to window events, no polling.
- Minimal memory: store only metadata (HWND, title, icon, group ID) per managed window.
- Tab rendering only repaints on changes (title update, focus change, hover).

#### Configuration Persistence
- Config and auto-grouping rules stored in `%APPDATA%\WinTab\config.yaml`.
- Window positions stored in `%APPDATA%\WinTab\positions.yaml`.
- File watcher for live reload is a future enhancement.

---

## Non-Goals (Out of Scope for V1)

- Tab pinning
- Tab session save/restore across reboots
- Tab grouping across virtual desktops
- Plugin/extension system
- Theming engine (beyond basic color configuration)
- Tabs for UWP/Store apps (if technically infeasible)
- Multi-monitor tab dragging between monitors

---

## Milestones

### M1+M2: MVP — Done
- Window enumeration and event hooking (`SetWinEventHook`, 9 event types)
- Tab rendering above managed windows (GDI layered overlays)
- Tab visibility states (hover reveal, opacity)
- Basic window filtering (`is_eligible()`)
- Drag-and-drop to create/merge/detach groups
- Tab switching (show/hide windows)
- Group position/size synchronization
- Peek overlay for ungrouped windows
- System tray icon (Enable/Disable, Exit)

### M3: Stability & Core UX — Done
- Hide tabs when switching virtual desktops (COM `IVirtualDesktopManager`)
- Peek z-order awareness (`WindowFromPoint` + `GetAncestor`)
- Show full title on hover (Win32 `TOOLTIPS_CLASS`)

### M4: YAML Configuration — Done
- `%APPDATA%\WinTab\config.yaml` with auto-grouping rules
- Match by process_name, class_name, title; operators: equals, contains, starts_with, ends_with, regex

### M5: Polish — Done
- Tab preview on hover (DWM Thumbnail API)
- Remember window position and size (`positions.yaml` persistence)

### M6: Automation — Done
- Rules engine in `config.rs`, integrated via `apply_rules()` in `state.rs`
- Pending singleton → group creation → group extension lifecycle

### Future
- See [tasks.md](tasks.md) and [implementation-gaps.md](implementation-gaps.md)
