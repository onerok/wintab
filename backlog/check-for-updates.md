# Check for Updates

## Problem

Users have no way to know when a new version of WinTab is available. Without an update mechanism, users must manually check for releases. A periodic update check that notifies the user via a tray balloon or dialog ensures users stay current with bug fixes and features.

## Location

**Files modified:**

- `src/tray.rs` — Show tray balloon notification when an update is available.
- `src/config.rs` — Add `check_for_updates: bool` (default: `true`) and `update_check_interval_hours: u32` (default: `24`).
- `src/main.rs` — Trigger update check on startup (delayed) and periodically via timer.

**New files:**

- `src/update.rs` — Version comparison logic, HTTP fetch for latest version, notification display.

## Requirements

- [ ] On startup (after a 30-second delay), check a remote URL for the latest version.
- [ ] Periodic re-check at a configurable interval (default: every 24 hours).
- [ ] Compare the remote version with `env!("CARGO_PKG_VERSION")`.
- [ ] If a newer version is available, show a tray balloon notification with a link to download.
- [ ] The check can be disabled via config (`check_for_updates: false`).
- [ ] Update checks do not block the main thread — use async HTTP or a background thread.
- [ ] No auto-download or auto-install — only notification.

## Suggested Implementation

### Version source

Host a simple JSON file at a known URL (e.g., GitHub releases API or a raw GitHub file):

```json
{
  "latest_version": "0.3.0",
  "download_url": "https://github.com/onerok/wintab/releases/latest",
  "changelog": "Added keyboard shortcuts, tab context menu"
}
```

### HTTP fetch with WinHTTP

Use the Windows `WinHTTP` API (already available via `windows-sys`) for minimal dependencies:

```rust
use windows_sys::Win32::Networking::WinHttp::*;

fn fetch_latest_version() -> Option<String> {
    unsafe {
        let session = WinHttpOpen(
            user_agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        );
        if session.is_null() { return None; }

        let connect = WinHttpConnect(session, host.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0);
        let request = WinHttpOpenRequest(connect, method.as_ptr(), path.as_ptr(),
            std::ptr::null(), std::ptr::null(), std::ptr::null(), WINHTTP_FLAG_SECURE);

        WinHttpSendRequest(request, ...);
        WinHttpReceiveResponse(request, std::ptr::null_mut());
        // Read response body, parse JSON, return version string
    }
}
```

### Background thread for non-blocking check

```rust
pub fn start_update_check() {
    std::thread::spawn(|| {
        // Delay 30 seconds on first check
        std::thread::sleep(std::time::Duration::from_secs(30));

        if let Some(latest) = fetch_latest_version() {
            let current = env!("CARGO_PKG_VERSION");
            if is_newer(&latest, current) {
                // Post a custom message to the main thread
                // to show the notification
                unsafe {
                    PostMessageW(get_msg_hwnd(), WM_APP + 200, 0, 0);
                }
            }
        }
    });
}
```

### Version comparison

```rust
fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |v: &str| -> (u32, u32, u32) {
        let parts: Vec<u32> = v.split('.').filter_map(|s| s.parse().ok()).collect();
        (
            parts.get(0).copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };
    parse(remote) > parse(current)
}
```

### Tray balloon notification

```rust
fn show_update_notification(version: &str) {
    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = msg_hwnd;
        nid.uID = TRAY_ICON_ID;
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = NIIF_INFO;

        let title = format!("WinTab Update Available\0");
        let info = format!("Version {} is available. Click to download.\0", version);

        // Copy to fixed-size arrays in NOTIFYICONDATAW
        copy_to_wide(&title, &mut nid.szInfoTitle);
        copy_to_wide(&info, &mut nid.szInfo);

        Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}
```

### Periodic timer

In `main.rs`, set a timer for periodic checks:

```rust
const TIMER_UPDATE_CHECK: usize = 300;
let interval_ms = config.update_check_interval_hours * 3600 * 1000;
unsafe { SetTimer(msg_hwnd, TIMER_UPDATE_CHECK, interval_ms, None); }
```

## Edge Cases

- **No internet**: If the HTTP request fails (timeout, DNS failure, etc.), silently skip the check. Do not show an error to the user. Retry at the next interval.

- **Firewall/proxy**: Corporate environments may block outbound HTTP. WinHTTP respects system proxy settings via `WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY`.

- **Rate limiting**: If using GitHub API, respect rate limits (60 requests/hour for unauthenticated). A 24-hour check interval is well within limits.

- **Thread safety**: The background thread communicates with the main thread via `PostMessageW`, which is thread-safe. Do not access `AppState` from the background thread.

- **Version format**: Use strict semver comparison. Handle versions like "0.2.0-beta" by comparing only the numeric parts (ignore pre-release suffixes) or treating pre-release as older.

- **Balloon click**: Handle `NIN_BALLOONUSERCLICK` in the tray message handler to open the download URL in the default browser via `ShellExecuteW`.

- **Disable on metered connections**: Consider checking `NetworkInformation` API to skip update checks on metered connections, or leave this to the user's config.

- **Cargo.toml feature gate**: Add `Win32_Networking_WinHttp` to `windows-sys` features. Alternatively, use `reqwest` as an optional dependency (adds significant binary size).
