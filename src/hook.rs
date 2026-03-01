use std::ptr::null_mut;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Accessibility::{
    SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::state;

use std::cell::RefCell;

const EVENT_SYSTEM_DESKTOPSWITCH: u32 = 0x0020;

thread_local! {
    static HOOKS: RefCell<Vec<HWINEVENTHOOK>> = const { RefCell::new(Vec::new()) };
}

pub fn install() {
    let events: &[(u32, u32)] = &[
        (EVENT_OBJECT_CREATE, EVENT_OBJECT_CREATE),
        (EVENT_OBJECT_DESTROY, EVENT_OBJECT_DESTROY),
        (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
        (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
        (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
        (EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW),
        (EVENT_OBJECT_HIDE, EVENT_OBJECT_HIDE),
        (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZESTART),
        (EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZEEND),
        (EVENT_SYSTEM_DESKTOPSWITCH, EVENT_SYSTEM_DESKTOPSWITCH),
    ];

    unsafe {
        HOOKS.with(|hooks| {
            let mut hooks = hooks.borrow_mut();
            for &(min, max) in events {
                let hook = SetWinEventHook(
                    min,
                    max,
                    null_mut(),
                    Some(win_event_proc),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                );
                if !hook.is_null() {
                    hooks.push(hook);
                }
            }
        });
    }
}

pub fn uninstall() {
    unsafe {
        HOOKS.with(|hooks| {
            for hook in hooks.borrow_mut().drain(..) {
                UnhookWinEvent(hook);
            }
        });
    }
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if id_object != 0 {
        return;
    }

    if event == EVENT_SYSTEM_DESKTOPSWITCH {
        state::with_state(|s| s.on_desktop_switch());
        return;
    }

    if hwnd.is_null() {
        return;
    }

    state::with_state(|s| {
        if event == EVENT_OBJECT_CREATE || event == EVENT_OBJECT_SHOW {
            s.on_window_created(hwnd);
        } else if event == EVENT_OBJECT_DESTROY {
            s.on_window_destroyed(hwnd);
        } else if event == EVENT_OBJECT_NAMECHANGE {
            s.on_title_changed(hwnd);
        } else if event == EVENT_OBJECT_LOCATIONCHANGE {
            s.on_window_moved(hwnd);
        } else if event == EVENT_SYSTEM_FOREGROUND {
            s.on_focus_changed(hwnd);
        } else if event == EVENT_SYSTEM_MINIMIZESTART {
            s.on_minimize(hwnd);
        } else if event == EVENT_SYSTEM_MINIMIZEEND {
            s.on_restore(hwnd);
        }
    });
}
