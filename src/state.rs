use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::group::GroupManager;
use crate::overlay::{self, OverlayManager};
use crate::window::{self, WindowInfo};

/// Central application state — single-threaded, accessed via thread-local.
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
            let old_gid = gid;
            self.groups.remove_from_group(hwnd);

            if !self.groups.groups.contains_key(&old_gid) {
                self.overlays.remove_overlay(old_gid);
            } else if let Some(&ov) = self.overlays.overlays.get(&old_gid) {
                overlay::update_overlay(ov, old_gid);
            }
        }
    }

    pub fn on_title_changed(&mut self, hwnd: HWND) {
        if let Some(info) = self.windows.get_mut(&hwnd) {
            info.refresh_title();
        }

        if let Some(gid) = self.groups.group_of(hwnd) {
            if let Some(&ov) = self.overlays.overlays.get(&gid) {
                overlay::update_overlay(ov, gid);
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
            let is_active = self
                .groups
                .groups
                .get(&gid)
                .map(|g| g.active_hwnd() == hwnd)
                .unwrap_or(false);

            if is_active {
                self.suppress_events = true;
                if let Some(group) = self.groups.groups.get(&gid) {
                    group.sync_positions();
                }
                self.suppress_events = false;

                if let Some(&ov) = self.overlays.overlays.get(&gid) {
                    overlay::update_overlay(ov, gid);
                }
            }
        }
    }

    pub fn on_focus_changed(&mut self, hwnd: HWND) {
        if !self.enabled {
            return;
        }

        // If a hidden window in a group gets focus (e.g., taskbar click), switch to it
        if let Some(gid) = self.groups.group_of(hwnd) {
            if let Some(group) = self.groups.groups.get_mut(&gid) {
                if let Some(idx) = group.tabs.iter().position(|&h| h == hwnd) {
                    if idx != group.active {
                        group.switch_to(idx);
                        if let Some(&ov) = self.overlays.overlays.get(&gid) {
                            overlay::update_overlay(ov, gid);
                        }
                    }
                }
            }

            // Bring overlay to top
            if let Some(&ov) = self.overlays.overlays.get(&gid) {
                unsafe {
                    SetWindowPos(
                        ov,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
        }

        self.overlays.update_all();
    }

    pub fn on_minimize(&mut self, hwnd: HWND) {
        if let Some(gid) = self.groups.group_of(hwnd) {
            let is_active = self
                .groups
                .groups
                .get(&gid)
                .map(|g| g.active_hwnd() == hwnd)
                .unwrap_or(false);

            if is_active {
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
                if let Some(&ov) = self.overlays.overlays.get(&gid) {
                    overlay::update_overlay(ov, gid);
                }
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
            self.overlays.update_all();
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
