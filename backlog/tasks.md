# WinTab Release Milestones

> MVP (M1 + M2) is complete: core tab engine, window grouping, drag-and-drop, peek.
> Below is the go-forward plan ranked by importance.

---

## M3: Stability & Core UX (High Priority)

Bug fixes and small UX wins that make the MVP feel solid before adding features.

| # | Item | Type | PBI | Effort |
|---|------|------|-----|--------|
| 1 | [Hide tabs when switching virtual desktops](hide-tabs-desktop-switch.md) | Bug | Full | Small |
| 2 | [Peek z-order awareness](peek-zorder-awareness.md) | Bug | Full | Small |
| 3 | [Show full title on hover](show-full-title-on-hover.md) | Feature | Full | Small |

**Why first:** The desktop-switch bug is the most visible defect — overlays bleed across desktops. Peek z-order is a correctness fix. Title tooltips are cheap and high-value polish.

---

## M4: Configuration & Settings (Medium-High Priority)

Unlock user customization. Required foundation for everything in M5+.

| # | Item | Type | PBI | Effort |
|---|------|------|-----|--------|
| 4 | [Config UI (system tray + settings dialog)](config-ui.md) | Feature | Full | Large |

**Why here:** Every subsequent feature (opacity, colors, preview size, shortcuts, rules) needs a settings system. Building it now unblocks M5 and M6.

**Includes:** Expanded tray menu, modeless settings dialog (General / Appearance / Behavior tabs), JSON persistence in `%APPDATA%\WinTab\config.json`, live-apply without restart.

---

## M5: Polish & Interactions (Medium Priority)

Rich interactions that make tab groups genuinely powerful.

| # | Item | Type | PBI | Effort |
|---|------|------|-----|--------|
| 5 | [Tab preview on hover](preview-on-hover.md) | Feature | Full | Medium |
| 6 | [Remember last position and size](remember-position-size.md) | Feature | Full | Medium |

**Why this order:** Preview is high-impact UX that leverages DWM thumbnails (no heavy lifting). Position memory builds on the config persistence from M4 and needs virtual desktop COM APIs already introduced in M3.

---

## M6: Automation & Rules Engine (Lower Priority)

Power-user features for automatic workflow setup.

| # | Item | Type | PBI | Effort |
|---|------|------|-----|--------|
| 7 | [Automatic groups (rules engine)](automatic-groups.md) | Feature | Full | Large |

**Why last among planned items:** Requires config UI (M4) for rule editing, benefits from position memory (M5) for default group positions. Most complex feature — needs new `rules.rs` module, regex matching, JSON rule storage, integration with `on_window_created`.

---

## Future (Not Yet Scheduled)

From spec sections not yet in backlog. To be groomed as earlier milestones land.

- Keyboard shortcuts (configurable hotkeys via `RegisterHotKey`)
- Tab context menu (right-click: rename, close, close others, ungroup, blacklist)
- Tab reordering (drag within tab bar)
- Tab renaming (double-click inline edit)
- Tab close button (X on each tab)
- Tab coloring (system accent / per-app / custom)
- Whitelist/blacklist exception management
- Drag file over tab to switch
- Start with Windows (registry run key)
- Window picker tool (crosshair for rule creation)

---

## Summary

| Milestone | Items | Dependencies | Effort |
|-----------|-------|--------------|--------|
| **M3: Stability** | 3 | None | Small |
| **M4: Config** | 1 | None | Large |
| **M5: Polish** | 2 | M3 (vdesktop APIs), M4 (settings) | Medium |
| **M6: Automation** | 1 | M4, M5 | Large |
| **Future** | 10+ | M4-M6 | TBD |
