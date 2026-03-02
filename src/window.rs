use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH, RECT, TRUE};
use windows_sys::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Metadata for a tracked window.
#[derive(Clone)]
pub struct WindowInfo {
    pub hwnd: HWND,
    pub title: String,
    pub process_name: String,
    pub class_name: String,
    pub icon: HICON,
    pub rect: RECT,
    /// Lazily populated when rules reference the command_line field.
    pub command_line: Option<String>,
}

impl WindowInfo {
    pub fn from_hwnd(hwnd: HWND) -> Option<Self> {
        if !is_eligible(hwnd) {
            return None;
        }
        Some(WindowInfo {
            hwnd,
            title: get_window_title(hwnd),
            process_name: get_process_name(hwnd),
            class_name: get_class_name(hwnd),
            icon: get_window_icon(hwnd),
            rect: get_window_rect(hwnd),
            command_line: None,
        })
    }

    pub fn refresh_title(&mut self) {
        self.title = get_window_title(self.hwnd);
    }

    pub fn refresh_rect(&mut self) {
        self.rect = get_window_rect(self.hwnd);
    }
}

/// Check whether a window should be managed by WinTab.
pub fn is_eligible(hwnd: HWND) -> bool {
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return false;
        }

        // Skip our own process
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == GetCurrentProcessId() {
            return false;
        }

        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

        // Must have a caption (title bar)
        if style & WS_CAPTION != WS_CAPTION {
            return false;
        }

        // Skip child windows
        if style & WS_CHILD != 0 {
            return false;
        }

        // WS_EX_TOOLWINDOW without WS_EX_APPWINDOW → skip
        if ex_style & WS_EX_TOOLWINDOW != 0 && ex_style & WS_EX_APPWINDOW == 0 {
            return false;
        }

        // Skip windows with an owner (owned popups, dialogs)
        let owner = GetWindow(hwnd, GW_OWNER);
        if !owner.is_null() {
            return false;
        }

        // Skip cloaked windows (virtual desktop, UWP hidden)
        let mut cloaked: u32 = 0;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED as u32,
            &mut cloaked as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return false;
        }

        // Skip very small windows
        let rect = get_window_rect(hwnd);
        let w = rect.right - rect.left;
        let h = rect.bottom - rect.top;
        if w < 100 || h < 50 {
            return false;
        }

        true
    }
}

/// Enumerate all eligible top-level windows.
pub fn enumerate_windows() -> Vec<WindowInfo> {
    let mut results: Vec<WindowInfo> = Vec::new();

    unsafe {
        EnumWindows(
            Some(enum_callback),
            &mut results as *mut Vec<WindowInfo> as LPARAM,
        );
    }

    results
}

unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> i32 {
    let results = &mut *(lparam as *mut Vec<WindowInfo>);
    if let Some(info) = WindowInfo::from_hwnd(hwnd) {
        results.push(info);
    }
    TRUE
}

pub fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        OsString::from_wide(&buf[..copied as usize])
            .to_string_lossy()
            .into_owned()
    }
}

pub fn get_window_icon(hwnd: HWND) -> HICON {
    unsafe {
        // Try WM_GETICON (small icon)
        let icon = SendMessageW(hwnd, WM_GETICON, ICON_SMALL as usize, 0);
        if icon != 0 {
            return icon as HICON;
        }

        // Try WM_GETICON (big icon)
        let icon = SendMessageW(hwnd, WM_GETICON, ICON_BIG as usize, 0);
        if icon != 0 {
            return icon as HICON;
        }

        // Try class icon
        let icon = GetClassLongPtrW(hwnd, GCLP_HICONSM);
        if icon != 0 {
            return icon as HICON;
        }

        let icon = GetClassLongPtrW(hwnd, GCLP_HICON);
        if icon != 0 {
            return icon as HICON;
        }

        // Default application icon
        LoadIconW(0 as _, IDI_APPLICATION)
    }
}

pub fn get_window_rect(hwnd: HWND) -> RECT {
    unsafe {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // Prefer DWM extended frame bounds (excludes invisible resize borders)
        let hr = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        );
        if hr != 0 {
            GetWindowRect(hwnd, &mut rect);
        }
        rect
    }
}

/// Check if a window is currently minimized.
pub fn is_minimized(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd) != 0 }
}

/// Check if a window still exists and is valid.
pub fn is_window_valid(hwnd: HWND) -> bool {
    unsafe { IsWindow(hwnd) != 0 }
}

/// Get the window class name.
pub fn get_class_name(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 256];
        let len = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len == 0 {
            return String::new();
        }
        OsString::from_wide(&buf[..len as usize])
            .to_string_lossy()
            .into_owned()
    }
}

/// Get the process executable name (filename only, e.g. "Code.exe").
pub fn get_process_name(hwnd: HWND) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return String::new();
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return String::new();
        }
        let mut buf = [0u16; MAX_PATH as usize];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ok == 0 || size == 0 {
            return String::new();
        }
        let full = OsString::from_wide(&buf[..size as usize])
            .to_string_lossy()
            .into_owned();
        // Extract just the filename
        full.rsplit('\\').next().unwrap_or(&full).to_string()
    }
}

/// Get the DPI for a window.
pub fn get_window_dpi(hwnd: HWND) -> u32 {
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        if dpi == 0 {
            96
        } else {
            dpi
        }
    }
}

/// Get the command line of the process owning the window.
///
/// Uses `NtQueryInformationProcess` to read the remote process's PEB and
/// extract the command line from `RTL_USER_PROCESS_PARAMETERS`. Returns an
/// empty string on any failure (access denied, 32/64-bit mismatch, etc.).
pub fn get_command_line(hwnd: HWND) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return String::new();
        }

        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return String::new();
        }

        let result = read_remote_command_line(handle);
        CloseHandle(handle);
        result
    }
}

/// Read the command line from a remote process handle via PEB.
///
/// # Safety
/// `handle` must be a valid process handle with PROCESS_QUERY_LIMITED_INFORMATION
/// and PROCESS_VM_READ access.
unsafe fn read_remote_command_line(handle: *mut std::ffi::c_void) -> String {
    // Dynamically resolve NtQueryInformationProcess from ntdll.dll
    let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr() as *const u8);
    if ntdll.is_null() {
        return String::new();
    }
    let proc_addr = GetProcAddress(ntdll, c"NtQueryInformationProcess".as_ptr() as *const u8);
    let nt_query: NtQueryInformationProcessFn = match proc_addr {
        Some(f) => std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            NtQueryInformationProcessFn,
        >(f),
        None => return String::new(),
    };

    // ProcessBasicInformation (class 0) gives us the PEB address
    #[repr(C)]
    struct ProcessBasicInformation {
        _reserved1: usize,
        peb_base_address: usize,
        _reserved2: [usize; 4],
    }

    let mut pbi: ProcessBasicInformation = std::mem::zeroed();
    let mut ret_len: u32 = 0;
    let status = nt_query(
        handle,
        0, // ProcessBasicInformation
        &mut pbi as *mut _ as *mut std::ffi::c_void,
        std::mem::size_of::<ProcessBasicInformation>() as u32,
        &mut ret_len,
    );
    if status < 0 || pbi.peb_base_address == 0 {
        return String::new();
    }

    // Read ProcessParameters pointer from PEB.
    // On 64-bit: offset 0x20 is RTL_USER_PROCESS_PARAMETERS*
    // (PEB layout: 2 bytes + 1 byte + 1 byte + 4 bytes padding + 2 pointers = 0x20)
    let params_ptr_offset = 0x20usize;
    let mut process_params_addr: usize = 0;
    let ok = ReadProcessMemory(
        handle,
        (pbi.peb_base_address + params_ptr_offset) as *const std::ffi::c_void,
        &mut process_params_addr as *mut _ as *mut std::ffi::c_void,
        std::mem::size_of::<usize>(),
        std::ptr::null_mut(),
    );
    if ok == 0 || process_params_addr == 0 {
        return String::new();
    }

    // Read CommandLine UNICODE_STRING from RTL_USER_PROCESS_PARAMETERS.
    // CommandLine is at offset 0x70 on 64-bit.
    // UNICODE_STRING: { u16 Length, u16 MaximumLength, padding, usize Buffer }
    let cmd_line_offset = 0x70usize;

    #[repr(C)]
    struct UnicodeString {
        length: u16, // in bytes
        max_length: u16,
        _padding: u32,
        buffer: usize,
    }

    let mut us: UnicodeString = std::mem::zeroed();
    let ok = ReadProcessMemory(
        handle,
        (process_params_addr + cmd_line_offset) as *const std::ffi::c_void,
        &mut us as *mut _ as *mut std::ffi::c_void,
        std::mem::size_of::<UnicodeString>(),
        std::ptr::null_mut(),
    );
    if ok == 0 || us.length == 0 || us.buffer == 0 {
        return String::new();
    }

    // Read the actual command line string
    let char_count = us.length as usize / 2;
    if char_count > 32768 {
        return String::new(); // sanity limit
    }
    let mut buf = vec![0u16; char_count];
    let ok = ReadProcessMemory(
        handle,
        us.buffer as *const std::ffi::c_void,
        buf.as_mut_ptr() as *mut std::ffi::c_void,
        us.length as usize,
        std::ptr::null_mut(),
    );
    if ok == 0 {
        return String::new();
    }

    OsString::from_wide(&buf).to_string_lossy().into_owned()
}

type NtQueryInformationProcessFn = unsafe extern "system" fn(
    process_handle: *mut std::ffi::c_void,
    process_information_class: u32,
    process_information: *mut std::ffi::c_void,
    process_information_length: u32,
    return_length: *mut u32,
) -> i32;
