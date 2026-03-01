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

// ── Serde schema ──

#[derive(Deserialize)]
struct ConfigFile {
    #[serde(default)]
    rules: Vec<RuleGroupDef>,
    #[serde(default)]
    preview: Option<PreviewConfig>,
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

/// Holds all parsed rules. Created once at startup.
pub struct RulesEngine {
    pub groups: Vec<RuleGroup>,
    pub preview_config: PreviewConfig,
}

/// Info needed to evaluate rules against a window.
pub struct WindowRuleInfo<'a> {
    pub process_name: &'a str,
    pub class_name: &'a str,
    pub title: &'a str,
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
            Matcher::Regex(re) => re.is_match(value),
        }
    }
}

impl WindowRule {
    fn matches(&self, info: &WindowRuleInfo) -> bool {
        let value = match self.field {
            RuleField::ProcessName => info.process_name,
            RuleField::ClassName => info.class_name,
            RuleField::Title => info.title,
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

        let groups = config
            .rules
            .into_iter()
            .filter_map(|def| {
                let rules: Vec<WindowRule> =
                    def.patterns.into_iter().filter_map(parse_pattern).collect();
                if rules.is_empty() {
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
        RulesEngine {
            groups,
            preview_config,
        }
    }

    pub fn empty() -> Self {
        RulesEngine {
            groups: Vec::new(),
            preview_config: PreviewConfig::default(),
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
}

fn parse_field(s: &str) -> Option<RuleField> {
    match s {
        "process_name" => Some(RuleField::ProcessName),
        "class_name" => Some(RuleField::ClassName),
        "title" => Some(RuleField::Title),
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
        // Leak strings for test convenience
        WindowRuleInfo {
            process_name: Box::leak(process.to_string().into_boxed_str()),
            class_name: Box::leak(class.to_string().into_boxed_str()),
            title: Box::leak(title.to_string().into_boxed_str()),
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
}
