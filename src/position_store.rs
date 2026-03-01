use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RectDef {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PositionEntry {
    pub process_name: String,
    pub class_name: String,
    pub title: String,
    pub rect: RectDef,
    pub dpi: u32,
    pub last_seen: u64,
    pub hit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemberKey {
    pub process_name: String,
    pub class_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupPositionEntry {
    pub group_name: String,
    pub member_keys: Vec<MemberKey>,
    pub rect: RectDef,
    pub dpi: u32,
    pub last_seen: u64,
}

#[derive(Serialize, Deserialize)]
struct PositionFile {
    version: u32,
    #[serde(default)]
    entries: Vec<PositionEntry>,
    #[serde(default)]
    group_entries: Vec<GroupPositionEntry>,
}

const FLUSH_THRESHOLD: usize = 20;
const MAX_ENTRIES: usize = 500;
const STALE_SECS: u64 = 30 * 24 * 3600; // 30 days

pub struct PositionStore {
    entries: Vec<PositionEntry>,
    group_entries: Vec<GroupPositionEntry>,
    path: PathBuf,
    dirty_count: usize,
}

impl PositionStore {
    pub fn load(path: &Path) -> Self {
        let (entries, group_entries) = match std::fs::read_to_string(path) {
            Ok(content) => match serde_yaml::from_str::<PositionFile>(&content) {
                Ok(f) => (f.entries, f.group_entries),
                Err(_) => (Vec::new(), Vec::new()),
            },
            Err(_) => (Vec::new(), Vec::new()),
        };
        PositionStore {
            entries,
            group_entries,
            path: path.to_path_buf(),
            dirty_count: 0,
        }
    }

    pub fn empty() -> Self {
        PositionStore {
            entries: Vec::new(),
            group_entries: Vec::new(),
            path: PathBuf::new(),
            dirty_count: 0,
        }
    }

    /// Exact match first, then fuzzy title match. Returns the best entry.
    pub fn lookup(
        &self,
        process_name: &str,
        class_name: &str,
        title: &str,
    ) -> Option<&PositionEntry> {
        // Exact match
        if let Some(e) = self.entries.iter().find(|e| {
            e.process_name == process_name && e.class_name == class_name && e.title == title
        }) {
            return Some(e);
        }

        // Fuzzy title match (same process + class, close title)
        self.entries.iter().find(|e| {
            e.process_name == process_name
                && e.class_name == class_name
                && fuzzy_title_match(&e.title, title)
        })
    }

    /// Upsert an entry. Auto-flushes after FLUSH_THRESHOLD dirty writes.
    pub fn record(
        &mut self,
        process_name: &str,
        class_name: &str,
        title: &str,
        rect: RectDef,
        dpi: u32,
    ) {
        let now = current_timestamp();

        // Try to find an existing exact match to update
        if let Some(e) = self.entries.iter_mut().find(|e| {
            e.process_name == process_name && e.class_name == class_name && e.title == title
        }) {
            e.rect = rect;
            e.dpi = dpi;
            e.last_seen = now;
            e.hit_count += 1;
            self.dirty_count += 1;
            if self.dirty_count >= FLUSH_THRESHOLD {
                self.flush();
            }
            return;
        }

        // Insert new entry
        self.entries.push(PositionEntry {
            process_name: process_name.to_string(),
            class_name: class_name.to_string(),
            title: title.to_string(),
            rect,
            dpi,
            last_seen: now,
            hit_count: 1,
        });
        self.dirty_count += 1;
        if self.dirty_count >= FLUSH_THRESHOLD {
            self.flush();
        }
    }

    /// Upsert a group position entry by group_name.
    pub fn record_group(
        &mut self,
        name: &str,
        member_keys: Vec<MemberKey>,
        rect: RectDef,
        dpi: u32,
    ) {
        let now = current_timestamp();

        if let Some(e) = self.group_entries.iter_mut().find(|e| e.group_name == name) {
            e.member_keys = member_keys;
            e.rect = rect;
            e.dpi = dpi;
            e.last_seen = now;
            self.dirty_count += 1;
            if self.dirty_count >= FLUSH_THRESHOLD {
                self.flush();
            }
            return;
        }

        self.group_entries.push(GroupPositionEntry {
            group_name: name.to_string(),
            member_keys,
            rect,
            dpi,
            last_seen: now,
        });
        self.dirty_count += 1;
        if self.dirty_count >= FLUSH_THRESHOLD {
            self.flush();
        }
    }

    /// Lookup a group position entry by name. Optionally verify member_keys overlap.
    pub fn lookup_group(
        &self,
        name: &str,
        member_keys: &[MemberKey],
    ) -> Option<&GroupPositionEntry> {
        self.group_entries.iter().find(|e| {
            if e.group_name != name {
                return false;
            }
            // If caller provided member_keys, verify at least one overlaps
            if !member_keys.is_empty() && !e.member_keys.is_empty() {
                return member_keys.iter().any(|mk| e.member_keys.contains(mk));
            }
            true
        })
    }

    /// Write entries to disk, evicting stale and capping at MAX_ENTRIES.
    pub fn flush(&mut self) {
        self.dirty_count = 0;

        let now = current_timestamp();

        // Evict stale entries
        self.entries.retain(|e| now - e.last_seen < STALE_SECS);
        self.group_entries
            .retain(|e| now - e.last_seen < STALE_SECS);

        // Cap at MAX_ENTRIES (keep most recently seen)
        if self.entries.len() > MAX_ENTRIES {
            self.entries.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
            self.entries.truncate(MAX_ENTRIES);
        }

        if self.path.as_os_str().is_empty() {
            return;
        }

        let file = PositionFile {
            version: 1,
            entries: self.entries.clone(),
            group_entries: self.group_entries.clone(),
        };
        if let Ok(yaml) = serde_yaml::to_string(&file) {
            let _ = std::fs::write(&self.path, yaml);
        }
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Levenshtein distance between two strings.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Returns true if titles are within 20% Levenshtein distance of the shorter string.
pub fn fuzzy_title_match(stored: &str, candidate: &str) -> bool {
    let min_len = stored.len().min(candidate.len());
    if min_len == 0 {
        return false;
    }
    let dist = levenshtein(stored, candidate);
    let threshold = min_len / 5; // 20%
    dist <= threshold
}

/// Check if any monitor contains the given rect (via MonitorFromRect).
#[cfg(not(test))]
pub fn monitor_exists_for_rect(rect: &RectDef) -> bool {
    use windows_sys::Win32::Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONULL};
    let win_rect = windows_sys::Win32::Foundation::RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    };
    unsafe { !MonitorFromRect(&win_rect, MONITOR_DEFAULTTONULL).is_null() }
}

#[cfg(test)]
pub fn monitor_exists_for_rect(_rect: &RectDef) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn levenshtein_one_edit() {
        assert_eq!(levenshtein("hello", "hallo"), 1);
    }

    #[test]
    fn levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
    }

    #[test]
    fn levenshtein_completely_different() {
        assert_eq!(levenshtein("abc", "xyz"), 3);
    }

    #[test]
    fn fuzzy_match_identical() {
        assert!(fuzzy_title_match(
            "Visual Studio Code",
            "Visual Studio Code"
        ));
    }

    #[test]
    fn fuzzy_match_close() {
        // "Visual Studio Code" vs "Visual Studio Code - main.rs" — too different (long suffix)
        assert!(!fuzzy_title_match(
            "Visual Studio Code",
            "Visual Studio Code - main.rs"
        ));
    }

    #[test]
    fn fuzzy_match_small_change() {
        // 20 chars, threshold = 4
        assert!(fuzzy_title_match(
            "Visual Studio Code!!",
            "Visual Studio Code! "
        ));
    }

    #[test]
    fn fuzzy_match_empty() {
        assert!(!fuzzy_title_match("", "anything"));
        assert!(!fuzzy_title_match("anything", ""));
    }

    #[test]
    fn lookup_exact_match() {
        let mut store = PositionStore::empty();
        store.record(
            "code.exe",
            "Chrome_WidgetWin_1",
            "main.rs",
            rect(0, 0, 800, 600),
            96,
        );
        let result = store.lookup("code.exe", "Chrome_WidgetWin_1", "main.rs");
        assert!(result.is_some());
        assert_eq!(result.unwrap().rect.right, 800);
    }

    #[test]
    fn lookup_fuzzy_fallback() {
        let mut store = PositionStore::empty();
        store.record(
            "code.exe",
            "CW1",
            "main.rs - Visual Studio",
            rect(10, 20, 800, 600),
            96,
        );
        // Exact title doesn't match, fuzzy should (only 1 char diff in 23-char string)
        let result = store.lookup("code.exe", "CW1", "main.rs - Visual Studi0");
        assert!(result.is_some());
    }

    #[test]
    fn lookup_no_match() {
        let mut store = PositionStore::empty();
        store.record("code.exe", "CW1", "main.rs", rect(0, 0, 800, 600), 96);
        assert!(store.lookup("notepad.exe", "Notepad", "Untitled").is_none());
    }

    #[test]
    fn record_upserts() {
        let mut store = PositionStore::empty();
        store.record("a.exe", "C", "T", rect(0, 0, 100, 100), 96);
        store.record("a.exe", "C", "T", rect(10, 10, 200, 200), 96);
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].rect.right, 200);
        assert_eq!(store.entries[0].hit_count, 2);
    }

    #[test]
    fn flush_evicts_stale() {
        let mut store = PositionStore::empty();
        store.entries.push(PositionEntry {
            process_name: "old.exe".into(),
            class_name: "C".into(),
            title: "T".into(),
            rect: RectDef {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            dpi: 96,
            last_seen: 0, // epoch — very old
            hit_count: 1,
        });
        store.entries.push(PositionEntry {
            process_name: "new.exe".into(),
            class_name: "C".into(),
            title: "T".into(),
            rect: RectDef {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            dpi: 96,
            last_seen: current_timestamp(),
            hit_count: 1,
        });
        store.flush();
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].process_name, "new.exe");
    }

    #[test]
    fn flush_caps_at_max() {
        let mut store = PositionStore::empty();
        let now = current_timestamp();
        for i in 0..600 {
            store.entries.push(PositionEntry {
                process_name: format!("app{}.exe", i),
                class_name: "C".into(),
                title: "T".into(),
                rect: RectDef {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100,
                },
                dpi: 96,
                last_seen: now - i as u64,
                hit_count: 1,
            });
        }
        store.flush();
        assert_eq!(store.entries.len(), MAX_ENTRIES);
    }

    fn rect(l: i32, t: i32, r: i32, b: i32) -> RectDef {
        RectDef {
            left: l,
            top: t,
            right: r,
            bottom: b,
        }
    }

    fn mk(process: &str, class: &str) -> MemberKey {
        MemberKey {
            process_name: process.to_string(),
            class_name: class.to_string(),
        }
    }

    // --- Group position entry tests ---

    #[test]
    fn record_group_and_lookup() {
        let mut store = PositionStore::empty();
        let keys = vec![
            mk("code.exe", "CW1"),
            mk("terminal.exe", "ConsoleWindowClass"),
        ];
        store.record_group("Editors", keys.clone(), rect(100, 100, 900, 700), 96);

        let result = store.lookup_group("Editors", &keys);
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.group_name, "Editors");
        assert_eq!(entry.rect.left, 100);
        assert_eq!(entry.rect.right, 900);
        assert_eq!(entry.dpi, 96);
        assert_eq!(entry.member_keys.len(), 2);
    }

    #[test]
    fn record_group_upserts() {
        let mut store = PositionStore::empty();
        let keys = vec![mk("a.exe", "C")];
        store.record_group("G1", keys.clone(), rect(0, 0, 100, 100), 96);
        store.record_group("G1", keys.clone(), rect(50, 50, 200, 200), 120);
        assert_eq!(store.group_entries.len(), 1);
        assert_eq!(store.group_entries[0].rect.right, 200);
        assert_eq!(store.group_entries[0].dpi, 120);
    }

    #[test]
    fn lookup_group_no_match() {
        let store = PositionStore::empty();
        assert!(store.lookup_group("Missing", &[]).is_none());
    }

    #[test]
    fn lookup_group_with_member_key_overlap() {
        let mut store = PositionStore::empty();
        let keys = vec![mk("a.exe", "C1"), mk("b.exe", "C2")];
        store.record_group("G1", keys, rect(0, 0, 100, 100), 96);

        // Partial overlap should match
        let query_keys = vec![mk("a.exe", "C1")];
        assert!(store.lookup_group("G1", &query_keys).is_some());

        // No overlap should not match
        let no_overlap = vec![mk("z.exe", "Z")];
        assert!(store.lookup_group("G1", &no_overlap).is_none());
    }

    #[test]
    fn lookup_group_empty_keys_matches_any() {
        let mut store = PositionStore::empty();
        let keys = vec![mk("a.exe", "C1")];
        store.record_group("G1", keys, rect(0, 0, 100, 100), 96);

        // Empty query keys should match (no overlap check)
        assert!(store.lookup_group("G1", &[]).is_some());
    }

    #[test]
    fn group_entry_serialization_roundtrip() {
        let mut store = PositionStore::empty();
        let keys = vec![mk("code.exe", "CW1"), mk("term.exe", "Console")];
        store.record_group("Dev", keys.clone(), rect(10, 20, 800, 600), 96);

        // Serialize
        let file = PositionFile {
            version: 1,
            entries: store.entries.clone(),
            group_entries: store.group_entries.clone(),
        };
        let yaml = serde_yaml::to_string(&file).unwrap();

        // Deserialize
        let parsed: PositionFile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.group_entries.len(), 1);
        assert_eq!(parsed.group_entries[0].group_name, "Dev");
        assert_eq!(parsed.group_entries[0].member_keys.len(), 2);
        assert_eq!(parsed.group_entries[0].rect.right, 800);
    }

    #[test]
    fn backward_compatibility_no_group_entries() {
        // YAML with no group_entries field — should deserialize cleanly
        let yaml = "version: 1\nentries:\n  - process_name: a.exe\n    class_name: C\n    title: T\n    rect:\n      left: 0\n      top: 0\n      right: 100\n      bottom: 100\n    dpi: 96\n    last_seen: 1000000\n    hit_count: 1\n";
        let parsed: PositionFile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert!(parsed.group_entries.is_empty());
    }

    #[test]
    fn flush_evicts_stale_group_entries() {
        let mut store = PositionStore::empty();
        store.group_entries.push(GroupPositionEntry {
            group_name: "Old".into(),
            member_keys: vec![mk("old.exe", "C")],
            rect: rect(0, 0, 100, 100),
            dpi: 96,
            last_seen: 0, // epoch — very old
        });
        store.group_entries.push(GroupPositionEntry {
            group_name: "New".into(),
            member_keys: vec![mk("new.exe", "C")],
            rect: rect(0, 0, 100, 100),
            dpi: 96,
            last_seen: current_timestamp(),
        });
        store.flush();
        assert_eq!(store.group_entries.len(), 1);
        assert_eq!(store.group_entries[0].group_name, "New");
    }
}
