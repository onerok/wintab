# WinTab Release Milestones

> MVP (M1 + M2) is complete: core tab engine, window grouping, drag-and-drop, peek.
> Below is the go-forward plan ranked by importance.

---

## M3: Stability & Core UX — Done

Bug fixes and small UX wins that make the MVP feel solid before adding features.

| # | Item | Type | Effort | Status |
|---|------|------|--------|--------|
| 1 | Hide tabs when switching virtual desktops | Bug | Small | Done |
| 2 | Peek z-order awareness | Bug | Small | Done |
| 3 | Show full title on hover | Feature | Small | Done |

---

## M4: YAML Configuration — Done

Barebones file-based config. No UI — users edit `%APPDATA%\WinTab\config.yaml` directly.

| # | Item | Type | Effort | Status |
|---|------|------|--------|--------|
| 4 | YAML config file with rules engine | Feature | Small | Done |

---

## M5: Polish & Interactions — Done

Rich interactions that make tab groups genuinely powerful.

| # | Item | Type | Effort | Status |
|---|------|------|--------|--------|
| 5 | Tab preview on hover | Feature | Medium | Done |
| 6 | Remember last position and size | Feature | Medium | Done |

---

## M6: Automation & Rules Engine — Done

Power-user features for automatic workflow setup.

| # | Item | Type | Effort | Status |
|---|------|------|--------|--------|
| 7 | Automatic groups (rules engine) | Feature | Large | Done |

---

## Gaps — [implementation-gaps.md](implementation-gaps.md)

11 items identified from completed feature specs (tooltip freshness, preview config, position store desktop tracking, rules engine negation operators, etc.).

---

## Future (Not Yet Scheduled)

- [Config UI](config-ui.md) (system tray settings dialog, modeless window with tabs for General / Appearance / Behavior, live-apply without restart)
- Keyboard shortcuts (configurable hotkeys via `RegisterHotKey`)
- Tab context menu (right-click: rename, close, close others, close all, ungroup, ungroup all, never tab this app, edit group)
- Tab reordering (drag within tab bar)
- Tab renaming (double-click inline edit)
- Tab close button (X on each tab)
- New tab button (+) at end of tab bar
- Middle-click to close tab
- Tab coloring (system accent / per-app / custom)
- Tab position configuration (Top / Bottom)
- Whitelist/blacklist exception management
- Drag file over tab to switch
- Move tab between groups (drag from Group A → Group B)
- Auto-hide tabs at screen edge / maximized windows
- Prevent tabs hidden by overlapping windows (force z-order above adjacent windows)
- Start with Windows (registry run key)
- Check for updates
- Reset to defaults
- About dialog (version info, tray menu item)
- Tray icon left-click / double-click opens config dialog
- New tab from running windows (`Ctrl+Win+T`) — window picker/selector UI
- Window picker tool (crosshair for rule creation)

---

## Summary

| Milestone | Items | Status |
|-----------|-------|--------|
| **M3: Stability** | 3 | Done |
| **M4: YAML Config** | 1 | Done |
| **M5: Polish** | 2 | Done |
| **M6: Automation** | 1 | Done |
| **Gaps** | 11 | Todo |
| **Future** | 22+ | Todo |
