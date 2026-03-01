# Implementation Gaps

Unimplemented items identified from completed feature specs. Grouped by area.

---

## Tooltip (Show Full Title on Hover)

| # | Item | Effort |
|---|------|--------|
| 1 | Call `TTM_UPDATE` on tooltip when `on_title_changed()` fires so tooltip text stays fresh | Small |
| 2 | Suppress tooltip during active drag (`is_dragging()` check in `TTN_GETDISPINFOW` handler) | Small |

---

## Preview on Hover

| # | Item | Effort |
|---|------|--------|
| 3 | Make preview width, max height, opacity, and delay configurable via `config.yaml` | Small |
| 4 | Hide preview in `sync_desktop_visibility()` (currently only hidden in `on_desktop_switch()`) | Small |

---

## Position Store (Remember Position/Size)

| # | Item | Effort |
|---|------|--------|
| 5 | Track virtual desktop GUID per entry via `IVirtualDesktopManager::GetWindowDesktopId`; restore window to correct desktop via `MoveWindowToDesktop` | Medium |
| 6 | Persist group-level position (member keys + shared rect) alongside individual window entries | Medium |
| 7 | Deferred restore via `PostMessage(WM_APP+N)` to let apps finish startup before repositioning (fixes flicker with Electron apps) | Small |

---

## Auto-Grouping (Rules Engine)

| # | Item | Effort |
|---|------|--------|
| 8 | `command_line` field matching (requires `NtQueryInformationProcess` or PEB read, best-effort) | Medium |
| 9 | `not_equals` / `not_contains` negation operators | Small |
| 10 | Hot-reload config on file change (`ReadDirectoryChangesW` file watcher) | Medium |
| 11 | Detect and warn on duplicate rule group names at load time | Small |
