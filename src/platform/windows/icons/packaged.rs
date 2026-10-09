//! AppsFolder icon extraction, called only by the initialized STA icon worker.
use super::super::{discovery::ComObject, ffi::*};
use super::{wide, Extracted, Icon};
use std::{ffi::c_void, mem::size_of, ptr::null_mut, sync::Arc};

// Windows SDK ShObjIdl_core.idl / ShlGuid.h GUIDs and vtable slots.
const SHELL_ITEM: Guid = Guid {
    a: 0x43826d1e,
    b: 0xe718,
    c: 0x42ee,
    d: [0xbc, 0x55, 0xa1, 0xe2, 0x61, 0xc3, 0x7b, 0xfe],
};
const SHELL_UI_OBJECT: Guid = Guid {
    a: 0x3981e225,
    b: 0xf559,
    c: 0x11d3,
    d: [0x8e, 0x3a, 0x00, 0xc0, 0x4f, 0x68, 0x37, 0xd5],
};
const EXTRACT_ICON: Guid = Guid {
    a: 0x000214fa,
    b: 0,
    c: 0,
    d: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
const NOT_FILE_NAME: u32 = 0x0008;
const DO_NOT_CACHE: u32 = 0x0010;

// Windows SDK shellapi.h SHFILEINFOW, including the full output buffers.
#[repr(C)]
#[cfg_attr(target_pointer_width = "32", repr(packed(1)))]
struct ShellFileInfo {
    icon: Handle,
    index: i32,
    attributes: u32,
    display_name: [u16; 260],
    type_name: [u16; 80],
}
#[link(name = "shell32")]
unsafe extern "system" {
    fn SHParseDisplayName(
        name: *const u16,
        bind_context: *mut c_void,
        item: *mut *mut c_void,
        requested_attributes: u32,
        attributes: *mut u32,
    ) -> i32;
    fn SHGetFileInfoW(
        path_or_item: *const u16,
        attributes: u32,
        info: *mut ShellFileInfo,
        size: u32,
        flags: u32,
    ) -> usize;
}
struct ItemIdList(*mut c_void);
impl Drop for ItemIdList {
    fn drop(&mut self) {
        // SHParseDisplayName returns an absolute PIDL allocated by the COM task allocator.
        // Free nonnull failure output too; CoTaskMemFree accepts null.
        unsafe { CoTaskMemFree(self.0) };
    }
}
pub(super) fn valid_id(id: &str) -> bool {
    let Some((family, app)) = id.split_once('!') else {
        return false;
    };
    !family.is_empty()
        && !app.is_empty()
        && !app.contains('!')
        && !id.contains(['\0', '\\', '/', ':'])
        && id.encode_utf16().count() <= 129
}
fn object(code: i32, pointer: *mut c_void) -> Option<ComObject> {
    // Release nonnull failure output too. A successful null output is unusable.
    let owned = (!pointer.is_null()).then(|| ComObject(pointer));
    (code >= 0).then_some(owned).flatten()
}
fn exact(name: &[u16], size: u16, location: &mut [u16]) -> (Option<Arc<Icon>>, bool) {
    let mut pointer = null_mut();
    // Every COM reference stays on this initialized worker STA and is released before
    // returning. No UI borrow or request lock crosses these potentially reentrant calls.
    let code = unsafe {
        SHCreateItemFromParsingName(name.as_ptr(), null_mut(), &SHELL_ITEM, &mut pointer)
    };
    let Some(item) = object(code, pointer) else {
        return (None, true);
    };
    let mut pointer = null_mut();
    let code = unsafe {
        let bind: unsafe extern "system" fn(
            *mut c_void,
            *mut c_void,
            *const Guid,
            *const Guid,
            *mut *mut c_void,
        ) -> i32 = std::mem::transmute(item.method(3));
        bind(
            item.0,
            null_mut(),
            &SHELL_UI_OBJECT,
            &EXTRACT_ICON,
            &mut pointer,
        )
    };
    let Some(handler) = object(code, pointer) else {
        return (None, true);
    };
    if location.is_empty() || location.len() > u32::MAX as usize {
        return (None, true);
    }
    // The caller lends its existing bounded worker buffer; a missing terminator
    // from a broken/truncated handler output must not become a native string.
    location.fill(u16::MAX);
    let mut index = 0;
    let mut flags = 0;
    let code = unsafe {
        let get: unsafe extern "system" fn(
            *mut c_void,
            u32,
            *mut u16,
            u32,
            *mut i32,
            *mut u32,
        ) -> i32 = std::mem::transmute(handler.method(3));
        get(
            handler.0,
            0,
            location.as_mut_ptr(),
            location.len() as u32,
            &mut index,
            &mut flags,
        )
    };
    let cacheable = flags & DO_NOT_CACHE == 0;
    let Some(end) = location.iter().position(|&unit| unit == 0) else {
        return (None, cacheable);
    };
    if code != 0 {
        return (None, cacheable);
    }
    let mut icon = null_mut();
    let code = unsafe {
        let extract: unsafe extern "system" fn(
            *mut c_void,
            *const u16,
            u32,
            *mut Handle,
            *mut Handle,
            u32,
        ) -> i32 = std::mem::transmute(handler.method(4));
        // LOWORD is the desired large-icon pixel size; no small icon is requested.
        extract(
            handler.0,
            location.as_ptr(),
            index as u32,
            &mut icon,
            null_mut(),
            u32::from(size),
        )
    };
    // Wrap even nonnull failure output so all native handles have exactly one owner.
    let owned = (!icon.is_null()).then(|| Arc::new(Icon(icon as usize)));
    if code == 0 {
        return (owned, cacheable);
    }
    drop(owned);
    if code == 1 && flags & NOT_FILE_NAME == 0 {
        // S_FALSE asks the caller to extract a genuine file/index pair. Opaque
        // AppsFolder locations marked GIL_NOTFILENAME must stay with the handler.
        return (
            super::extract_file(&location[..=end], index, size),
            cacheable,
        );
    }
    (None, cacheable)
}
fn shell_fallback(name: &[u16]) -> Option<Arc<Icon>> {
    let mut item = ItemIdList(null_mut());
    // No UI borrow or request lock crosses either call. The owned absolute PIDL and
    // parsing-name buffer stay alive through SHGetFileInfoW; neither is retained.
    let code = unsafe { SHParseDisplayName(name.as_ptr(), null_mut(), &mut item.0, 0, null_mut()) };
    if code < 0 || item.0.is_null() {
        return None;
    }
    let mut info = ShellFileInfo {
        icon: null_mut(),
        index: 0,
        attributes: 0,
        display_name: [0; 260],
        type_name: [0; 80],
    };
    // Preserve the previous SHGFI_PIDL | SHGFI_ICON | SHGFI_LARGEICON path if the
    // item does not provide a working size-specific icon handler.
    let success = unsafe {
        SHGetFileInfoW(
            item.0.cast(),
            0,
            &mut info,
            size_of::<ShellFileInfo>() as u32,
            0x108,
        )
    };
    let icon = (!info.icon.is_null()).then(|| Arc::new(Icon(info.icon as usize)));
    // SHGFI_ICON gives an owned HICON. Failed nonnull output is dropped here;
    // success shares ownership between the bounded worker cache and UI snapshot.
    if success == 0 {
        None
    } else {
        icon
    }
}
pub(super) fn extract(id: &str, size: u16, location: &mut [u16]) -> Option<Extracted> {
    if !valid_id(id) || size == 0 {
        return None;
    }
    let name = wide(format!("shell:AppsFolder\\{id}"));
    let (icon, cacheable) = exact(&name, size, location);
    icon.or_else(|| shell_fallback(&name))
        .map(|icon| Extracted { icon, cacheable })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_file_info_matches_sdk_layout() {
        let handle = size_of::<Handle>();
        assert_eq!(std::mem::offset_of!(ShellFileInfo, index), handle);
        assert_eq!(std::mem::offset_of!(ShellFileInfo, attributes), handle + 4);
        assert_eq!(
            std::mem::offset_of!(ShellFileInfo, display_name),
            handle + 8
        );
        assert_eq!(std::mem::offset_of!(ShellFileInfo, type_name), handle + 528);
        assert_eq!(size_of::<ShellFileInfo>(), handle + 688);
    }
    #[test]
    fn rejects_non_aumids_before_shell_parsing() {
        assert!(valid_id("Synthetic.Family_1234567890abc!App"));
        for id in [
            "",
            "Desktop.App",
            "!App",
            "Family!",
            "Family!A!B",
            "Family!A\0B",
            "Family!A\\B",
            "Family!A/B",
            "Family!A:B",
        ] {
            assert!(extract(id, 25, &mut []).is_none());
        }
        assert!(!valid_id(&format!("Family!{}", "a".repeat(130))));
        assert!(extract("Synthetic.Family!App", 0, &mut []).is_none());
    }
}
