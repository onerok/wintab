# Whitelist/Blacklist Exception Management

## Problem

WinTab's `is_eligible()` function in `window.rs` uses hard-coded heuristics to decide which windows to manage. Some windows that should be tabbed are filtered out (false negatives), and some that shouldn't be tabbed slip through (false positives). Users need a way to override eligibility decisions via whitelist (force-tab) and blacklist (never-tab) rules, similar to the auto-grouping rules engine.

## Location

**Files modified:**

- `src/config.rs` — Add `whitelist` and `blacklist` sections to the config YAML schema, using the same `WindowRule` pattern matching as auto-grouping rules.
- `src/window.rs` — Modify `is_eligible()` to check whitelist/blacklist before applying default heuristics.
- `src/state.rs` — Add `blacklist_app()` method for the "Never Tab This Application" context menu action.

**New files:** None.

## Requirements

- [ ] Config supports `blacklist` and `whitelist` sections, each containing a list of matching rules.
- [ ] Rules use the same pattern matching as auto-grouping: `process_name`, `class_name`, `title` fields with `equals`, `contains`, `starts_with`, `ends_with`, `regex` operators.
- [ ] Evaluation order: Blacklist → Whitelist → Default heuristics.
  - If a window matches a blacklist rule, it is NEVER tabbed (regardless of whitelist or heuristics).
  - If a window matches a whitelist rule, it is ALWAYS tabbed (overrides default heuristics).
  - Otherwise, default `is_eligible()` logic applies.
- [ ] The "Never Tab This Application" context menu action adds a blacklist rule by `process_name` and saves config.
- [ ] Blacklisted windows that are already in groups are ungrouped when the blacklist is applied.

## Suggested Implementation

### Config YAML format

```yaml
blacklist:
  - process_name:
      equals: "conhost.exe"
  - class_name:
      contains: "Popup"

whitelist:
  - process_name:
      equals: "CustomApp.exe"
  - title:
      starts_with: "Terminal"
```

### Config struct additions

```rust
pub struct RulesEngine {
    pub groups: Vec<RuleGroup>,
    pub preview_config: PreviewConfig,
    pub blacklist: Vec<WindowRule>,
    pub whitelist: Vec<WindowRule>,
}
```

### Modified `is_eligible()` in `window.rs`

```rust
pub fn is_eligible(hwnd: HWND) -> bool {
    // 1. Check blacklist
    let info = WindowRuleInfo {
        process_name: &get_process_name(hwnd),
        class_name: &get_class_name(hwnd),
        title: &get_window_title(hwnd),
        command_line: None,
    };

    let rules = state::try_with_state_ret(|s| {
        let bl = s.rules.blacklist.iter().any(|r| r.matches(&info));
        let wl = s.rules.whitelist.iter().any(|r| r.matches(&info));
        (bl, wl)
    });

    if let Some((blacklisted, whitelisted)) = rules {
        if blacklisted { return false; }
        if whitelisted { return true; }
    }

    // 2. Default heuristics (existing logic)
    is_eligible_default(hwnd)
}
```

Note: `is_eligible` currently doesn't access AppState. Adding `try_with_state_ret` here requires care around re-entrancy since `is_eligible` is called from `on_window_created` (inside `with_state`). Consider passing the rules as a parameter instead:

```rust
pub fn is_eligible(hwnd: HWND, blacklist: &[WindowRule], whitelist: &[WindowRule]) -> bool {
    // ...
}
```

### "Never Tab This Application" implementation

```rust
pub fn blacklist_app(&mut self, group_id: GroupId, tab_index: usize) {
    let (hwnd, process_name) = match self.groups.groups.get(&group_id) {
        Some(g) => match g.tabs.get(tab_index) {
            Some(&h) => match self.windows.get(&h) {
                Some(info) => (h, info.process_name.clone()),
                None => return,
            },
            None => return,
        },
        None => return,
    };

    // Add blacklist rule
    self.rules.blacklist.push(WindowRule {
        field: RuleField::ProcessName,
        matcher: Matcher::Equals(process_name.clone()),
    });

    // Save updated config
    config::save_rules(&self.rules);

    // Remove all windows with this process from groups
    let to_remove: Vec<HWND> = self.windows.values()
        .filter(|w| w.process_name == process_name)
        .map(|w| w.hwnd)
        .collect();
    for hwnd in to_remove {
        self.remove_window(hwnd);
    }
}
```

### Config save helper

```rust
pub fn save_rules(rules: &RulesEngine) {
    // Read existing config.yaml, update blacklist/whitelist sections, write back
    // Preserve existing auto-grouping rules and other sections
}
```

## Edge Cases

- **Blacklist vs. auto-grouping**: A blacklisted window should never be added to any group, even if an auto-grouping rule matches it. Blacklist is checked before rule evaluation in `apply_rules()`.

- **Re-entrancy in `is_eligible`**: `is_eligible` is called from `on_window_created`, which runs inside `with_state()`. Calling `try_with_state_ret` from `is_eligible` would fail due to the existing borrow. Solution: pass `&rules.blacklist` and `&rules.whitelist` into `is_eligible` as parameters, or evaluate eligibility inside `on_window_created` directly.

- **Already-grouped windows**: When adding a blacklist rule via context menu, remove all windows matching the rule from existing groups. This triggers group dissolution if groups fall below 2 members.

- **Config file format**: The blacklist/whitelist sections must be properly serialized alongside existing config. Use `serde_yaml`'s partial serialization or manual YAML editing to avoid overwriting other config sections.

- **Empty blacklist/whitelist**: Missing sections in `config.yaml` should default to empty lists, not error.

- **Rule ordering**: Blacklist rules are evaluated in order. First match wins. This matches the auto-grouping behavior.

- **Process name casing**: Windows process names are case-insensitive. Consider using case-insensitive matching for `process_name` rules (the `equals` operator should compare case-insensitively on Windows).
