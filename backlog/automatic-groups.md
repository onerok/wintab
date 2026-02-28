# Automatic Groups (Rules Engine)

## Problem

Currently WinTab only groups windows through manual user action (drag-and-drop). Every time a user launches a common application — a terminal, a browser, a code editor — they must manually re-group it with its siblings. This friction means users who have consistent, predictable window arrangements get no benefit from WinTab unless they redo the grouping on every session.

A rules engine lets users express their grouping intent once ("all VS Code windows go into the Dev group") and have it applied automatically whenever a matching window appears, making WinTab useful from first launch without any repeated manual effort.

## Location

### Existing files modified

- **`src/state.rs`** — `AppState::on_window_created` is the single integration point. After a new window passes `is_eligible()` and is added to `self.windows`, the rules engine is consulted. `AppState` gains a `rules: RulesEngine` field.
- **`src/window.rs`** — `WindowInfo` must be extended with `process_name: String` and `class_name: String` so the matcher has all required fields. `WindowInfo::from_hwnd` populates these at creation time.
- **`src/group.rs`** — `GroupManager` needs a way to look up a named group by its rule-defined name (a new `named_groups: HashMap<String, GroupId>` map) so the engine can route windows into the correct `TabGroup` without creating duplicates.

### New files

- **`src/rules.rs`** — Core module: `RuleField`, `FieldMatcher`, `WindowRule`, `RuleGroup`, and `RulesEngine`. Contains matching logic and the `apply` method called from `on_window_created`.
- **`%APPDATA%\WinTab\rules.json`** — Persisted configuration written/read by `RulesEngine`. Not a source file; path resolved at runtime via `SHGetKnownFolderPath(FOLDERID_RoamingAppData)`.

## Requirements

### Rule definition

- [ ] Each rule group has: a unique `name` (string), an `enabled` flag (bool), an ordered list of `rules` (one or more field matchers), and a `match_mode` (`all` = AND, `any` = OR).
- [ ] Supported fields for matching: `process_name`, `class_name`, `title`, `command_line` (optional, best-effort).
- [ ] Supported operators per field: `equals`, `not_equals`, `starts_with`, `ends_with`, `contains`, `not_contains`, `matches_regex`.
- [ ] All string comparisons default to case-insensitive; an optional `case_sensitive: bool` flag per matcher overrides this.
- [ ] Rule groups are evaluated in the order they appear in `rules.json`; a window joins the first matching enabled group and evaluation stops.
- [ ] A rule group with `enabled: false` is skipped entirely during matching but preserved in `rules.json`.
- [ ] The rules engine itself can be disabled globally (separate from the per-group `enabled` flag), controlled by `AppState::enabled` — when WinTab is disabled via the tray icon, no automatic grouping occurs.

### Window matching

- [ ] Matching is attempted in `AppState::on_window_created`, after `WindowInfo::from_hwnd` succeeds (i.e., only eligible windows are considered).
- [ ] If no rule group matches, the window is left ungrouped (existing behaviour unchanged).
- [ ] If a match is found and the target named group already has an active `TabGroup`, the new window is added to that group via `GroupManager::add_to_group`.
- [ ] If a match is found and no `TabGroup` yet exists for that name, a singleton group is created and recorded in `GroupManager::named_groups`; the second window to match will then join it.
- [ ] A window that is already in a group (manually grouped before the rules engine runs) is not re-grouped automatically.

### Persistence

- [ ] `rules.json` is loaded once at startup from `%APPDATA%\WinTab\rules.json`. Missing file is treated as empty rules list (no error, no crash).
- [ ] The file is not written by WinTab itself in this iteration; users edit it manually. (A future PBI may add a settings UI.)
- [ ] Malformed JSON is reported to the Windows event log (or a log file) and treated as empty rules; the application continues normally.
- [ ] Invalid regex patterns in a rule are reported per-rule and that specific rule is skipped; other rules in the same group continue to be evaluated.

### Data integrity

- [ ] `GroupManager::named_groups` is kept consistent with `groups`: when a named group dissolves (drops to one window and is removed), its entry in `named_groups` is also removed so the next matching window creates a fresh group.
- [ ] Rules are read-only at runtime in this iteration; no hot-reload is required (a future PBI may add file-watch support).

## Suggested Implementation

### Data structures (`src/rules.rs`)

```rust
use regex::Regex;

pub enum FieldMatcher {
    Equals(String, bool),         // (value, case_sensitive)
    NotEquals(String, bool),
    StartsWith(String, bool),
    EndsWith(String, bool),
    Contains(String, bool),
    NotContains(String, bool),
    MatchesRegex(Regex),          // pre-compiled at load time
}

pub enum RuleField {
    ProcessName,
    ClassName,
    Title,
    CommandLine,
}

pub struct WindowRule {
    pub field: RuleField,
    pub matcher: FieldMatcher,
}

pub enum MatchMode { All, Any }

pub struct RuleGroup {
    pub name: String,
    pub enabled: bool,
    pub match_mode: MatchMode,
    pub rules: Vec<WindowRule>,
}

pub struct RulesEngine {
    pub groups: Vec<RuleGroup>,   // evaluation order preserved
}
```

`FieldMatcher` is constructed from the deserialized JSON during `RulesEngine::load`. Regex variants call `Regex::new()` at that point; failures are logged and the rule is omitted.

### JSON schema (`rules.json`)

```json
{
  "groups": [
    {
      "name": "Dev Terminals",
      "enabled": true,
      "match_mode": "any",
      "rules": [
        { "field": "process_name", "op": "equals", "value": "WindowsTerminal.exe", "case_sensitive": false },
        { "field": "class_name",   "op": "equals", "value": "CASCADIA_HOSTING_WINDOW_CLASS", "case_sensitive": false }
      ]
    },
    {
      "name": "Editors",
      "enabled": true,
      "match_mode": "all",
      "rules": [
        { "field": "process_name", "op": "starts_with", "value": "Code", "case_sensitive": false }
      ]
    }
  ]
}
```

### `RulesEngine::apply` method

```rust
impl RulesEngine {
    /// Returns the name of the first matching enabled group, or None.
    pub fn apply(&self, info: &WindowInfo) -> Option<&str> {
        for group in &self.groups {
            if !group.enabled { continue; }
            let matched = match group.match_mode {
                MatchMode::All => group.rules.iter().all(|r| r.matches(info)),
                MatchMode::Any => group.rules.iter().any(|r| r.matches(info)),
            };
            if matched {
                return Some(&group.name);
            }
        }
        None
    }
}
```

`WindowRule::matches` extracts the relevant string from `WindowInfo` (process_name, class_name, title, or command_line) and delegates to `FieldMatcher`.

### Integration with `AppState::on_window_created` (`src/state.rs`)

```rust
pub fn on_window_created(&mut self, hwnd: HWND) {
    if self.suppress_events || !self.enabled { return; }
    if self.windows.contains_key(&hwnd) { return; }

    if let Some(info) = WindowInfo::from_hwnd(hwnd) {
        self.windows.insert(info.hwnd, info);

        // --- new: rules engine ---
        // Skip windows already grouped (e.g. manually grouped at startup scan)
        if self.groups.group_of(hwnd).is_some() { return; }

        if let Some(group_name) = self.rules.apply(self.windows.get(&hwnd).unwrap()) {
            let group_name = group_name.to_owned();
            if let Some(&existing_gid) = self.groups.named_groups.get(&group_name) {
                self.groups.add_to_group(existing_gid, hwnd);
                self.overlays.refresh_overlay(existing_gid, &self.groups, &self.windows);
            } else {
                // First window for this rule group — record as singleton, await a second.
                let gid = group::next_id();
                let tab_group = TabGroup { id: gid, tabs: vec![hwnd], active: 0 };
                self.groups.groups.insert(gid, tab_group);
                self.groups.window_to_group.insert(hwnd, gid);
                self.groups.named_groups.insert(group_name, gid);
            }
        }
    }
}
```

### `WindowInfo` extensions (`src/window.rs`)

```rust
pub struct WindowInfo {
    pub hwnd: HWND,
    pub title: String,
    pub process_name: String,   // new
    pub class_name: String,     // new
    pub icon: HICON,
    pub rect: RECT,
}
```

`process_name` is derived from `QueryFullProcessImageNameW` (called with the PID from `GetWindowThreadProcessId`) — extract only the final path component. `class_name` comes from `GetClassNameW`. Both are populated inside `WindowInfo::from_hwnd`.

### `GroupManager` extensions (`src/group.rs`)

```rust
pub struct GroupManager {
    pub groups: HashMap<GroupId, TabGroup>,
    pub window_to_group: HashMap<HWND, GroupId>,
    pub named_groups: HashMap<String, GroupId>,  // new
}
```

`remove_from_group` must check whether the dissolved group's `GroupId` is referenced in `named_groups` and remove it if so:

```rust
if dissolve {
    if let Some(group) = self.groups.remove(&group_id) {
        for &h in &group.tabs { self.window_to_group.remove(&h); }
        // Remove stale named group reference
        self.named_groups.retain(|_, &mut gid| gid != group_id);
    }
}
```

### New dependency

Add `regex = "1"` to `Cargo.toml` under `[dependencies]`. The `regex` crate is pure Rust, no additional Win32 features needed.

### New module (`src/rules.rs`) — file structure outline

1. `use` imports: `serde::{Deserialize}`, `regex::Regex`, `crate::window::WindowInfo`.
2. Serde-deserializable `JsonRuleGroup` / `JsonWindowRule` types (flat, stringly-typed) that convert into the typed `RuleGroup` / `WindowRule` via `TryFrom`.
3. `RulesEngine::load(path: &Path) -> Self` — reads JSON, converts, logs errors per-rule, returns valid rules only.
4. `RulesEngine::apply(&self, info: &WindowInfo) -> Option<&str>`.
5. `impl FieldMatcher { fn matches(&self, value: &str) -> bool }`.
6. `#[cfg(test)]` module with unit tests for each operator (pure string logic, no Win32 calls).

Add `serde = { version = "1", features = ["derive"] }` and `serde_json = "1"` to `Cargo.toml`.

### Startup loading (`src/main.rs` or `src/state.rs`)

```rust
let appdata = get_appdata_path(); // SHGetKnownFolderPath(FOLDERID_RoamingAppData)
let rules_path = appdata.join("WinTab").join("rules.json");
let rules = RulesEngine::load(&rules_path);
// Store in AppState before init() scans existing windows
state.rules = rules;
state.init(); // existing window scan now runs through the engine
```

Running the rules engine during `init()` (the startup window scan) ensures that pre-existing windows are also grouped automatically, not only newly created ones.

## Edge Cases

### Window matches multiple rule groups
The evaluation stops at the first match (ordered list, short-circuit). Users control precedence by ordering groups in `rules.json`. Document this clearly.

### Named group dissolves and re-forms
When all windows in a named group close, `named_groups` is cleaned up. The next window matching that rule name creates a fresh singleton. This is correct but means a user who closes all terminals and opens one new one will have a singleton until a second one opens — not a group. This is expected behaviour; document it.

### Window already in a manual group at startup
`init()` calls `on_window_created` for each enumerated window in an unspecified order. If the rules engine creates a singleton group for window A, then encounters window B which matches the same rule, it will try to add B to A's group. But A may have already been manually grouped with C from a previous session (sessions do not persist group state currently). Since `on_window_created` checks `group_of(hwnd).is_some()` before applying rules, re-scanning an already-grouped window is a no-op.

### Race between window creation events
`EVENT_OBJECT_CREATE` and `EVENT_OBJECT_SHOW` both call `on_window_created`. The existing early-exit `if self.windows.contains_key(&hwnd)` prevents double-processing. The rules engine benefits from this guard without additional changes.

### Regex compilation failure
If a user writes an invalid regex (e.g. `"[unclosed"`), `Regex::new()` returns an `Err`. Log the group name and pattern, skip that specific `WindowRule`. Other rules in the same `RuleGroup` are still evaluated. If all rules in a group are invalid, the group effectively never matches (vacuously correct for `all` mode; never matches for `any` mode).

### Regex catastrophic backtracking
The `regex` crate uses a finite-automaton engine with guaranteed linear-time matching, so malicious or poorly written patterns cannot cause runaway CPU usage. No additional mitigation needed.

### `command_line` field availability
`NtQueryInformationProcess` (or reading the PEB via `ReadProcessMemory`) is required to obtain another process's command line. This is fragile and requires elevated privileges for some processes. The `command_line` field should be attempted with a best-effort helper that returns `None` on failure; a `None` command line causes any `CommandLine` rule to evaluate as no-match rather than panic.

### Case sensitivity on Windows paths
`process_name` is extracted from the file system path, which is case-insensitive on NTFS but the extracted string may vary in case between processes. All comparisons default to case-insensitive (`unicase` or `.to_lowercase()` normalisation) unless the rule specifies `"case_sensitive": true`.

### Named group collisions between rules
Two `RuleGroup` entries with the same `name` field would map to the same `GroupId` after the first group is created. This is a user configuration error. Detect it during `RulesEngine::load`, log a warning for the duplicate, and skip the second entry entirely.

### Hot-reload (not in scope)
File-watch support (`ReadDirectoryChangesW`) is explicitly deferred. If users edit `rules.json` while WinTab is running, changes take effect only after restart. Document this limitation.

### Singleton groups in the overlay
A singleton `TabGroup` created by the rules engine for the first matching window will show a tab bar with one tab — indistinguishable from a manually created group in the same state. This is consistent behaviour but may confuse users. A future enhancement could suppress the overlay for singleton groups or label rule-created groups distinctly.
