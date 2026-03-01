# Testing Rules

## Three Test Tiers

### 1. Unit Tests — pure logic, no Win32

- Inline `#[cfg(test)] mod tests` in the module being tested
- Use fake HWNDs (`n as HWND`) — no real windows needed
- Cover: math, parsing, state transitions, index logic, data structures
- Examples: `calculate_tab_index`, `premultiply_pixel`, `TabGroup::remove` index adjustment, config pattern matching, position store fuzzy lookup
- `#[cfg(test)]` accessors are fine for exposing internal state to tests

### 2. Acceptance Tests — real Win32 windows, in-process

- Live in `src/acceptance.rs`
- Create real windows via `create_test_window()` or `dummy_window.exe` subprocess
- Insert `WindowInfo` into state manually (bypasses `is_eligible` PID check)
- Call state methods directly (`on_focus_changed`, `switch_to`, etc.) to simulate hook events — hooks use `WINEVENT_SKIPOWNPROCESS` so they don't fire for in-process windows
- Use `pump_messages()` between steps to let Win32 settle
- Use `VDesktopManager::set_off_desktop()` mock to test desktop visibility logic — real COM returns correct results for in-process windows
- Always clean up: remove from groups, remove from state, `DestroyWindow`, kill child processes

### 3. Desktop-Switch E2E — `#[ignore]`, real desktop switching

- 2 tests that use `SendInput` to press Ctrl+Win+Arrow
- Must run serial: `--test-threads=1` via `just test-all`
- Only run when needed — they manipulate real virtual desktops

## E2E Evidence Required for New Features

Any feature that creates, manipulates, or interacts with windows MUST include an acceptance test with screenshot evidence before being marked as done.

1. **Write the test as part of the feature implementation**, not as a follow-up
2. Screenshots captured via `screenshot::capture_region()` / `screenshot::capture_window()`, saved to `evidence/<test_name>/`
3. Test must assert observable state (window visibility, position, group membership)

### E2E test pattern

```rust
#[test]
fn acceptance_e2e_feature_name() {
    // 1. Register classes, create test windows (or spawn dummy_window.exe)
    // 2. Insert WindowInfo into state
    // 3. Set up state (create groups, switch tabs, etc.)
    // 4. Exercise the feature
    // 5. Assert state + capture screenshot evidence
    // 6. Clean up (remove from state, destroy windows, kill child processes)
}
```

### When E2E is not needed

- Pure config parsing logic (unit tests suffice)
- Internal refactors with no behavior change
- Bug fixes where an existing test already covers the scenario

## Which Tier to Use

| Change | Unit | Acceptance | Desktop E2E |
|---|---|---|---|
| New pure function / algorithm | Yes | — | — |
| State logic (group add/remove/switch) | Yes | — | — |
| Config parsing / matching | Yes | — | — |
| Window creation / grouping / overlay | — | Yes | — |
| Tab switching + hook events | — | Yes | — |
| Virtual desktop visibility (mock) | — | Yes | — |
| Real desktop switching | — | — | Yes |

## Test Hygiene

- Run `just test` before committing — all tests must pass
- Run `just lint` — zero clippy warnings
- Bug fixes require a failing test FIRST, then the fix
