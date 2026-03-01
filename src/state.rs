use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetCapture;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::config::{RulesEngine, WindowRuleInfo};
use crate::group::GroupManager;
use crate::overlay::{self, OverlayManager};
use crate::position_store::{self, PositionStore, RectDef};
use crate::preview::PreviewManager;
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
    pub vdesktop: Option<crate::vdesktop::VDesktopManager>,
    pub rules: RulesEngine,
    pub position_store: PositionStore,
    pub preview: PreviewManager,
}

thread_local! {
    static STATE: RefCell<AppState> = RefCell::new(AppState {
        windows: HashMap::new(),
        groups: GroupManager::new(),
        overlays: OverlayManager::new(),
        enabled: true,
        peek: None,
        suppress_events: false,
        vdesktop: None,
        rules: RulesEngine { groups: Vec::new(), preview_config: crate::config::PreviewConfig::default() },
        position_store: PositionStore::empty(),
        preview: PreviewManager::new(),
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

/// Try to access state without panicking, returning a value.
pub fn try_with_state_ret<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut AppState) -> R,
{
    STATE.with(|cell| cell.try_borrow_mut().ok().map(|mut state| f(&mut state)))
}

impl AppState {
    pub fn init(&mut self) {
        self.vdesktop = crate::vdesktop::VDesktopManager::new();

        // Load config and position store
        if let Some(dir) = crate::appdata::config_dir() {
            self.rules = RulesEngine::load(&dir.join("config.yaml"));
            self.position_store = PositionStore::load(&dir.join("positions.yaml"));
        }

        self.preview.configure(&self.rules.preview_config);

        let windows = window::enumerate_windows();
        for info in windows {
            let hwnd = info.hwnd;
            self.windows.insert(hwnd, info);
            self.apply_rules(hwnd);
        }
    }

    pub fn on_desktop_switch(&mut self) {
        if !self.enabled {
            return;
        }

        self.preview.hide();

        let vd = match &self.vdesktop {
            Some(vd) => vd,
            None => return,
        };

        // Collect group IDs and their overlay/active-hwnd to avoid borrow conflicts
        let group_info: Vec<_> = self
            .overlays
            .overlays
            .iter()
            .filter_map(|(&gid, &ov)| {
                let group = self.groups.groups.get(&gid)?;
                Some((gid, ov, group.active_hwnd()))
            })
            .collect();

        for (gid, ov, active_hwnd) in group_info {
            if vd.is_on_current_desktop(active_hwnd) {
                self.overlays.desktop_hidden.remove(&gid);
                overlay::update_overlay(ov, gid, &self.groups, &self.windows);
            } else {
                self.overlays.desktop_hidden.insert(gid);
                unsafe {
                    ShowWindow(ov, SW_HIDE);
                }
            }
        }

        // Hide peek overlay if peek target is off-desktop
        if let Some(ref peek) = self.peek {
            if !vd.is_on_current_desktop(peek.target_hwnd) {
                let peek_ov = peek.overlay_hwnd;
                self.peek = None;
                overlay::destroy_overlay(peek_ov);
            }
        }
    }

    /// Check virtual desktop state for all groups and hide/show overlays
    /// as needed.  Called from on_focus_changed() as a reliable fallback
    /// since EVENT_SYSTEM_DESKTOPSWITCH may not arrive via the hook.
    fn sync_desktop_visibility(&mut self, focused_hwnd: HWND) {
        self.preview.hide();

        let vd = match &self.vdesktop {
            Some(vd) => vd,
            None => return,
        };

        let group_info: Vec<_> = self
            .overlays
            .overlays
            .iter()
            .filter_map(|(&gid, &ov)| {
                let group = self.groups.groups.get(&gid)?;
                Some((gid, ov, group.active_hwnd()))
            })
            .collect();

        for (gid, ov, active_hwnd) in group_info {
            // The foreground window is always on the current desktop.
            // Trust this over COM which can return stale results for
            // windows recently shown from a hidden state (tab switch).
            let on_current = if active_hwnd == focused_hwnd {
                true
            } else {
                vd.is_on_current_desktop(active_hwnd)
            };
            let was_hidden = self.overlays.desktop_hidden.contains(&gid);

            if on_current && was_hidden {
                self.overlays.desktop_hidden.remove(&gid);
                overlay::update_overlay(ov, gid, &self.groups, &self.windows);
            } else if !on_current && !was_hidden {
                self.overlays.desktop_hidden.insert(gid);
                unsafe {
                    ShowWindow(ov, SW_HIDE);
                }
            }
        }

        // Hide peek overlay if peek target is off-desktop
        if let Some(ref peek) = self.peek {
            if !vd.is_on_current_desktop(peek.target_hwnd) {
                let peek_ov = peek.overlay_hwnd;
                self.peek = None;
                overlay::destroy_overlay(peek_ov);
            }
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
            self.try_restore_position(hwnd, &info);
            self.windows.insert(info.hwnd, info);
            self.apply_rules(hwnd);
        }
    }

    pub fn on_window_destroyed(&mut self, hwnd: HWND) {
        self.windows.remove(&hwnd);

        // Clean up pending rule singletons
        self.groups.pending_rules.retain(|_, &mut h| h != hwnd);

        if self.peek.as_ref().map(|p| p.target_hwnd) == Some(hwnd) {
            self.hide_peek();
        }

        if let Some(gid) = self.groups.group_of(hwnd) {
            self.groups.remove_from_group(hwnd);
            self.overlays
                .refresh_overlay(gid, &self.groups, &self.windows);
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
            if !self.overlays.desktop_hidden.contains(&gid) {
                if let Some(&ov) = self.overlays.overlays.get(&gid) {
                    overlay::update_overlay(ov, gid, &self.groups, &self.windows);
                }
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

        // Record position for persistence
        if let Some(info) = self.windows.get(&hwnd) {
            let dpi = window::get_window_dpi(hwnd);
            self.position_store.record(
                &info.process_name,
                &info.class_name,
                &info.title,
                RectDef {
                    left: info.rect.left,
                    top: info.rect.top,
                    right: info.rect.right,
                    bottom: info.rect.bottom,
                },
                dpi,
            );
        }

        if let Some(gid) = self.groups.group_of(hwnd) {
            let is_active = self.groups.is_active_in_group(gid, hwnd);

            if is_active {
                self.suppress_events = true;
                if let Some(group) = self.groups.groups.get(&gid) {
                    group.sync_positions();
                }
                self.suppress_events = false;

                if !self.overlays.desktop_hidden.contains(&gid) {
                    if let Some(&ov) = self.overlays.overlays.get(&gid) {
                        overlay::update_overlay(ov, gid, &self.groups, &self.windows);
                    }
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

        // EVENT_SYSTEM_DESKTOPSWITCH is not reliably delivered via
        // SetWinEventHook(WINEVENT_OUTOFCONTEXT), but foreground events
        // always fire when the user switches virtual desktops.  Re-check
        // desktop visibility here so overlays are hidden/shown promptly.
        self.sync_desktop_visibility(hwnd);

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
                        0,
                        0,
                        0,
                        0,
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
            if !self.overlays.desktop_hidden.contains(&gid) {
                if let Some(&ov) = self.overlays.overlays.get(&gid) {
                    overlay::update_overlay(ov, gid, &self.groups, &self.windows);
                }
            }
        }
    }

    pub fn toggle_enabled(&mut self) {
        self.enabled = !self.enabled;
        if !self.enabled {
            self.preview.hide();
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
        self.position_store.flush();
        self.preview.destroy();
        self.hide_peek();
        self.groups.show_all_windows();
        self.overlays.destroy_all();
    }

    /// Apply rules engine to a window. If it matches a rule group:
    /// - If there's already a pending singleton, create a real group.
    /// - If there's an existing named group, add to it.
    /// - Otherwise, record as pending singleton (no overlay yet).
    pub(crate) fn apply_rules(&mut self, hwnd: HWND) {
        if !self.rules.has_rules() {
            return;
        }
        // Skip if already in a group
        if self.groups.group_of(hwnd).is_some() {
            return;
        }
        let info = match self.windows.get(&hwnd) {
            Some(i) => i,
            None => return,
        };
        let rule_info = WindowRuleInfo {
            process_name: &info.process_name,
            class_name: &info.class_name,
            title: &info.title,
        };
        let group_name = match self.rules.apply(&rule_info) {
            Some(name) => name.to_string(),
            None => return,
        };

        // Check if there's an existing named group
        if let Some(&gid) = self.groups.named_groups.get(&group_name) {
            if self.groups.groups.contains_key(&gid) {
                self.groups.add_to_group(gid, hwnd);
                self.overlays
                    .refresh_overlay(gid, &self.groups, &self.windows);
                return;
            }
        }

        // Check if there's a pending singleton
        if let Some(pending_hwnd) = self.groups.pending_rules.remove(&group_name) {
            if pending_hwnd != hwnd && self.windows.contains_key(&pending_hwnd) {
                let gid = self.groups.create_group(pending_hwnd, hwnd);
                self.groups.named_groups.insert(group_name, gid);
                let ov = self.overlays.ensure_overlay(gid);
                overlay::update_overlay(ov, gid, &self.groups, &self.windows);
                return;
            }
        }

        // Record as pending singleton
        self.groups.pending_rules.insert(group_name, hwnd);
    }

    /// Try to restore a window's saved position from the position store.
    pub(crate) fn try_restore_position(&mut self, hwnd: HWND, info: &WindowInfo) {
        let entry =
            match self
                .position_store
                .lookup(&info.process_name, &info.class_name, &info.title)
            {
                Some(e) => e,
                None => return,
            };

        // Validate that a monitor exists at the saved rect
        if !position_store::monitor_exists_for_rect(&entry.rect) {
            return;
        }

        // Scale by DPI ratio
        let current_dpi = window::get_window_dpi(hwnd);
        let stored_dpi = entry.dpi;
        let (left, top, right, bottom) = if stored_dpi > 0 && stored_dpi != current_dpi {
            let scale = current_dpi as f64 / stored_dpi as f64;
            (
                (entry.rect.left as f64 * scale) as i32,
                (entry.rect.top as f64 * scale) as i32,
                (entry.rect.right as f64 * scale) as i32,
                (entry.rect.bottom as f64 * scale) as i32,
            )
        } else {
            (
                entry.rect.left,
                entry.rect.top,
                entry.rect.right,
                entry.rect.bottom,
            )
        };

        unsafe {
            SetWindowPos(
                hwnd,
                0 as _,
                left,
                top,
                right - left,
                bottom - top,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
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
                    overlay::update_peek_overlay(
                        peek.overlay_hwnd,
                        peek.target_hwnd,
                        &self.windows,
                    );
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

        // Use WindowFromPoint for z-order-aware fallback
        let hit = unsafe {
            WindowFromPoint(POINT {
                x: cursor.x,
                y: cursor.y,
            })
        };
        if hit.is_null() {
            return None;
        }
        let top = unsafe { GetAncestor(hit, GA_ROOT) };
        let top = if top.is_null() { hit } else { top };
        if self.windows.contains_key(&top)
            && self.groups.group_of(top).is_none()
            && !window::is_minimized(top)
            && Self::cursor_in_hot_zone(cursor, top)
        {
            return Some(top);
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
