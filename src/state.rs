use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetCapture;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::group::GroupManager;
use crate::overlay::{self, OverlayManager};
use crate::window::{self, WindowInfo};

pub struct PeekState {
    pub target_hwnd: HWND,
    pub overlay_hwnd: HWND,
    pub leave_ticks: u32,
}

pub struct AppState {
    pub windows: HashMap<HWND, WindowInfo>,
    pub groups: GroupManager,
    pub overlays: OverlayManager,
    pub enabled: bool,
    pub peek: Option<PeekState>,
    suppress_events: bool,
}

thread_local! {
    static STATE: RefCell<AppState> = RefCell::new(AppState {
        windows: HashMap::new(),
        groups: GroupManager::new(),
        overlays: OverlayManager::new(),
        enabled: true,
        peek: None,
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

        if self.peek.as_ref().map(|p| p.target_hwnd) == Some(hwnd) {
            self.hide_peek();
        }

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

        if let Some(ref peek) = self.peek {
            if peek.target_hwnd == hwnd {
                overlay::update_peek_overlay(peek.overlay_hwnd, peek.target_hwnd, &self.windows);
            }
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

        if let Some(ref peek) = self.peek {
            if peek.target_hwnd == hwnd {
                overlay::update_peek_overlay(peek.overlay_hwnd, peek.target_hwnd, &self.windows);
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
        if self.peek.as_ref().map(|p| p.target_hwnd) == Some(hwnd) {
            self.hide_peek();
        }

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
            self.hide_peek();
            for &ov in self.overlays.overlays.values() {
                unsafe {
                    ShowWindow(ov, SW_HIDE);
                }
            }
        } else {
            self.overlays.update_all(&self.groups, &self.windows);
        }
    }

    pub fn shutdown(&mut self) {
        self.hide_peek();
        self.groups.show_all_windows();
        self.overlays.destroy_all();
    }

    pub fn hide_peek(&mut self) {
        if let Some(peek) = self.peek.take() {
            overlay::destroy_overlay(peek.overlay_hwnd);
        }
    }

    fn show_peek(&mut self, hwnd: HWND) {
        let ov = overlay::create_peek_overlay(hwnd);
        if !ov.is_null() {
            overlay::update_peek_overlay(ov, hwnd, &self.windows);
            self.peek = Some(PeekState {
                target_hwnd: hwnd,
                overlay_hwnd: ov,
                leave_ticks: 0,
            });
        }
    }

    pub fn on_peek_timer(&mut self) {
        if !self.enabled {
            self.hide_peek();
            return;
        }

        let cursor = unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            pt
        };

        if let Some(mut peek) = self.peek.take() {
            // Overlay externally destroyed
            if !window::is_window_valid(peek.overlay_hwnd) {
                return;
            }

            // Target invalid, minimized, or now grouped → hide
            if !window::is_window_valid(peek.target_hwnd)
                || window::is_minimized(peek.target_hwnd)
                || self.groups.group_of(peek.target_hwnd).is_some()
            {
                overlay::destroy_overlay(peek.overlay_hwnd);
                return;
            }

            let has_capture = unsafe { GetCapture() } == peek.overlay_hwnd;
            let in_hot = Self::cursor_in_hot_zone(&cursor, peek.target_hwnd);
            let in_overlay = Self::cursor_in_peek_overlay(&cursor, peek.overlay_hwnd);

            if in_hot || in_overlay || has_capture {
                peek.leave_ticks = 0;
                if !has_capture {
                    overlay::update_peek_overlay(peek.overlay_hwnd, peek.target_hwnd, &self.windows);
                }
                self.peek = Some(peek);
            } else {
                peek.leave_ticks += 1;
                if peek.leave_ticks >= 5 {
                    overlay::destroy_overlay(peek.overlay_hwnd);
                } else {
                    self.peek = Some(peek);
                }
            }
            return;
        }

        // No peek active — check for candidate
        if let Some(hwnd) = self.find_peek_candidate(&cursor) {
            self.show_peek(hwnd);
        }
    }

    fn cursor_in_hot_zone(cursor: &POINT, hwnd: HWND) -> bool {
        let rect = window::get_window_rect(hwnd);
        cursor.x >= rect.left
            && cursor.x < rect.right
            && cursor.y >= rect.top - overlay::TAB_HEIGHT
            && cursor.y < rect.top + 8
    }

    fn cursor_in_peek_overlay(cursor: &POINT, overlay_hwnd: HWND) -> bool {
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        unsafe {
            GetWindowRect(overlay_hwnd, &mut rect);
        }
        cursor.x >= rect.left
            && cursor.x < rect.right
            && cursor.y >= rect.top
            && cursor.y < rect.bottom
    }

    fn find_peek_candidate(&self, cursor: &POINT) -> Option<HWND> {
        // Check foreground window first for z-order correctness
        let fg = unsafe { GetForegroundWindow() };
        if !fg.is_null()
            && self.windows.contains_key(&fg)
            && self.groups.group_of(fg).is_none()
            && !window::is_minimized(fg)
            && Self::cursor_in_hot_zone(cursor, fg)
        {
            return Some(fg);
        }

        // Fall back to scanning all managed windows
        for &hwnd in self.windows.keys() {
            if hwnd == fg {
                continue;
            }
            if self.groups.group_of(hwnd).is_some() {
                continue;
            }
            if window::is_minimized(hwnd) {
                continue;
            }
            if Self::cursor_in_hot_zone(cursor, hwnd) {
                return Some(hwnd);
            }
        }

        None
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
