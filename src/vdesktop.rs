use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Com::{
    CoCreateInstance, CLSCTX_ALL,
};

// IVirtualDesktopManager COM interface
const CLSID_VIRTUAL_DESKTOP_MANAGER: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0xAA509086,
    data2: 0x5CA9,
    data3: 0x4C25,
    data4: [0x8F, 0x95, 0x58, 0x9D, 0x3C, 0x07, 0xB4, 0x8A],
};

const IID_IVIRTUAL_DESKTOP_MANAGER: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0xA5CD92FF,
    data2: 0x29BE,
    data3: 0x454C,
    data4: [0x8D, 0x04, 0xD8, 0x28, 0x79, 0xFB, 0x3F, 0x1B],
};

/// Raw COM vtable for IVirtualDesktopManager.
/// Layout: IUnknown (QueryInterface, AddRef, Release) + IsWindowOnCurrentVirtualDesktop, GetWindowDesktopId, MoveWindowToDesktop
#[repr(C)]
struct IVirtualDesktopManagerVtbl {
    // IUnknown
    query_interface: usize,
    add_ref: unsafe extern "system" fn(*mut IVirtualDesktopManagerRaw) -> u32,
    release: unsafe extern "system" fn(*mut IVirtualDesktopManagerRaw) -> u32,
    // IVirtualDesktopManager
    is_window_on_current_virtual_desktop:
        unsafe extern "system" fn(*mut IVirtualDesktopManagerRaw, HWND, *mut i32) -> i32,
}

#[repr(C)]
struct IVirtualDesktopManagerRaw {
    vtbl: *const IVirtualDesktopManagerVtbl,
}

pub struct VDesktopManager {
    ptr: *mut IVirtualDesktopManagerRaw,
    #[cfg(test)]
    mock_off_desktop: std::collections::HashSet<isize>,
}

// SAFETY: VDesktopManager is only used from the single UI thread.
unsafe impl Send for VDesktopManager {}

impl VDesktopManager {
    pub fn new() -> Option<Self> {
        unsafe {
            let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            let hr = CoCreateInstance(
                &CLSID_VIRTUAL_DESKTOP_MANAGER,
                std::ptr::null_mut(),
                CLSCTX_ALL,
                &IID_IVIRTUAL_DESKTOP_MANAGER,
                &mut ptr,
            );
            if hr < 0 || ptr.is_null() {
                return None;
            }
            Some(VDesktopManager {
                ptr: ptr as *mut IVirtualDesktopManagerRaw,
                #[cfg(test)]
                mock_off_desktop: std::collections::HashSet::new(),
            })
        }
    }

    #[cfg(test)]
    pub fn set_off_desktop(&mut self, hwnds: &[HWND]) {
        self.mock_off_desktop.clear();
        for &h in hwnds {
            self.mock_off_desktop.insert(h as isize);
        }
    }

    #[cfg(test)]
    pub fn clear_mock(&mut self) {
        self.mock_off_desktop.clear();
    }

    pub fn is_on_current_desktop(&self, hwnd: HWND) -> bool {
        #[cfg(test)]
        if !self.mock_off_desktop.is_empty() {
            return !self.mock_off_desktop.contains(&(hwnd as isize));
        }
        if self.ptr.is_null() {
            return true; // safe fallback
        }
        unsafe {
            let vtbl = &*(*self.ptr).vtbl;
            let mut on_current: i32 = 0;
            let hr = (vtbl.is_window_on_current_virtual_desktop)(self.ptr, hwnd, &mut on_current);
            if hr < 0 {
                return true; // safe fallback on error
            }
            on_current != 0
        }
    }
}

impl Drop for VDesktopManager {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                let vtbl = &*(*self.ptr).vtbl;
                (vtbl.release)(self.ptr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdesktop_fallback_when_ptr_is_null() {
        let mgr = VDesktopManager {
            ptr: std::ptr::null_mut(),
            mock_off_desktop: std::collections::HashSet::new(),
        };
        assert!(mgr.is_on_current_desktop(1 as HWND));
    }

    #[test]
    fn vdesktop_mock_off_desktop() {
        let mut mgr = VDesktopManager {
            ptr: std::ptr::null_mut(),
            mock_off_desktop: std::collections::HashSet::new(),
        };
        mgr.set_off_desktop(&[1 as HWND, 2 as HWND]);
        assert!(!mgr.is_on_current_desktop(1 as HWND));
        assert!(!mgr.is_on_current_desktop(2 as HWND));
        assert!(mgr.is_on_current_desktop(3 as HWND));
        mgr.clear_mock();
        assert!(mgr.is_on_current_desktop(1 as HWND));
    }
}
