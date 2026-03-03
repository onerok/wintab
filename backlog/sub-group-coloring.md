# Sub-Group Color Coding

## Problem

Within a tab group containing many windows from the same application (e.g., 8 VS Code windows), all tabs look identical — same icon, similar titles. Users who remote into multiple machines via VS Code SSH have no way to visually distinguish "SSH: desktop" tabs from "SSH: laptop" tabs at a glance. A sub-group coloring system lets users define pattern-based color rules that tint individual tabs within a group based on their window title (or other fields), making clusters of related tabs instantly recognizable.

## Mockups

See [mockups/sub-group-coloring.html](mockups/sub-group-coloring.html) for visual examples of styling options (bottom stripe, left edge bar, top stripe, dot indicator, full tint, tint + stripe).

## Location

**Files modified:**

- `src/config.rs` — Add `tab_colors` section to the YAML schema: a list of `TabColorRule` entries, each with a pattern (field + operator + value, reusing existing `PatternDef` format) and a color.
- `src/overlay.rs` — Modify the per-tab color selection in `paint_tabs()` (lines 581–587) to check the window's metadata against color rules before falling back to the default `COLOR_ACTIVE` / `COLOR_INACTIVE` / `COLOR_HOVER`. Pass resolved color into `fill_rect_alpha`.
- `src/state.rs` — Expose color rules from `RulesEngine` to overlay rendering.

**New files:** None.

## Requirements

- [ ] A `tab_colors` section in `config.yaml` defines color rules as a list of pattern + color pairs.
- [ ] Each rule matches window metadata using the same field/operator system as auto-grouping rules (`process_name`, `class_name`, `title`, `command_line` with `equals`, `contains`, `starts_with`, `ends_with`, `regex`).
- [ ] When a tab's window matches a color rule, that rule's color is used as the tab's base color instead of the hard-coded `COLOR_ACTIVE` / `COLOR_INACTIVE`.
- [ ] Multiple color rules are evaluated in order; first match wins.
- [ ] A tab that matches no color rule uses the default colors (current behavior).
- [ ] Active/inactive/hover variants are derived from the rule's base color (darken for inactive, lighten for hover).
- [ ] Color rules apply per-tab, not per-group — different tabs in the same group can have different colors.
- [ ] Config changes are picked up on hot reload (existing `config.rs` hot-reload path).

## Suggested Implementation

### YAML config format

```yaml
tab_colors:
  - pattern:
      field: title
      op: contains
      value: "SSH: rok5"
    color: "#2D7D46"       # green tint for rok5

  - pattern:
      field: title
      op: contains
      value: "SSH: rok7"
    color: "#7D2D46"       # red tint for rok7

  - pattern:
      field: process_name
      op: equals
      value: "firefox.exe"
    color: "#E66000"       # Firefox orange
```

### Serde schema additions in `config.rs`

Reuse the existing `PatternDef` struct for the pattern field:

```rust
#[derive(Deserialize)]
struct TabColorRuleDef {
    pattern: PatternDef,
    color: String,          // "#RRGGBB" hex string
}

#[derive(Deserialize)]
struct ConfigFile {
    #[serde(default)]
    rules: Vec<RuleGroupDef>,
    #[serde(default)]
    preview: Option<PreviewConfig>,
    #[serde(default)]
    tab_colors: Vec<TabColorRuleDef>,
}
```

### Runtime type

```rust
#[derive(Debug)]
pub struct TabColorRule {
    pub rule: WindowRule,       // reuse existing field + matcher
    pub color: u32,             // 0x00RRGGBB
}

pub struct RulesEngine {
    pub groups: Vec<RuleGroup>,
    pub preview_config: PreviewConfig,
    pub tab_colors: Vec<TabColorRule>,
}
```

### Parsing the color string

```rust
fn parse_hex_color(s: &str) -> Option<u32> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    u32::from_str_radix(hex, 16).ok()
}
```

### Color derivation from base

```rust
fn derive_tab_colors(base: u32) -> (u32, u32, u32) {
    let active = base;
    let inactive = darken(base, 0.55);
    let hover = lighten(base, 1.3);
    (active, inactive, hover)
}

fn darken(color: u32, factor: f32) -> u32 {
    let r = (((color >> 16) & 0xFF) as f32 * factor).min(255.0) as u32;
    let g = (((color >> 8) & 0xFF) as f32 * factor).min(255.0) as u32;
    let b = ((color & 0xFF) as f32 * factor).min(255.0) as u32;
    (r << 16) | (g << 8) | b
}

fn lighten(color: u32, factor: f32) -> u32 {
    let r = (((color >> 16) & 0xFF) as f32 * factor).min(255.0) as u32;
    let g = (((color >> 8) & 0xFF) as f32 * factor).min(255.0) as u32;
    let b = ((color & 0xFF) as f32 * factor).min(255.0) as u32;
    (r << 16) | (g << 8) | b
}
```

### Integration into `paint_tabs()` in `overlay.rs`

The current per-tab color logic (lines 581–587) is:

```rust
let color = if is_hover {
    COLOR_HOVER
} else if is_active {
    COLOR_ACTIVE
} else {
    COLOR_INACTIVE
};
```

Replace with:

```rust
// Resolve per-tab color from rules (if any match)
let (c_active, c_inactive, c_hover) = if let Some(info) = info {
    resolve_tab_color(info, &tab_color_rules)
} else {
    (COLOR_ACTIVE, COLOR_INACTIVE, COLOR_HOVER)
};

let color = if is_hover {
    c_hover
} else if is_active {
    c_active
} else {
    c_inactive
};
```

### `resolve_tab_color` function

```rust
fn resolve_tab_color(
    info: &WindowInfo,
    rules: &[TabColorRule],
) -> (u32, u32, u32) {
    let rule_info = WindowRuleInfo {
        process_name: &info.process_name,
        class_name: &info.class_name,
        title: &info.title,
        command_line: info.command_line.as_deref().unwrap_or(""),
    };

    for rule in rules {
        let field_value = match rule.rule.field {
            RuleField::ProcessName => rule_info.process_name,
            RuleField::ClassName => rule_info.class_name,
            RuleField::Title => rule_info.title,
            RuleField::CommandLine => rule_info.command_line,
        };
        if rule.rule.matcher.matches(field_value) {
            return derive_tab_colors(rule.color);
        }
    }

    // No match — default colors
    (COLOR_ACTIVE, COLOR_INACTIVE, COLOR_HOVER)
}
```

### Threading color rules into `paint_tabs`

`paint_tabs` currently takes `(overlay_hwnd, group, rect, windows)`. The color rules come from `RulesEngine` in `AppState`. Two options:

**Option A — Pass rules as a parameter** (preferred, avoids re-entrancy):

```rust
fn paint_tabs(
    overlay_hwnd: HWND,
    group: &TabGroup,
    rect: &RECT,
    windows: &HashMap<HWND, WindowInfo>,
    tab_color_rules: &[TabColorRule],    // new parameter
)
```

Callers (`update_overlay`, `update_overlay_standalone`) extract the rules from state before calling paint.

**Option B — Read rules from state inside paint** (simpler callsite, uses `try_with_state_ret`):

```rust
let tab_color_rules = state::try_with_state_ret(|s| s.rules.tab_colors.clone())
    .unwrap_or_default();
```

Option A is safer (no `RefCell` re-entrancy risk since `paint_tabs` may be called from `update_overlay_standalone` inside `overlay_wnd_proc`).

### Example config for the VS Code SSH use case

```yaml
tab_colors:
  - pattern:
      field: title
      op: contains
      value: "SSH: rok5"
    color: "#2E8B57"       # sea green

  - pattern:
      field: title
      op: contains
      value: "SSH: rok7"
    color: "#CD5C5C"       # indian red

  - pattern:
      field: title
      op: contains
      value: "[WSL"
    color: "#6A5ACD"       # slate blue for WSL windows
```

## Edge Cases

- **Title changes**: Window titles change dynamically (e.g., VS Code changes title as you switch files, but the "SSH: rok5" suffix persists). The color rule re-evaluates on every repaint since `paint_tabs` is called on title change events (`EVENT_OBJECT_NAMECHANGE` -> `on_title_changed` -> `update_overlay`). No caching needed.

- **Overlapping rules**: If a window title contains both "SSH: rok5" and "Firefox" and both have rules, first-match-wins (evaluation order in the YAML list). Document that rule order matters.

- **Case sensitivity**: The existing `PatternDef` has a `case_sensitive` field (default `false`). Color rules inherit this — `contains: "ssh: rok5"` matches `"SSH: rok5"` by default. This is the correct behavior for title matching.

- **Color legibility**: Users may pick colors that make text unreadable (e.g., white base color with white text). The text color (`COLOR_TEXT = 0x00FFFFFF`) is constant. Consider adding a per-rule text color option, or auto-selecting white/black text based on luminance: `if (0.299*R + 0.587*G + 0.114*B) > 186 { black } else { white }`.

- **Interaction with `tab-coloring.md`**: The `tab-coloring` PBI defines group-level color modes (System accent, Per-app, Custom, None). Sub-group color rules take precedence — if a tab matches a color rule, that color is used regardless of the group-level color mode. Rules are per-tab overrides on top of the group default.

- **No rules defined**: If `tab_colors` is absent or empty in `config.yaml`, all tabs use the default colors (current behavior). Zero performance impact — the loop in `resolve_tab_color` does no iterations.

- **Performance**: Color rules are evaluated per-tab per-repaint. With a small number of rules (< 20) and the existing single-threaded repaint model, this is negligible. Each rule evaluation is a string `contains`/`starts_with`/etc. — microseconds at worst.

- **Hot reload**: The existing config hot-reload path re-parses `config.yaml` and replaces `RulesEngine` in `AppState`. New color rules take effect on the next overlay repaint (triggered by `overlays.update_all` after config reload).

- **Alpha premultiplication**: The resolved color is passed to `fill_rect_alpha` which calls `premultiply_pixel`. The color must be in 0x00RRGGBB format (no alpha), matching the existing constant format. The `parse_hex_color` function strips `#` and returns this format.

- **Config validation**: Invalid hex strings (e.g., `"not-a-color"`) should be skipped with a warning logged via `eprintln!` or `OutputDebugStringW`, not cause a parse failure of the entire config.
