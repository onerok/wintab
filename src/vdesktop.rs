use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

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
    get_window_desktop_id: unsafe extern "system" fn(
        *mut IVirtualDesktopManagerRaw,
        HWND,
        *mut windows_sys::core::GUID,
    ) -> i32,
    move_window_to_desktop: unsafe extern "system" fn(
        *mut IVirtualDesktopManagerRaw,
        HWND,
        *const windows_sys::core::GUID,
    ) -> i32,
}

#[repr(C)]
struct IVirtualDesktopManagerRaw {
    vtbl: *const IVirtualDesktopManagerVtbl,
}

pub struct VDesktopManager {
    ptr: *mut IVirtualDesktopManagerRaw,
    #[cfg(test)]
    mock_off_desktop: std::collections::HashSet<isize>,
    #[cfg(test)]
    mock_desktop_id: Option<[u8; 16]>,
    #[cfg(test)]
    mock_per_window_desktop: std::collections::HashMap<isize, [u8; 16]>,
    #[cfg(test)]
    mock_moves: Vec<(isize, [u8; 16])>,
    #[cfg(test)]
    mock_desktop_order: Option<Vec<[u8; 16]>>,
}

// SAFETY: VDesktopManager is only used from the single UI thread.
unsafe impl Send for VDesktopManager {}

/// Convert a COM GUID to a 16-byte array (little-endian field layout).
pub fn guid_to_bytes(guid: &windows_sys::core::GUID) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&guid.data1.to_le_bytes());
    bytes[4..6].copy_from_slice(&guid.data2.to_le_bytes());
    bytes[6..8].copy_from_slice(&guid.data3.to_le_bytes());
    bytes[8..16].copy_from_slice(&guid.data4);
    bytes
}

/// Convert a 16-byte array back to a COM GUID (little-endian field layout).
pub fn bytes_to_guid(bytes: &[u8; 16]) -> windows_sys::core::GUID {
    windows_sys::core::GUID {
        data1: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        data2: u16::from_le_bytes([bytes[4], bytes[5]]),
        data3: u16::from_le_bytes([bytes[6], bytes[7]]),
        data4: [
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ],
    }
}

/// Find the index of a desktop ID in the ordered list.
pub fn desktop_index_of(id: &[u8; 16], order: &[[u8; 16]]) -> Option<usize> {
    order.iter().position(|d| d == id)
}

/// Read the virtual desktop order from the registry.
/// Returns desktop IDs in left-to-right order.
/// `VirtualDesktopIDs` is a REG_BINARY value containing concatenated 16-byte GUIDs.
fn read_desktop_order_from_registry() -> Option<Vec<[u8; 16]>> {
    use windows_sys::Win32::System::Registry::*;

    let subkey: Vec<u16> =
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops\0"
            .encode_utf16()
            .collect();
    let value_name: Vec<u16> = "VirtualDesktopIDs\0".encode_utf16().collect();

    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        let res = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        );
        if res != 0 || hkey.is_null() {
            return None;
        }

        // First query to get size
        let mut data_type: u32 = 0;
        let mut data_size: u32 = 0;
        let res = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null(),
            &mut data_type,
            std::ptr::null_mut(),
            &mut data_size,
        );
        if res != 0 || data_type != REG_BINARY || data_size == 0 {
            RegCloseKey(hkey);
            return None;
        }

        let mut buf = vec![0u8; data_size as usize];
        let res = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null(),
            &mut data_type,
            buf.as_mut_ptr(),
            &mut data_size,
        );
        RegCloseKey(hkey);

        if res != 0 {
            return None;
        }

        Some(parse_desktop_order_blob(&buf[..data_size as usize]))
    }
}

/// Parse a raw registry blob of concatenated 16-byte GUIDs into a Vec.
pub fn parse_desktop_order_blob(blob: &[u8]) -> Vec<[u8; 16]> {
    blob.chunks_exact(16)
        .map(|chunk| {
            let mut id = [0u8; 16];
            id.copy_from_slice(chunk);
            id
        })
        .collect()
}

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
                #[cfg(test)]
                mock_desktop_id: None,
                #[cfg(test)]
                mock_per_window_desktop: std::collections::HashMap::new(),
                #[cfg(test)]
                mock_moves: Vec::new(),
                #[cfg(test)]
                mock_desktop_order: None,
            })
        }
    }

    /// Create a VDesktopManager with a null COM pointer for testing.
    /// Uses mock fields exclusively — no real COM calls.
    #[cfg(test)]
    pub fn new_null_for_test() -> Self {
        VDesktopManager {
            ptr: std::ptr::null_mut(),
            mock_off_desktop: std::collections::HashSet::new(),
            mock_desktop_id: None,
            mock_per_window_desktop: std::collections::HashMap::new(),
            mock_moves: Vec::new(),
            mock_desktop_order: None,
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

    #[cfg(test)]
    pub fn set_mock_desktop_id(&mut self, id: Option<[u8; 16]>) {
        self.mock_desktop_id = id;
    }

    #[cfg(test)]
    pub fn set_mock_window_desktop(&mut self, hwnd: HWND, desktop_id: [u8; 16]) {
        self.mock_per_window_desktop
            .insert(hwnd as isize, desktop_id);
    }

    #[cfg(test)]
    pub fn set_mock_desktop_order(&mut self, order: Vec<[u8; 16]>) {
        self.mock_desktop_order = Some(order);
    }

    #[cfg(test)]
    pub fn take_mock_moves(&mut self) -> Vec<(isize, [u8; 16])> {
        std::mem::take(&mut self.mock_moves)
    }

    /// Get the virtual desktop GUID for the given window.
    /// Returns None on failure or if COM is unavailable.
    pub fn get_desktop_id(&self, hwnd: HWND) -> Option<[u8; 16]> {
        #[cfg(test)]
        {
            if let Some(id) = self.mock_per_window_desktop.get(&(hwnd as isize)) {
                return Some(*id);
            }
            if self.mock_desktop_id.is_some() {
                return self.mock_desktop_id;
            }
        }
        if self.ptr.is_null() {
            return None;
        }
        unsafe {
            let vtbl = &*(*self.ptr).vtbl;
            let mut guid: windows_sys::core::GUID = std::mem::zeroed();
            let hr = (vtbl.get_window_desktop_id)(self.ptr, hwnd, &mut guid);
            if hr < 0 {
                return None;
            }
            Some(guid_to_bytes(&guid))
        }
    }

    /// Move a window to the virtual desktop identified by the given GUID bytes.
    /// Returns true on success.
    pub fn move_to_desktop(&mut self, hwnd: HWND, desktop_id: &[u8; 16]) -> bool {
        #[cfg(test)]
        {
            self.mock_moves.push((hwnd as isize, *desktop_id));
            self.mock_per_window_desktop
                .insert(hwnd as isize, *desktop_id);
            return true;
        }
        #[allow(unreachable_code)]
        {
            if self.ptr.is_null() {
                return false;
            }
            unsafe {
                let vtbl = &*(*self.ptr).vtbl;
                let guid = bytes_to_guid(desktop_id);
                let hr = (vtbl.move_window_to_desktop)(self.ptr, hwnd, &guid);
                hr >= 0
            }
        }
    }

    /// Get the ordered list of virtual desktop IDs (left-to-right).
    pub fn get_desktop_order(&self) -> Option<Vec<[u8; 16]>> {
        #[cfg(test)]
        if self.mock_desktop_order.is_some() {
            return self.mock_desktop_order.clone();
        }
        read_desktop_order_from_registry()
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

    fn null_manager() -> VDesktopManager {
        VDesktopManager::new_null_for_test()
    }

    #[test]
    fn vdesktop_fallback_when_ptr_is_null() {
        let mgr = null_manager();
        assert!(mgr.is_on_current_desktop(1 as HWND));
    }

    #[test]
    fn vdesktop_mock_off_desktop() {
        let mut mgr = null_manager();
        mgr.set_off_desktop(&[1 as HWND, 2 as HWND]);
        assert!(!mgr.is_on_current_desktop(1 as HWND));
        assert!(!mgr.is_on_current_desktop(2 as HWND));
        assert!(mgr.is_on_current_desktop(3 as HWND));
        mgr.clear_mock();
        assert!(mgr.is_on_current_desktop(1 as HWND));
    }

    #[test]
    fn guid_bytes_roundtrip() {
        let guid = windows_sys::core::GUID {
            data1: 0xAA509086,
            data2: 0x5CA9,
            data3: 0x4C25,
            data4: [0x8F, 0x95, 0x58, 0x9D, 0x3C, 0x07, 0xB4, 0x8A],
        };
        let bytes = guid_to_bytes(&guid);
        let restored = bytes_to_guid(&bytes);
        assert_eq!(guid.data1, restored.data1);
        assert_eq!(guid.data2, restored.data2);
        assert_eq!(guid.data3, restored.data3);
        assert_eq!(guid.data4, restored.data4);
    }

    #[test]
    fn guid_bytes_zeroed() {
        let guid = windows_sys::core::GUID {
            data1: 0,
            data2: 0,
            data3: 0,
            data4: [0; 8],
        };
        let bytes = guid_to_bytes(&guid);
        assert_eq!(bytes, [0u8; 16]);
        let restored = bytes_to_guid(&bytes);
        assert_eq!(restored.data1, 0);
        assert_eq!(restored.data2, 0);
        assert_eq!(restored.data3, 0);
        assert_eq!(restored.data4, [0; 8]);
    }

    #[test]
    fn get_desktop_id_returns_none_when_ptr_null_no_mock() {
        let mgr = null_manager();
        assert!(mgr.get_desktop_id(1 as HWND).is_none());
    }

    #[test]
    fn get_desktop_id_returns_mock_when_set() {
        let mut mgr = null_manager();
        let id = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        mgr.set_mock_desktop_id(Some(id));
        assert_eq!(mgr.get_desktop_id(1 as HWND), Some(id));
    }

    #[test]
    fn move_to_desktop_records_mock_move() {
        let mut mgr = null_manager();
        let id = [0xAA; 16];
        assert!(mgr.move_to_desktop(1 as HWND, &id));
        let moves = mgr.take_mock_moves();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0], (1, [0xAA; 16]));
        // Per-window desktop should be updated
        assert_eq!(mgr.get_desktop_id(1 as HWND), Some([0xAA; 16]));
    }

    #[test]
    fn per_window_desktop_id_overrides_global_mock() {
        let mut mgr = null_manager();
        let global_id = [0x11; 16];
        let per_window_id = [0x22; 16];
        mgr.set_mock_desktop_id(Some(global_id));
        mgr.set_mock_window_desktop(1 as HWND, per_window_id);
        // Per-window should take priority
        assert_eq!(mgr.get_desktop_id(1 as HWND), Some(per_window_id));
        // Other windows fall back to global
        assert_eq!(mgr.get_desktop_id(2 as HWND), Some(global_id));
    }

    #[test]
    fn mock_desktop_order() {
        let mut mgr = null_manager();
        let d1 = [0x01; 16];
        let d2 = [0x02; 16];
        mgr.set_mock_desktop_order(vec![d1, d2]);
        let order = mgr.get_desktop_order().unwrap();
        assert_eq!(order, vec![d1, d2]);
    }

    #[test]
    fn desktop_index_of_found() {
        let d1 = [0x01; 16];
        let d2 = [0x02; 16];
        let d3 = [0x03; 16];
        let order = vec![d1, d2, d3];
        assert_eq!(desktop_index_of(&d1, &order), Some(0));
        assert_eq!(desktop_index_of(&d2, &order), Some(1));
        assert_eq!(desktop_index_of(&d3, &order), Some(2));
    }

    #[test]
    fn desktop_index_of_not_found() {
        let d1 = [0x01; 16];
        let d2 = [0x02; 16];
        let unknown = [0xFF; 16];
        let order = vec![d1, d2];
        assert_eq!(desktop_index_of(&unknown, &order), None);
    }

    #[test]
    fn desktop_index_of_empty_order() {
        let id = [0x01; 16];
        let order: Vec<[u8; 16]> = Vec::new();
        assert_eq!(desktop_index_of(&id, &order), None);
    }

    #[test]
    fn desktop_index_of_single_desktop() {
        let d1 = [0x01; 16];
        let order = vec![d1];
        assert_eq!(desktop_index_of(&d1, &order), Some(0));
    }

    #[test]
    fn parse_desktop_order_blob_valid() {
        let d1 = [0x01; 16];
        let d2 = [0x02; 16];
        let mut blob = Vec::new();
        blob.extend_from_slice(&d1);
        blob.extend_from_slice(&d2);
        let result = parse_desktop_order_blob(&blob);
        assert_eq!(result, vec![d1, d2]);
    }

    #[test]
    fn parse_desktop_order_blob_empty() {
        let result = parse_desktop_order_blob(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_desktop_order_blob_partial_guid_ignored() {
        // 20 bytes = 1 full GUID (16) + 4 leftover (ignored by chunks_exact)
        let mut blob = vec![0xAA; 20];
        blob[0..16].copy_from_slice(&[0xBB; 16]);
        let result = parse_desktop_order_blob(&blob);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], [0xBB; 16]);
    }
}
