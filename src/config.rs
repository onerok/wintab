use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ── Preview config ──

fn default_preview_width() -> i32 {
    300
}
fn default_preview_max_height() -> i32 {
    400
}
fn default_preview_opacity() -> u8 {
    200
}
fn default_preview_delay_ms() -> u32 {
    500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewConfig {
    #[serde(default = "default_preview_width")]
    pub width: i32,
    #[serde(default = "default_preview_max_height")]
    pub max_height: i32,
    #[serde(default = "default_preview_opacity")]
    pub opacity: u8,
    #[serde(default = "default_preview_delay_ms")]
    pub delay_ms: u32,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        PreviewConfig {
            width: default_preview_width(),
            max_height: default_preview_max_height(),
            opacity: default_preview_opacity(),
            delay_ms: default_preview_delay_ms(),
        }
    }
}

// ── Tab color style ──

#[derive(Deserialize, Serialize, Default, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TabColorStyle {
    BottomStripe,
    LeftBar,
    TopStripe,
    FullTint,
    #[default]
    TintStripe,
}

// ── Serde schema ──

#[derive(Deserialize)]
struct ConfigFile {
    #[serde(default)]
    rules: Vec<RuleGroupDef>,
    #[serde(default)]
    preview: Option<PreviewConfig>,
    #[serde(default)]
    tab_colors: Vec<TabColorRuleDef>,
    #[serde(default)]
    tab_color_style: TabColorStyle,
}

#[derive(Deserialize)]
struct TabColorRuleDef {
    pattern: PatternDef,
    color: String,
}

#[derive(Deserialize)]
struct RuleGroupDef {
    name: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    #[serde(rename = "match")]
    match_mode: MatchModeDef,
    #[serde(default)]
    patterns: Vec<PatternDef>,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum MatchModeDef {
    #[default]
    All,
    Any,
}

#[derive(Deserialize)]
struct PatternDef {
    field: String,
    op: String,
    value: String,
    #[serde(default)]
    case_sensitive: bool,
}

// ── Runtime types ──

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RuleField {
    ProcessName,
    ClassName,
    Title,
    CommandLine,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatchMode {
    All,
    Any,
}

#[derive(Debug)]
pub enum Matcher {
    Equals(String, bool),
    Contains(String, bool),
    StartsWith(String, bool),
    EndsWith(String, bool),
    NotEquals(String, bool),
    NotContains(String, bool),
    Regex(regex::Regex),
}

#[derive(Debug)]
pub struct WindowRule {
    pub field: RuleField,
    pub matcher: Matcher,
}

#[derive(Debug)]
pub struct RuleGroup {
    pub name: String,
    pub enabled: bool,
    pub match_mode: MatchMode,
    pub rules: Vec<WindowRule>,
}

/// A compiled tab color rule: pattern + GDI-order color.
#[derive(Debug)]
pub struct TabColorRule {
    pub rule: WindowRule,
    pub color: u32, // 0x00BBGGRR (Windows GDI byte order)
}

/// Holds all parsed rules. Created once at startup.
pub struct RulesEngine {
    pub groups: Vec<RuleGroup>,
    pub preview_config: PreviewConfig,
    pub tab_colors: Vec<TabColorRule>,
    pub tab_color_style: TabColorStyle,
}

/// Info needed to evaluate rules against a window.
pub struct WindowRuleInfo<'a> {
    pub process_name: &'a str,
    pub class_name: &'a str,
    pub title: &'a str,
    pub command_line: &'a str,
}

impl Matcher {
    fn matches(&self, value: &str) -> bool {
        match self {
            Matcher::Equals(pat, case_sensitive) => {
                if *case_sensitive {
                    value == pat
                } else {
                    value.eq_ignore_ascii_case(pat)
                }
            }
            Matcher::Contains(pat, case_sensitive) => {
                if *case_sensitive {
                    value.contains(pat.as_str())
                } else {
                    value
                        .to_ascii_lowercase()
                        .contains(&pat.to_ascii_lowercase())
                }
            }
            Matcher::StartsWith(pat, case_sensitive) => {
                if *case_sensitive {
                    value.starts_with(pat.as_str())
                } else {
                    value
                        .to_ascii_lowercase()
                        .starts_with(&pat.to_ascii_lowercase())
                }
            }
            Matcher::EndsWith(pat, case_sensitive) => {
                if *case_sensitive {
                    value.ends_with(pat.as_str())
                } else {
                    value
                        .to_ascii_lowercase()
                        .ends_with(&pat.to_ascii_lowercase())
                }
            }
            Matcher::NotEquals(pat, case_sensitive) => {
                if *case_sensitive {
                    value != pat
                } else {
                    !value.eq_ignore_ascii_case(pat)
                }
            }
            Matcher::NotContains(pat, case_sensitive) => {
                if *case_sensitive {
                    !value.contains(pat.as_str())
                } else {
                    !value
                        .to_ascii_lowercase()
                        .contains(&pat.to_ascii_lowercase())
                }
            }
            Matcher::Regex(re) => re.is_match(value),
        }
    }
}

impl WindowRule {
    pub fn matches(&self, info: &WindowRuleInfo) -> bool {
        let value = match self.field {
            RuleField::ProcessName => info.process_name,
            RuleField::ClassName => info.class_name,
            RuleField::Title => info.title,
            RuleField::CommandLine => info.command_line,
        };
        self.matcher.matches(value)
    }
}

impl RuleGroup {
    fn matches(&self, info: &WindowRuleInfo) -> bool {
        if !self.enabled || self.rules.is_empty() {
            return false;
        }
        match self.match_mode {
            MatchMode::All => self.rules.iter().all(|r| r.matches(info)),
            MatchMode::Any => self.rules.iter().any(|r| r.matches(info)),
        }
    }
}

impl RulesEngine {
    /// Load rules from a YAML file. Returns an empty engine on any error.
    pub fn load(path: &Path) -> Self {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Self::empty(),
        };
        let config: ConfigFile = match serde_yaml::from_str(&content) {
            Ok(c) => c,
            Err(_) => return Self::empty(),
        };

        let mut seen_names = HashSet::new();
        let groups = config
            .rules
            .into_iter()
            .filter_map(|def| {
                let rules: Vec<WindowRule> =
                    def.patterns.into_iter().filter_map(parse_pattern).collect();
                if rules.is_empty() {
                    return None;
                }
                if !seen_names.insert(def.name.clone()) {
                    eprintln!("WinTab: duplicate rule group name {:?}, skipping", def.name);
                    return None;
                }
                Some(RuleGroup {
                    name: def.name,
                    enabled: def.enabled,
                    match_mode: match def.match_mode {
                        MatchModeDef::All => MatchMode::All,
                        MatchModeDef::Any => MatchMode::Any,
                    },
                    rules,
                })
            })
            .collect();

        let preview_config = config.preview.unwrap_or_default();

        let tab_colors = config
            .tab_colors
            .into_iter()
            .filter_map(|def| {
                let rule = parse_pattern(def.pattern)?;
                let color = match parse_hex_color(&def.color) {
                    Some(c) => c,
                    None => {
                        eprintln!("WinTab: invalid tab color {:?}, skipping", def.color);
                        return None;
                    }
                };
                Some(TabColorRule { rule, color })
            })
            .collect();

        RulesEngine {
            groups,
            preview_config,
            tab_colors,
            tab_color_style: config.tab_color_style,
        }
    }

    pub fn empty() -> Self {
        RulesEngine {
            groups: Vec::new(),
            preview_config: PreviewConfig::default(),
            tab_colors: Vec::new(),
            tab_color_style: TabColorStyle::default(),
        }
    }

    /// Returns the name of the first matching enabled rule group, or None.
    pub fn apply(&self, info: &WindowRuleInfo) -> Option<&str> {
        self.groups
            .iter()
            .find(|g| g.matches(info))
            .map(|g| g.name.as_str())
    }

    pub fn has_rules(&self) -> bool {
        !self.groups.is_empty()
    }

    /// Returns true if any rule references the command_line field.
    pub fn uses_command_line(&self) -> bool {
        self.groups
            .iter()
            .any(|g| g.rules.iter().any(|r| r.field == RuleField::CommandLine))
            || self
                .tab_colors
                .iter()
                .any(|tc| tc.rule.field == RuleField::CommandLine)
    }

    /// Find the first matching tab color for a window.
    #[cfg(test)]
    pub fn match_tab_color(&self, info: &WindowRuleInfo) -> Option<u32> {
        self.tab_colors
            .iter()
            .find(|tc| tc.rule.matches(info))
            .map(|tc| tc.color)
    }
}

/// Parse "#RRGGBB" hex string to GDI byte order (0x00BBGGRR).
fn parse_hex_color(s: &str) -> Option<u32> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    let r = (rgb >> 16) & 0xFF;
    let g = (rgb >> 8) & 0xFF;
    let b = rgb & 0xFF;
    Some((b << 16) | (g << 8) | r)
}

fn parse_field(s: &str) -> Option<RuleField> {
    match s {
        "process_name" => Some(RuleField::ProcessName),
        "class_name" => Some(RuleField::ClassName),
        "title" => Some(RuleField::Title),
        "command_line" => Some(RuleField::CommandLine),
        _ => None,
    }
}

fn parse_pattern(def: PatternDef) -> Option<WindowRule> {
    let field = parse_field(&def.field)?;
    let matcher = match def.op.as_str() {
        "equals" => Matcher::Equals(def.value, def.case_sensitive),
        "contains" => Matcher::Contains(def.value, def.case_sensitive),
        "starts_with" => Matcher::StartsWith(def.value, def.case_sensitive),
        "ends_with" => Matcher::EndsWith(def.value, def.case_sensitive),
        "not_equals" => Matcher::NotEquals(def.value, def.case_sensitive),
        "not_contains" => Matcher::NotContains(def.value, def.case_sensitive),
        "regex" => {
            let re = regex::Regex::new(&def.value).ok()?;
            Matcher::Regex(re)
        }
        _ => return None,
    };
    Some(WindowRule { field, matcher })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn info(process: &str, class: &str, title: &str) -> WindowRuleInfo<'static> {
        info_with_cmd(process, class, title, "")
    }

    fn info_with_cmd(
        process: &str,
        class: &str,
        title: &str,
        cmd: &str,
    ) -> WindowRuleInfo<'static> {
        // Leak strings for test convenience
        WindowRuleInfo {
            process_name: Box::leak(process.to_string().into_boxed_str()),
            class_name: Box::leak(class.to_string().into_boxed_str()),
            title: Box::leak(title.to_string().into_boxed_str()),
            command_line: Box::leak(cmd.to_string().into_boxed_str()),
        }
    }

    // ── Matcher tests ──

    #[test]
    fn matcher_equals_case_insensitive() {
        let m = Matcher::Equals("Foo.exe".into(), false);
        assert!(m.matches("foo.exe"));
        assert!(m.matches("FOO.EXE"));
        assert!(!m.matches("bar.exe"));
    }

    #[test]
    fn matcher_equals_case_sensitive() {
        let m = Matcher::Equals("Foo.exe".into(), true);
        assert!(m.matches("Foo.exe"));
        assert!(!m.matches("foo.exe"));
    }

    #[test]
    fn matcher_contains() {
        let m = Matcher::Contains("term".into(), false);
        assert!(m.matches("WindowsTerminal.exe"));
        assert!(m.matches("TERMINAL"));
        assert!(!m.matches("notepad"));
    }

    #[test]
    fn matcher_starts_with() {
        let m = Matcher::StartsWith("Code".into(), false);
        assert!(m.matches("Code.exe"));
        assert!(m.matches("code - something"));
        assert!(!m.matches("VSCode.exe"));
    }

    #[test]
    fn matcher_ends_with() {
        let m = Matcher::EndsWith(".exe".into(), false);
        assert!(m.matches("notepad.EXE"));
        assert!(!m.matches("notepad.com"));
    }

    #[test]
    fn matcher_regex() {
        let re = regex::Regex::new(r"(?i)code.*\.exe$").unwrap();
        let m = Matcher::Regex(re);
        assert!(m.matches("Code.exe"));
        assert!(m.matches("VSCode.exe"));
        assert!(!m.matches("notepad.exe"));
    }

    // ── Engine tests ──

    #[test]
    fn engine_first_match_wins() {
        let engine = RulesEngine {
            groups: vec![
                RuleGroup {
                    name: "First".into(),
                    enabled: true,
                    match_mode: MatchMode::All,
                    rules: vec![WindowRule {
                        field: RuleField::ProcessName,
                        matcher: Matcher::Equals("code.exe".into(), false),
                    }],
                },
                RuleGroup {
                    name: "Second".into(),
                    enabled: true,
                    match_mode: MatchMode::All,
                    rules: vec![WindowRule {
                        field: RuleField::ProcessName,
                        matcher: Matcher::Equals("code.exe".into(), false),
                    }],
                },
            ],
            preview_config: PreviewConfig::default(),
            tab_colors: Vec::new(),
            tab_color_style: TabColorStyle::default(),
        };
        let i = info("code.exe", "", "");
        assert_eq!(engine.apply(&i), Some("First"));
    }

    #[test]
    fn engine_disabled_group_skipped() {
        let engine = RulesEngine {
            groups: vec![RuleGroup {
                name: "Disabled".into(),
                enabled: false,
                match_mode: MatchMode::All,
                rules: vec![WindowRule {
                    field: RuleField::ProcessName,
                    matcher: Matcher::Equals("code.exe".into(), false),
                }],
            }],
            preview_config: PreviewConfig::default(),
            tab_colors: Vec::new(),
            tab_color_style: TabColorStyle::default(),
        };
        let i = info("code.exe", "", "");
        assert_eq!(engine.apply(&i), None);
    }

    #[test]
    fn engine_all_mode_requires_all_patterns() {
        let engine = RulesEngine {
            groups: vec![RuleGroup {
                name: "AllMode".into(),
                enabled: true,
                match_mode: MatchMode::All,
                rules: vec![
                    WindowRule {
                        field: RuleField::ProcessName,
                        matcher: Matcher::Equals("code.exe".into(), false),
                    },
                    WindowRule {
                        field: RuleField::ClassName,
                        matcher: Matcher::Equals("Chrome_WidgetWin_1".into(), false),
                    },
                ],
            }],
            preview_config: PreviewConfig::default(),
            tab_colors: Vec::new(),
            tab_color_style: TabColorStyle::default(),
        };
        // Both match
        let i = info("code.exe", "Chrome_WidgetWin_1", "");
        assert_eq!(engine.apply(&i), Some("AllMode"));

        // Only one matches
        let i2 = info("code.exe", "Other", "");
        assert_eq!(engine.apply(&i2), None);
    }

    #[test]
    fn engine_any_mode_requires_one_pattern() {
        let engine = RulesEngine {
            groups: vec![RuleGroup {
                name: "AnyMode".into(),
                enabled: true,
                match_mode: MatchMode::Any,
                rules: vec![
                    WindowRule {
                        field: RuleField::ProcessName,
                        matcher: Matcher::Equals("code.exe".into(), false),
                    },
                    WindowRule {
                        field: RuleField::ProcessName,
                        matcher: Matcher::Equals("notepad.exe".into(), false),
                    },
                ],
            }],
            preview_config: PreviewConfig::default(),
            tab_colors: Vec::new(),
            tab_color_style: TabColorStyle::default(),
        };
        let i = info("notepad.exe", "", "");
        assert_eq!(engine.apply(&i), Some("AnyMode"));
    }

    #[test]
    fn engine_no_match_returns_none() {
        let engine = RulesEngine {
            groups: vec![RuleGroup {
                name: "X".into(),
                enabled: true,
                match_mode: MatchMode::All,
                rules: vec![WindowRule {
                    field: RuleField::ProcessName,
                    matcher: Matcher::Equals("x.exe".into(), false),
                }],
            }],
            preview_config: PreviewConfig::default(),
            tab_colors: Vec::new(),
            tab_color_style: TabColorStyle::default(),
        };
        let i = info("y.exe", "", "");
        assert_eq!(engine.apply(&i), None);
    }

    #[test]
    fn engine_empty_has_no_rules() {
        let engine = RulesEngine::empty();
        assert!(!engine.has_rules());
    }

    // ── Load tests ──

    #[test]
    fn load_missing_file_returns_empty() {
        let engine = RulesEngine::load(Path::new("nonexistent_wintab_test.yaml"));
        assert!(!engine.has_rules());
    }

    #[test]
    fn load_invalid_yaml_returns_empty() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bad.yaml");
        std::fs::write(&path, "{{{{not yaml").unwrap();
        let engine = RulesEngine::load(&path);
        assert!(!engine.has_rules());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_bad_regex_skips_pattern() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bad_regex.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"rules:
  - name: "Bad"
    patterns:
      - field: process_name
        op: regex
        value: "[invalid("
"#
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        // Group skipped because all patterns failed
        assert!(!engine.has_rules());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_valid_yaml() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("valid.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"rules:
  - name: "Terminals"
    match: any
    patterns:
      - field: process_name
        op: equals
        value: "WindowsTerminal.exe"
      - field: class_name
        op: equals
        value: "CASCADIA_HOSTING_WINDOW_CLASS"
"#
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert!(engine.has_rules());
        assert_eq!(engine.groups.len(), 1);
        assert_eq!(engine.groups[0].name, "Terminals");
        assert_eq!(engine.groups[0].match_mode, MatchMode::Any);
        assert_eq!(engine.groups[0].rules.len(), 2);

        let i = info("WindowsTerminal.exe", "", "");
        assert_eq!(engine.apply(&i), Some("Terminals"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reload_picks_up_new_rules() {
        let dir = std::env::temp_dir().join("wintab_test_reload");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("reload.yaml");

        // Initial config: one rule group
        std::fs::write(
            &path,
            r#"rules:
  - name: "Editors"
    patterns:
      - field: process_name
        op: equals
        value: "code.exe"
"#,
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert_eq!(engine.groups.len(), 1);
        assert_eq!(engine.groups[0].name, "Editors");

        // Modify config: two rule groups
        std::fs::write(
            &path,
            r#"rules:
  - name: "Editors"
    patterns:
      - field: process_name
        op: equals
        value: "code.exe"
  - name: "Terminals"
    patterns:
      - field: process_name
        op: equals
        value: "WindowsTerminal.exe"
"#,
        )
        .unwrap();

        // Reload
        let engine = RulesEngine::load(&path);
        assert_eq!(engine.groups.len(), 2);
        assert_eq!(engine.groups[0].name, "Editors");
        assert_eq!(engine.groups[1].name, "Terminals");

        let i = info("WindowsTerminal.exe", "", "");
        assert_eq!(engine.apply(&i), Some("Terminals"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_bad_field_skips_pattern() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bad_field.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"rules:
  - name: "X"
    patterns:
      - field: unknown_field
        op: equals
        value: "test"
"#
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        // Group skipped — no valid patterns
        assert!(!engine.has_rules());
        let _ = std::fs::remove_file(&path);
    }

    // ── Unit 8: command_line field tests ──

    #[test]
    fn parse_field_command_line() {
        assert_eq!(parse_field("command_line"), Some(RuleField::CommandLine));
    }

    #[test]
    fn command_line_rule_matches() {
        let engine = RulesEngine {
            groups: vec![RuleGroup {
                name: "DevTools".into(),
                enabled: true,
                match_mode: MatchMode::All,
                rules: vec![WindowRule {
                    field: RuleField::CommandLine,
                    matcher: Matcher::Contains("--profile=dev".into(), false),
                }],
            }],
            preview_config: PreviewConfig::default(),
            tab_colors: Vec::new(),
            tab_color_style: TabColorStyle::default(),
        };
        let i = info_with_cmd("app.exe", "", "", "app.exe --profile=dev --verbose");
        assert_eq!(engine.apply(&i), Some("DevTools"));

        let i2 = info_with_cmd("app.exe", "", "", "app.exe --profile=prod");
        assert_eq!(engine.apply(&i2), None);
    }

    #[test]
    fn load_command_line_yaml() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("cmd_line.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"rules:
  - name: "DevApps"
    patterns:
      - field: command_line
        op: contains
        value: "--dev-mode"
"#
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert!(engine.has_rules());
        assert!(engine.uses_command_line());
        assert_eq!(engine.groups[0].rules[0].field, RuleField::CommandLine);

        let i = info_with_cmd("app.exe", "", "", "app.exe --dev-mode");
        assert_eq!(engine.apply(&i), Some("DevApps"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn uses_command_line_false_when_not_referenced() {
        let engine = RulesEngine {
            groups: vec![RuleGroup {
                name: "X".into(),
                enabled: true,
                match_mode: MatchMode::All,
                rules: vec![WindowRule {
                    field: RuleField::ProcessName,
                    matcher: Matcher::Equals("x.exe".into(), false),
                }],
            }],
            preview_config: PreviewConfig::default(),
            tab_colors: Vec::new(),
            tab_color_style: TabColorStyle::default(),
        };
        assert!(!engine.uses_command_line());
    }

    // ── Unit 9: negation operator tests ──

    #[test]
    fn matcher_not_equals_matches_when_different() {
        let m = Matcher::NotEquals("foo.exe".into(), false);
        assert!(m.matches("bar.exe"));
        assert!(m.matches("BAR.EXE"));
    }

    #[test]
    fn matcher_not_equals_no_match_when_equal() {
        let m = Matcher::NotEquals("foo.exe".into(), false);
        assert!(!m.matches("foo.exe"));
        assert!(!m.matches("FOO.EXE"));
    }

    #[test]
    fn matcher_not_equals_case_sensitive() {
        let m = Matcher::NotEquals("Foo.exe".into(), true);
        assert!(m.matches("foo.exe")); // different case = not equal
        assert!(!m.matches("Foo.exe")); // exact match = equal
    }

    #[test]
    fn matcher_not_contains_matches_when_absent() {
        let m = Matcher::NotContains("debug".into(), false);
        assert!(m.matches("release-build"));
        assert!(m.matches("PRODUCTION"));
    }

    #[test]
    fn matcher_not_contains_no_match_when_present() {
        let m = Matcher::NotContains("debug".into(), false);
        assert!(!m.matches("debug-build"));
        assert!(!m.matches("has-DEBUG-flag"));
    }

    #[test]
    fn matcher_not_contains_case_sensitive() {
        let m = Matcher::NotContains("Debug".into(), true);
        assert!(m.matches("debug-build")); // wrong case = absent
        assert!(!m.matches("Debug-build")); // exact case = present
    }

    #[test]
    fn load_negation_operators_yaml() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("negation.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"rules:
  - name: "NonDebug"
    match: all
    patterns:
      - field: process_name
        op: not_equals
        value: "debug.exe"
      - field: title
        op: not_contains
        value: "[DEBUG]"
"#
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert!(engine.has_rules());
        assert_eq!(engine.groups[0].rules.len(), 2);

        // Neither condition triggers exclusion
        let i = info("app.exe", "", "My App");
        assert_eq!(engine.apply(&i), Some("NonDebug"));

        // process_name matches exclusion
        let i2 = info("debug.exe", "", "My App");
        assert_eq!(engine.apply(&i2), None);

        // title contains exclusion substring
        let i3 = info("app.exe", "", "My App [DEBUG]");
        assert_eq!(engine.apply(&i3), None);

        let _ = std::fs::remove_file(&path);
    }

    // ── Unit 11: duplicate group name tests ──

    #[test]
    fn load_duplicate_group_names_keeps_first() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("dups.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"rules:
  - name: "Browsers"
    patterns:
      - field: process_name
        op: equals
        value: "chrome.exe"
  - name: "Browsers"
    patterns:
      - field: process_name
        op: equals
        value: "firefox.exe"
  - name: "Editors"
    patterns:
      - field: process_name
        op: equals
        value: "code.exe"
"#
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert!(engine.has_rules());
        // Only 2 groups: first "Browsers" + "Editors" (second "Browsers" skipped)
        assert_eq!(engine.groups.len(), 2);
        assert_eq!(engine.groups[0].name, "Browsers");
        assert_eq!(engine.groups[1].name, "Editors");

        // First "Browsers" group matches chrome, not firefox
        let i_chrome = info("chrome.exe", "", "");
        assert_eq!(engine.apply(&i_chrome), Some("Browsers"));

        let i_firefox = info("firefox.exe", "", "");
        assert_eq!(engine.apply(&i_firefox), None);

        let _ = std::fs::remove_file(&path);
    }

    // ── Preview config tests ──

    #[test]
    fn load_with_preview_section() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("preview_full.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"preview:
  width: 400
  max_height: 500
  opacity: 180
  delay_ms: 250
"#
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert_eq!(engine.preview_config.width, 400);
        assert_eq!(engine.preview_config.max_height, 500);
        assert_eq!(engine.preview_config.opacity, 180);
        assert_eq!(engine.preview_config.delay_ms, 250);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_without_preview_section_uses_defaults() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("no_preview.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"rules: []
"#
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert_eq!(engine.preview_config.width, 300);
        assert_eq!(engine.preview_config.max_height, 400);
        assert_eq!(engine.preview_config.opacity, 200);
        assert_eq!(engine.preview_config.delay_ms, 500);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_partial_preview_config() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("preview_partial.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"preview:
  width: 450
  opacity: 150
"#
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert_eq!(engine.preview_config.width, 450);
        assert_eq!(engine.preview_config.max_height, 400); // default
        assert_eq!(engine.preview_config.opacity, 150);
        assert_eq!(engine.preview_config.delay_ms, 500); // default
        let _ = std::fs::remove_file(&path);
    }

    // ── Tab color tests ──

    #[test]
    fn parse_hex_color_basic() {
        // #2E8B57 → R=0x2E, G=0x8B, B=0x57 → GDI = 0x00578B2E
        assert_eq!(parse_hex_color("#2E8B57"), Some(0x00578B2E));
    }

    #[test]
    fn parse_hex_color_without_hash() {
        assert_eq!(parse_hex_color("FF0000"), Some(0x000000FF));
    }

    #[test]
    fn parse_hex_color_white() {
        assert_eq!(parse_hex_color("#FFFFFF"), Some(0x00FFFFFF));
    }

    #[test]
    fn parse_hex_color_black() {
        assert_eq!(parse_hex_color("#000000"), Some(0x00000000));
    }

    #[test]
    fn parse_hex_color_pure_red() {
        // #FF0000 → R=0xFF, G=0, B=0 → GDI = 0x000000FF
        assert_eq!(parse_hex_color("#FF0000"), Some(0x000000FF));
    }

    #[test]
    fn parse_hex_color_pure_blue() {
        // #0000FF → R=0, G=0, B=0xFF → GDI = 0x00FF0000
        assert_eq!(parse_hex_color("#0000FF"), Some(0x00FF0000));
    }

    #[test]
    fn parse_hex_color_invalid_length() {
        assert_eq!(parse_hex_color("#FFF"), None);
        assert_eq!(parse_hex_color("#FFFFFFF"), None);
    }

    #[test]
    fn parse_hex_color_invalid_chars() {
        assert_eq!(parse_hex_color("#GGHHII"), None);
    }

    #[test]
    fn tab_color_style_default_is_tint_stripe() {
        assert_eq!(TabColorStyle::default(), TabColorStyle::TintStripe);
    }

    #[test]
    fn tab_color_style_serde_roundtrip() {
        let styles = [
            ("\"bottom_stripe\"", TabColorStyle::BottomStripe),
            ("\"left_bar\"", TabColorStyle::LeftBar),
            ("\"top_stripe\"", TabColorStyle::TopStripe),
            ("\"full_tint\"", TabColorStyle::FullTint),
            ("\"tint_stripe\"", TabColorStyle::TintStripe),
        ];
        for (yaml, expected) in styles {
            let parsed: TabColorStyle = serde_yaml::from_str(yaml).unwrap();
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn load_tab_colors_yaml() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("tab_colors.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r##"tab_color_style: bottom_stripe

tab_colors:
  - pattern:
      field: title
      op: contains
      value: "SSH: rok5"
    color: "#2E8B57"
  - pattern:
      field: title
      op: contains
      value: "SSH: rok7"
    color: "#CD5C5C"
"##
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert_eq!(engine.tab_color_style, TabColorStyle::BottomStripe);
        assert_eq!(engine.tab_colors.len(), 2);
        assert_eq!(engine.tab_colors[0].color, 0x00578B2E); // #2E8B57 swapped
        assert_eq!(engine.tab_colors[1].color, 0x005C5CCD); // #CD5C5C swapped

        let i = info("code.exe", "", "SSH: rok5 - main.rs");
        assert_eq!(engine.match_tab_color(&i), Some(0x00578B2E));

        let i2 = info("code.exe", "", "something else");
        assert_eq!(engine.match_tab_color(&i2), None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_tab_colors_invalid_color_skipped() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("tab_colors_bad.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r##"tab_colors:
  - pattern:
      field: title
      op: contains
      value: "test"
    color: "not-a-color"
  - pattern:
      field: title
      op: contains
      value: "test2"
    color: "#FF0000"
"##
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert_eq!(engine.tab_colors.len(), 1);
        assert_eq!(engine.tab_colors[0].color, 0x000000FF); // #FF0000 → GDI
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn uses_command_line_includes_tab_color_rules() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("tab_colors_cmd.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r##"tab_colors:
  - pattern:
      field: command_line
      op: contains
      value: "--ssh"
    color: "#00FF00"
"##
        )
        .unwrap();
        let engine = RulesEngine::load(&path);
        assert!(engine.uses_command_line());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_default_tab_color_style() {
        let dir = std::env::temp_dir().join("wintab_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("no_style.yaml");
        std::fs::write(&path, "rules: []\n").unwrap();
        let engine = RulesEngine::load(&path);
        assert_eq!(engine.tab_color_style, TabColorStyle::TintStripe);
        let _ = std::fs::remove_file(&path);
    }
}
