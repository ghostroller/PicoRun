//! AppsFolder icon extraction, called only by the initialized STA icon worker.
use super::super::ffi::*;
use super::{wide, Icon};
use std::{ffi::c_void, mem::size_of, ptr::null_mut, sync::Arc};

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
pub(super) fn extract(id: &str) -> Option<Arc<Icon>> {
    if !valid_id(id) {
        return None;
    }
    let name = wide(format!("shell:AppsFolder\\{id}"));
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
    // SHGFI_PIDL | SHGFI_ICON | SHGFI_LARGEICON. The first argument is a fully
    // qualified PIDL with SHGFI_PIDL, rather than a UTF-16 filesystem path.
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
            assert!(extract(id).is_none());
        }
        assert!(!valid_id(&format!("Family!{}", "a".repeat(130))));
    }
}
