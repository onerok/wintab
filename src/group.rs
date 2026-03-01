use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::window;

pub type GroupId = u64;

static NEXT_GROUP_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> GroupId {
    NEXT_GROUP_ID.fetch_add(1, Ordering::Relaxed)
}

/// A tab group: multiple windows sharing one screen position.
#[allow(dead_code)]
pub struct TabGroup {
    pub id: GroupId,
    pub tabs: Vec<HWND>,
    pub active: usize,
}

impl TabGroup {
    pub fn active_hwnd(&self) -> HWND {
        debug_assert!(!self.tabs.is_empty(), "active_hwnd called on empty group");
        self.tabs.get(self.active).copied().unwrap_or(std::ptr::null_mut())
    }

    /// Switch to a different tab by index.
    pub fn switch_to(&mut self, index: usize) {
        if index >= self.tabs.len() || index == self.active {
            return;
        }

        let old = self.tabs[self.active];
        let new = self.tabs[index];
        self.active = index;

        unsafe {
            let rect = window::get_window_rect(old);
            ShowWindow(old, SW_HIDE);
            SetWindowPos(
                new,
                0 as _,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
            );
            ShowWindow(new, SW_SHOW);
            SetForegroundWindow(new);
        }
    }

    /// Add a window to this group.
    pub fn add(&mut self, hwnd: HWND) {
        if self.tabs.contains(&hwnd) {
            return;
        }

        let active = self.active_hwnd();
        let rect = window::get_window_rect(active);

        unsafe {
            SetWindowPos(
                hwnd,
                0 as _,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
            ShowWindow(hwnd, SW_HIDE);
        }

        self.tabs.push(hwnd);
        self.switch_to(self.tabs.len() - 1);
    }

    /// Remove a window from this group, making it standalone again.
    /// Returns true if the group should be dissolved (0 or 1 windows left).
    pub fn remove(&mut self, hwnd: HWND) -> bool {
        let Some(idx) = self.tabs.iter().position(|&h| h == hwnd) else {
            return false;
        };

        self.tabs.remove(idx);

        unsafe {
            ShowWindow(hwnd, SW_SHOW);
        }

        if self.tabs.len() <= 1 {
            if let Some(&remaining) = self.tabs.first() {
                unsafe {
                    ShowWindow(remaining, SW_SHOW);
                }
            }
            return true;
        }

        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if idx < self.active {
            self.active -= 1;
        } else if idx == self.active {
            let new_active = self.active.min(self.tabs.len() - 1);
            self.active = new_active;
            unsafe {
                let h = self.tabs[self.active];
                ShowWindow(h, SW_SHOW);
                SetForegroundWindow(h);
            }
        }

        false
    }

    /// Sync position of all hidden windows to match the active window.
    pub fn sync_positions(&self) {
        let active = self.active_hwnd();
        let rect = window::get_window_rect(active);

        unsafe {
            for (i, &hwnd) in self.tabs.iter().enumerate() {
                if i != self.active {
                    SetWindowPos(
                        hwnd,
                        0 as _,
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOREDRAW,
                    );
                }
            }
        }
    }

    /// Minimize all windows in the group.
    pub fn minimize_all(&self) {
        unsafe {
            for &hwnd in &self.tabs {
                ShowWindow(hwnd, SW_MINIMIZE);
            }
        }
    }

    /// Restore the group: show the active tab, keep others hidden.
    pub fn restore(&mut self) {
        unsafe {
            let active = self.tabs[self.active];
            ShowWindow(active, SW_RESTORE);
            SetForegroundWindow(active);
        }
    }

    /// Get the screen rect of the active window (for overlay positioning).
    pub fn active_rect(&self) -> RECT {
        window::get_window_rect(self.active_hwnd())
    }
}

/// Manager that tracks all groups.
pub struct GroupManager {
    pub groups: HashMap<GroupId, TabGroup>,
    pub window_to_group: HashMap<HWND, GroupId>,
}

impl GroupManager {
    pub fn new() -> Self {
        GroupManager {
            groups: HashMap::new(),
            window_to_group: HashMap::new(),
        }
    }

    pub fn create_group(&mut self, hwnd_a: HWND, hwnd_b: HWND) -> GroupId {
        self.remove_from_group(hwnd_a);
        self.remove_from_group(hwnd_b);

        let id = next_id();
        let rect = window::get_window_rect(hwnd_a);

        unsafe {
            SetWindowPos(
                hwnd_b,
                0 as _,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
            ShowWindow(hwnd_a, SW_HIDE);
            ShowWindow(hwnd_b, SW_SHOW);
            SetForegroundWindow(hwnd_b);
        }

        let group = TabGroup {
            id,
            tabs: vec![hwnd_a, hwnd_b],
            active: 1,
        };

        self.window_to_group.insert(hwnd_a, id);
        self.window_to_group.insert(hwnd_b, id);
        self.groups.insert(id, group);
        id
    }

    pub fn add_to_group(&mut self, group_id: GroupId, hwnd: HWND) {
        self.remove_from_group(hwnd);
        if let Some(group) = self.groups.get_mut(&group_id) {
            group.add(hwnd);
            self.window_to_group.insert(hwnd, group_id);
        }
    }

    pub fn remove_from_group(&mut self, hwnd: HWND) {
        if let Some(group_id) = self.window_to_group.remove(&hwnd) {
            let dissolve = if let Some(group) = self.groups.get_mut(&group_id) {
                group.remove(hwnd)
            } else {
                false
            };

            if dissolve {
                if let Some(group) = self.groups.remove(&group_id) {
                    for &h in &group.tabs {
                        self.window_to_group.remove(&h);
                    }
                }
            }
        }
    }

    pub fn group_of(&self, hwnd: HWND) -> Option<GroupId> {
        self.window_to_group.get(&hwnd).copied()
    }

    pub fn is_active_in_group(&self, gid: GroupId, hwnd: HWND) -> bool {
        self.groups
            .get(&gid)
            .map(|g| !g.tabs.is_empty() && g.active_hwnd() == hwnd)
            .unwrap_or(false)
    }

    pub fn show_all_windows(&self) {
        unsafe {
            for group in self.groups.values() {
                for &hwnd in &group.tabs {
                    if window::is_window_valid(hwnd) {
                        ShowWindow(hwnd, SW_SHOW);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_hwnd(n: usize) -> HWND {
        n as HWND
    }

    // --- TabGroup::active_hwnd ---

    #[test]
    fn active_hwnd_returns_first_tab() {
        let group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200)],
            active: 0,
        };
        assert_eq!(group.active_hwnd(), fake_hwnd(100));
    }

    #[test]
    fn active_hwnd_returns_second_tab() {
        let group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200)],
            active: 1,
        };
        assert_eq!(group.active_hwnd(), fake_hwnd(200));
    }

    #[test]
    #[should_panic(expected = "active_hwnd called on empty group")]
    fn active_hwnd_empty_panics_in_debug() {
        let group = TabGroup {
            id: 1,
            tabs: vec![],
            active: 0,
        };
        let _ = group.active_hwnd();
    }

    #[test]
    fn active_hwnd_out_of_bounds_returns_null() {
        let group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100)],
            active: 5,
        };
        assert!(group.active_hwnd().is_null());
    }

    // --- next_id ---

    #[test]
    fn next_id_increments() {
        let a = next_id();
        let b = next_id();
        assert!(b > a);
    }

    // --- GroupManager ---

    #[test]
    fn group_of_returns_none_for_unknown() {
        let gm = GroupManager::new();
        assert_eq!(gm.group_of(fake_hwnd(999)), None);
    }

    #[test]
    fn group_of_returns_id_when_present() {
        let mut gm = GroupManager::new();
        gm.window_to_group.insert(fake_hwnd(100), 42);
        assert_eq!(gm.group_of(fake_hwnd(100)), Some(42));
    }

    #[test]
    fn is_active_in_group_true_for_active_window() {
        let mut gm = GroupManager::new();
        let id = 10;
        gm.groups.insert(id, TabGroup {
            id,
            tabs: vec![fake_hwnd(100), fake_hwnd(200)],
            active: 0,
        });
        assert!(gm.is_active_in_group(id, fake_hwnd(100)));
    }

    #[test]
    fn is_active_in_group_false_for_inactive_window() {
        let mut gm = GroupManager::new();
        let id = 10;
        gm.groups.insert(id, TabGroup {
            id,
            tabs: vec![fake_hwnd(100), fake_hwnd(200)],
            active: 0,
        });
        assert!(!gm.is_active_in_group(id, fake_hwnd(200)));
    }

    #[test]
    fn is_active_in_group_false_for_unknown_group() {
        let gm = GroupManager::new();
        assert!(!gm.is_active_in_group(999, fake_hwnd(100)));
    }

    #[test]
    fn is_active_in_group_false_for_empty_group() {
        let mut gm = GroupManager::new();
        let id = 10;
        gm.groups.insert(id, TabGroup {
            id,
            tabs: vec![],
            active: 0,
        });
        assert!(!gm.is_active_in_group(id, fake_hwnd(100)));
    }

    // --- GroupManager remove_from_group (data structure only) ---

    #[test]
    fn remove_from_group_unknown_window_is_noop() {
        let mut gm = GroupManager::new();
        gm.remove_from_group(fake_hwnd(999)); // should not panic
    }

    // --- TabGroup::remove (index adjustment logic) ---
    // Note: Win32 calls (ShowWindow, SetForegroundWindow) on fake HWNDs
    // return failure silently; we're testing the state logic only.

    #[test]
    fn remove_after_active_preserves_index() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200), fake_hwnd(300)],
            active: 0,
        };
        let dissolve = group.remove(fake_hwnd(300));
        assert!(!dissolve);
        assert_eq!(group.tabs, vec![fake_hwnd(100), fake_hwnd(200)]);
        assert_eq!(group.active, 0);
    }

    #[test]
    fn remove_before_active_decrements_index() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200), fake_hwnd(300)],
            active: 2,
        };
        let dissolve = group.remove(fake_hwnd(100));
        assert!(!dissolve);
        assert_eq!(group.tabs, vec![fake_hwnd(200), fake_hwnd(300)]);
        assert_eq!(group.active, 1);
    }

    #[test]
    fn remove_active_selects_successor() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200), fake_hwnd(300)],
            active: 1,
        };
        let dissolve = group.remove(fake_hwnd(200));
        assert!(!dissolve);
        assert_eq!(group.tabs, vec![fake_hwnd(100), fake_hwnd(300)]);
        assert_eq!(group.active, 1);
        assert_eq!(group.active_hwnd(), fake_hwnd(300));
    }

    #[test]
    fn remove_active_at_end_clamps() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200), fake_hwnd(300)],
            active: 2,
        };
        let dissolve = group.remove(fake_hwnd(300));
        assert!(!dissolve);
        assert_eq!(group.tabs, vec![fake_hwnd(100), fake_hwnd(200)]);
        assert_eq!(group.active, 1);
    }

    #[test]
    fn remove_dissolves_two_tab_group() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200)],
            active: 0,
        };
        let dissolve = group.remove(fake_hwnd(100));
        assert!(dissolve);
        assert_eq!(group.tabs, vec![fake_hwnd(200)]);
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200), fake_hwnd(300)],
            active: 1,
        };
        let dissolve = group.remove(fake_hwnd(999));
        assert!(!dissolve);
        assert_eq!(group.tabs.len(), 3);
        assert_eq!(group.active, 1);
    }

    // --- TabGroup::switch_to ---

    #[test]
    fn switch_to_changes_active() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200), fake_hwnd(300)],
            active: 0,
        };
        group.switch_to(2);
        assert_eq!(group.active, 2);
        assert_eq!(group.active_hwnd(), fake_hwnd(300));
    }

    #[test]
    fn switch_to_same_index_is_noop() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200)],
            active: 1,
        };
        group.switch_to(1);
        assert_eq!(group.active, 1);
    }

    #[test]
    fn switch_to_out_of_bounds_is_noop() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200)],
            active: 0,
        };
        group.switch_to(10);
        assert_eq!(group.active, 0);
    }

    // --- TabGroup::add ---

    #[test]
    fn add_appends_and_activates_new_tab() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200)],
            active: 0,
        };
        group.add(fake_hwnd(300));
        assert_eq!(group.tabs, vec![fake_hwnd(100), fake_hwnd(200), fake_hwnd(300)]);
        assert_eq!(group.active, 2);
    }

    #[test]
    fn add_duplicate_is_noop() {
        let mut group = TabGroup {
            id: 1,
            tabs: vec![fake_hwnd(100), fake_hwnd(200)],
            active: 0,
        };
        group.add(fake_hwnd(100));
        assert_eq!(group.tabs.len(), 2);
        assert_eq!(group.active, 0);
    }

    // --- GroupManager lifecycle ---

    #[test]
    fn create_group_sets_up_state() {
        let mut gm = GroupManager::new();
        let gid = gm.create_group(fake_hwnd(100), fake_hwnd(200));
        assert!(gm.groups.contains_key(&gid));
        assert_eq!(gm.group_of(fake_hwnd(100)), Some(gid));
        assert_eq!(gm.group_of(fake_hwnd(200)), Some(gid));
        let group = gm.groups.get(&gid).unwrap();
        assert_eq!(group.tabs, vec![fake_hwnd(100), fake_hwnd(200)]);
        assert_eq!(group.active, 1);
    }

    #[test]
    fn add_to_group_extends_and_tracks() {
        let mut gm = GroupManager::new();
        let gid = gm.create_group(fake_hwnd(100), fake_hwnd(200));
        gm.add_to_group(gid, fake_hwnd(300));
        assert_eq!(gm.group_of(fake_hwnd(300)), Some(gid));
        let group = gm.groups.get(&gid).unwrap();
        assert_eq!(group.tabs.len(), 3);
    }

    #[test]
    fn remove_dissolves_two_tab_group_via_manager() {
        let mut gm = GroupManager::new();
        let gid = gm.create_group(fake_hwnd(100), fake_hwnd(200));
        gm.remove_from_group(fake_hwnd(100));
        assert_eq!(gm.group_of(fake_hwnd(100)), None);
        assert_eq!(gm.group_of(fake_hwnd(200)), None);
        assert!(!gm.groups.contains_key(&gid));
    }

    #[test]
    fn remove_from_three_tab_keeps_group() {
        let mut gm = GroupManager::new();
        let gid = gm.create_group(fake_hwnd(100), fake_hwnd(200));
        gm.add_to_group(gid, fake_hwnd(300));
        gm.remove_from_group(fake_hwnd(200));
        assert_eq!(gm.group_of(fake_hwnd(200)), None);
        assert_eq!(gm.group_of(fake_hwnd(100)), Some(gid));
        assert_eq!(gm.group_of(fake_hwnd(300)), Some(gid));
        assert!(gm.groups.contains_key(&gid));
    }
}
