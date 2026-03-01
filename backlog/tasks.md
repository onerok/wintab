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

## M4: Configuration & Settings (Medium-High Priority)

Unlock user customization. Required foundation for everything in M5+.

| # | Item | Type | PBI | Effort | Status |
|---|------|------|-----|--------|--------|
| 4 | [Config UI (system tray + settings dialog)](config-ui.md) | Feature | Full | Large | Todo |

**Why here:** Every subsequent feature (opacity, colors, preview size, shortcuts, rules) needs a settings system. Building it now unblocks M5 and M6.

**Includes:** Expanded tray menu, modeless settings dialog (General / Appearance / Behavior tabs), JSON persistence in `%APPDATA%\WinTab\config.json`, live-apply without restart.

---

## M5: Polish & Interactions (Medium Priority)

Rich interactions that make tab groups genuinely powerful.

| # | Item | Type | PBI | Effort | Status |
|---|------|------|-----|--------|--------|
| 5 | [Tab preview on hover](preview-on-hover.md) | Feature | Full | Medium | Todo |
| 6 | [Remember last position and size](remember-position-size.md) | Feature | Full | Medium | Todo |

**Why this order:** Preview is high-impact UX that leverages DWM thumbnails (no heavy lifting). Position memory builds on the config persistence from M4 and needs virtual desktop COM APIs already introduced in M3.

---

## M6: Automation & Rules Engine (Lower Priority)

Power-user features for automatic workflow setup.

| # | Item | Type | PBI | Effort | Status |
|---|------|------|-----|--------|--------|
| 7 | [Automatic groups (rules engine)](automatic-groups.md) | Feature | Full | Large | Todo |

**Why last among planned items:** Requires config UI (M4) for rule editing, benefits from position memory (M5) for default group positions. Most complex feature — needs new `rules.rs` module, regex matching, JSON rule storage, integration with `on_window_created`.

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

---

## Summary

| Milestone | Items | Dependencies | Effort | Status |
|-----------|-------|--------------|--------|--------|
| **M3: Stability** | 3 | None | Small | Done |
| **M4: Config** | 1 | None | Large | Todo |
| **M5: Polish** | 2 | M3 (vdesktop APIs), M4 (settings) | Medium | Todo |
| **M6: Automation** | 1 | M4, M5 | Large | Todo |
| **Future** | 22 | M4-M6 | TBD | Todo |
