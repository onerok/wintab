# Bugs
- [ ] Tabs remain visible when switching virtual desktops (desktop-switch bug)





# WinTab Release Milestones

> MVP (M1 + M2) is complete: core tab engine, window grouping, drag-and-drop, peek.
> Below is the go-forward plan ranked by importance.

---

## M3: Stability & Core UX (High Priority)

Bug fixes and small UX wins that make the MVP feel solid before adding features.

| # | Item | Type | PBI | Effort | Status |
|---|------|------|-----|--------|--------|
| 1 | [Hide tabs when switching virtual desktops](hide-tabs-desktop-switch.md) | Bug | Full | Small | Done |
| 2 | [Peek z-order awareness](peek-zorder-awareness.md) | Bug | Full | Small | Done |
| 3 | [Show full title on hover](show-full-title-on-hover.md) | Feature | Full | Small | Done |

**Why first:** The desktop-switch bug is the most visible defect — overlays bleed across desktops. Peek z-order is a correctness fix. Title tooltips are cheap and high-value polish.

---

## M4: YAML Configuration (Medium-High Priority)

Barebones file-based config. No UI — users edit `%APPDATA%\WinTab\config.yaml` directly.

| # | Item | Type | PBI | Effort | Status |
|---|------|------|-----|--------|--------|
| 4 | YAML config file with rules engine | Feature | Full | Small | Done |

**What's included:** `config.yaml` in `%APPDATA%\WinTab\` with auto-grouping rules (match by process_name, class_name, title; operators: equals, contains, starts_with, ends_with, regex; match modes: all/any). Loaded at startup, graceful fallback on missing/invalid file.

---

## M5: Polish & Interactions (Medium Priority)

Rich interactions that make tab groups genuinely powerful.

| # | Item | Type | PBI | Effort | Status |
|---|------|------|-----|--------|--------|
| 5 | [Tab preview on hover](preview-on-hover.md) | Feature | Full | Medium | Done |
| 6 | [Remember last position and size](remember-position-size.md) | Feature | Full | Medium | Done |

**Why this order:** Preview is high-impact UX that leverages DWM thumbnails (no heavy lifting). Position memory builds on YAML config persistence and virtual desktop COM APIs already introduced in M3.

---

## M6: Automation & Rules Engine (Lower Priority)

Power-user features for automatic workflow setup.

| # | Item | Type | PBI | Effort | Status |
|---|------|------|-----|--------|--------|
| 7 | [Automatic groups (rules engine)](automatic-groups.md) | Feature | Full | Large | Done |

**Status:** Rules engine implemented in `config.rs`, integrated into `state.rs` via `apply_rules()`. Windows matched on creation and added to named groups automatically.

---

## Future (Not Yet Scheduled)

From spec sections not yet in backlog. To be groomed as earlier milestones land.

- Keyboard shortcuts (configurable hotkeys via `RegisterHotKey`; spec defines 16 shortcuts including tab nav, move, ungroup, rename)
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
- Config file watcher for live reload on external changes
- Config UI (system tray settings dialog, modeless window with tabs for General / Appearance / Behavior, live-apply without restart)

---

## Summary

| Milestone | Items | Dependencies | Effort | Status |
|-----------|-------|--------------|--------|--------|
| **M3: Stability** | 3 | None | Small | Done |
| **M4: YAML Config** | 1 | None | Small | Done |
| **M5: Polish** | 2 | M3 (vdesktop APIs) | Medium | Done |
| **M6: Automation** | 1 | M4 | Large | Done |
| **Future** | 23 | Varies | TBD | Todo |
