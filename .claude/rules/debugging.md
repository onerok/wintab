# Debugging Rules

## Process

1. **Reproduce first** — never investigate or hypothesize before reproducing the bug
2. **Write a failing test** — capture the bug as a test before touching any production code
3. **Then investigate** — use the failing test to identify the root cause
4. **Fix** — implement the minimal fix that makes the test pass
5. **Verify** — `just test` all green, `just lint` clean

## Known COM Quirks (IVirtualDesktopManager)

- `IsWindowOnCurrentVirtualDesktop` can return **stale results** for windows recently transitioned via `ShowWindow(SW_HIDE)` → `ShowWindow(SW_SHOW)` (e.g., during tab switch)
- The foreground window is **always** on the current desktop by definition — trust this over COM
- When testing desktop visibility logic, use `VDesktopManager::set_off_desktop()` mock to simulate COM inconsistencies — the real COM API returns correct results for in-process test windows

## Testing Desktop Visibility

- `vdesktop` is `None` by default in test state — `sync_desktop_visibility()` is a no-op
- To exercise the desktop visibility codepath, initialize `s.vdesktop = VDesktopManager::new()` and use `set_off_desktop()` / `clear_mock()` to control what the mock reports
- Hooks use `WINEVENT_SKIPOWNPROCESS` — events from in-process test windows don't fire through hooks; call `on_focus_changed()` etc. directly to simulate
