use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::group::GroupManager;
use crate::overlay::{self, OverlayManager};
use crate::window::{self, WindowInfo};

pub struct AppState {
    pub windows: HashMap<HWND, WindowInfo>,
    pub groups: GroupManager,
    pub overlays: OverlayManager,
    pub enabled: bool,
    suppress_events: bool,
}

thread_local! {
    static STATE: RefCell<AppState> = RefCell::new(AppState {
        windows: HashMap::new(),
        groups: GroupManager::new(),
        overlays: OverlayManager::new(),
        enabled: true,
        suppress_events: false,
    });
}

pub fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut AppState) -> R,
{
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        f(&mut state)
    })
}

/// Try to access state without panicking (for panic hook).
pub fn try_with_state<F>(f: F)
where
    F: FnOnce(&mut AppState),
{
    STATE.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            f(&mut state);
        }
    });
}

impl AppState {
    pub fn init(&mut self) {
        let windows = window::enumerate_windows();
        for info in windows {
            self.windows.insert(info.hwnd, info);
        }
    }

    pub fn on_window_created(&mut self, hwnd: HWND) {
        if self.suppress_events || !self.enabled {
            return;
        }
        if self.windows.contains_key(&hwnd) {
            return;
        }
        if let Some(info) = WindowInfo::from_hwnd(hwnd) {
            self.windows.insert(info.hwnd, info);
        }
    }

    pub fn on_window_destroyed(&mut self, hwnd: HWND) {
        self.windows.remove(&hwnd);

        if let Some(gid) = self.groups.group_of(hwnd) {
            self.groups.remove_from_group(hwnd);
            self.overlays.refresh_overlay(gid, &self.groups, &self.windows);
        }
    }

    pub fn on_title_changed(&mut self, hwnd: HWND) {
        let old_title = self.windows.get(&hwnd).map(|i| i.title.clone());
        if let Some(info) = self.windows.get_mut(&hwnd) {
            info.refresh_title();
        }

        // Only repaint if title actually changed
        let new_title = self.windows.get(&hwnd).map(|i| &i.title);
        if old_title.as_deref() == new_title.map(|s| s.as_str()) {
            return;
        }

        if let Some(gid) = self.groups.group_of(hwnd) {
            if let Some(&ov) = self.overlays.overlays.get(&gid) {
                overlay::update_overlay(ov, gid, &self.groups, &self.windows);
            }
        }
    }

    pub fn on_window_moved(&mut self, hwnd: HWND) {
        if self.suppress_events {
            return;
        }

        if let Some(info) = self.windows.get_mut(&hwnd) {
            info.refresh_rect();
        }

        if let Some(gid) = self.groups.group_of(hwnd) {
            let is_active = self.groups.is_active_in_group(gid, hwnd);

            if is_active {
                self.suppress_events = true;
                if let Some(group) = self.groups.groups.get(&gid) {
                    group.sync_positions();
                }
                self.suppress_events = false;

                if let Some(&ov) = self.overlays.overlays.get(&gid) {
                    overlay::update_overlay(ov, gid, &self.groups, &self.windows);
                }
            }
        }
    }

    pub fn on_focus_changed(&mut self, hwnd: HWND) {
        if !self.enabled {
            return;
        }

        if let Some(gid) = self.groups.group_of(hwnd) {
            if let Some(group) = self.groups.groups.get_mut(&gid) {
                if let Some(idx) = group.tabs.iter().position(|&h| h == hwnd) {
                    if idx != group.active {
                        group.switch_to(idx);
                    }
                }
            }

            // Bring overlay to top
            if let Some(&ov) = self.overlays.overlays.get(&gid) {
                unsafe {
                    SetWindowPos(
                        ov,
                        HWND_TOPMOST,
                        0, 0, 0, 0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
        }

        self.overlays.update_all(&self.groups, &self.windows);
    }

    pub fn on_minimize(&mut self, hwnd: HWND) {
        if let Some(gid) = self.groups.group_of(hwnd) {
            if self.groups.is_active_in_group(gid, hwnd) {
                self.suppress_events = true;
                if let Some(group) = self.groups.groups.get(&gid) {
                    group.minimize_all();
                }
                self.suppress_events = false;

                if let Some(&ov) = self.overlays.overlays.get(&gid) {
                    unsafe {
                        ShowWindow(ov, SW_HIDE);
                    }
                }
            }
        }
    }

    pub fn on_restore(&mut self, hwnd: HWND) {
        if let Some(gid) = self.groups.group_of(hwnd) {
            if let Some(group) = self.groups.groups.get_mut(&gid) {
                group.restore();
            }
            if let Some(&ov) = self.overlays.overlays.get(&gid) {
                overlay::update_overlay(ov, gid, &self.groups, &self.windows);
            }
        }
    }

    pub fn toggle_enabled(&mut self) {
        self.enabled = !self.enabled;
        if !self.enabled {
            for (_, &ov) in &self.overlays.overlays {
                unsafe {
                    ShowWindow(ov, SW_HIDE);
                }
            }
        } else {
            self.overlays.update_all(&self.groups, &self.windows);
        }
    }

    pub fn shutdown(&mut self) {
        self.groups.show_all_windows();
        self.overlays.destroy_all();
    }

    pub fn find_managed_window_at(&self, pt: POINT) -> Option<HWND> {
        unsafe {
            let hwnd = WindowFromPoint(pt);
            if hwnd.is_null() {
                return None;
            }

            if self.windows.contains_key(&hwnd) {
                return Some(hwnd);
            }

            let parent = GetAncestor(hwnd, GA_ROOT);
            if !parent.is_null() && self.windows.contains_key(&parent) {
                return Some(parent);
            }

            None
        }
    }
}
