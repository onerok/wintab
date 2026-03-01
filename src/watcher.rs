use std::path::Path;
use std::thread;
use std::time::Instant;

use windows_sys::Win32::Foundation::{FALSE, HANDLE, HWND, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
    FILE_NOTIFY_CHANGE_LAST_WRITE,
};
use windows_sys::Win32::System::Threading::{WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

/// Custom message posted to the message window when config changes are detected.
pub const WM_WINTAB_CONFIG_RELOAD: u32 = WM_APP + 101;

/// Minimum interval between reload notifications (debounce).
const DEBOUNCE_SECS: u64 = 1;

/// Start a background thread that watches `dir` for file modifications.
///
/// When a change is detected (debounced), it posts `msg_id` to `msg_hwnd`
/// so the main thread can reload the config without any cross-thread state access.
pub fn start_config_watcher(dir: &Path, msg_hwnd: HWND, msg_id: u32) {
    let wide_dir = path_to_wide(dir);

    let handle = unsafe {
        FindFirstChangeNotificationW(wide_dir.as_ptr(), FALSE, FILE_NOTIFY_CHANGE_LAST_WRITE)
    };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        eprintln!("[watcher] FindFirstChangeNotificationW failed for config dir");
        return;
    }

    // HWND and HANDLE are raw pointers (*mut c_void), not Send.
    // Convert to usize for the thread closure. This is safe because:
    // - handle is only used with WaitForSingleObject/FindNextChangeNotification
    //   (thread-safe kernel object operations)
    // - msg_hwnd is only used with PostMessageW (explicitly cross-thread safe)
    let handle_val = handle as usize;
    let msg_hwnd_val = msg_hwnd as usize;

    thread::spawn(move || {
        let handle = handle_val as HANDLE;
        let mut last_notify = Instant::now() - std::time::Duration::from_secs(DEBOUNCE_SECS + 1);

        loop {
            let wait_result = unsafe { WaitForSingleObject(handle, INFINITE) };
            // WAIT_OBJECT_0 == 0
            if wait_result != 0 {
                // Wait failed — clean up and exit thread
                unsafe {
                    FindCloseChangeNotification(handle);
                }
                break;
            }

            let now = Instant::now();
            if now.duration_since(last_notify).as_secs() >= DEBOUNCE_SECS {
                last_notify = now;
                unsafe {
                    PostMessageW(msg_hwnd_val as HWND, msg_id, 0, 0);
                }
            }

            if unsafe { FindNextChangeNotification(handle) } == 0 {
                // FindNextChangeNotification failed — clean up and exit
                unsafe {
                    FindCloseChangeNotification(handle);
                }
                break;
            }
        }
    });
}

fn path_to_wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
