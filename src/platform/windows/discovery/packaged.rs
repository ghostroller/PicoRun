//! Current-user packaged applications exposed by the native AppsFolder namespace.
//! COM objects and property strings exist only during startup/F5 discovery or activation.
use super::ComObject;
use crate::{
    i18n::Failure,
    model::{AppEntry, LaunchTarget},
    platform::windows::{ffi::*, wide},
};
use std::{ffi::c_void, io, ptr::null_mut};

// GUIDs and vtable order are from Windows SDK ShObjIdl_core.idl / ShlGuid.h.
const SHELL_ITEM: Guid = Guid {
    a: 0x43826d1e,
    b: 0xe718,
    c: 0x42ee,
    d: [0xbc, 0x55, 0xa1, 0xe2, 0x61, 0xc3, 0x7b, 0xfe],
};
const SHELL_ITEM_2: Guid = Guid {
    a: 0x7e9fb0d3,
    b: 0x919f,
    c: 0x4307,
    d: [0xab, 0x2e, 0x9b, 0x18, 0x60, 0x31, 0x0c, 0x93],
};
const ENUM_ITEMS: Guid = Guid {
    a: 0x94f60519,
    b: 0x2850,
    c: 0x4924,
    d: [0xaa, 0x5a, 0xd1, 0x5e, 0x84, 0x86, 0x80, 0x39],
};
const ENUM_SHELL_ITEMS: Guid = Guid {
    a: 0x70629033,
    b: 0xe363,
    c: 0x4a28,
    d: [0xa5, 0x67, 0x0d, 0xb7, 0x80, 0x06, 0xe6, 0xd7],
};
const ACTIVATION_MANAGER: Guid = Guid {
    a: 0x45ba127d,
    b: 0x10a8,
    c: 0x46ea,
    d: [0x8a, 0xb7, 0x56, 0xea, 0x90, 0x78, 0x94, 0x3c],
};
const APPLICATION_ACTIVATION_MANAGER: Guid = Guid {
    a: 0x2e941141,
    b: 0x7f97,
    c: 0x4756,
    d: [0xba, 0x1d, 0x9d, 0xec, 0xde, 0x89, 0x4a, 0x3d],
};
#[repr(C)]
struct PropertyKey {
    format: Guid,
    id: u32,
}
// PKEY_AppUserModel_ID, Windows SDK propkey.h.
const APP_USER_MODEL_ID: PropertyKey = PropertyKey {
    format: Guid {
        a: 0x9f4c2855,
        b: 0x9f79,
        c: 0x4b39,
        d: [0xa8, 0xd0, 0xe1, 0xd4, 0x2d, 0xe1, 0xd5, 0xf3],
    },
    id: 5,
};
const E_UNEXPECTED: i32 = 0x8000ffffu32 as i32;
const E_INVALIDARG: i32 = 0x80070057u32 as i32;
// APPLICATION_USER_MODEL_ID_MAX_LENGTH in minappmodel.h includes its NUL.
const MAX_ID_UNITS: usize = 129;
const MAX_NAME_UNITS: usize = 32768;
const MAX_ITEMS: usize = 100_000;
const BYTE_BUDGET: usize = 16 * 1024 * 1024;

fn failure(code: i32) -> io::Error {
    io::Error::other(Failure::PackagedDiscovery(code))
}
fn win32_hresult(code: i32) -> i32 {
    if code <= 0 {
        code
    } else {
        (0x80070000u32 | (code as u32 & 0xffff)) as i32
    }
}

// Native methods return one owned reference. Wrap even nonnull failure output so it
// is released on every path; a successful null output is a source error.
fn object(code: i32, pointer: *mut c_void) -> io::Result<ComObject> {
    let owned = (!pointer.is_null()).then(|| ComObject(pointer));
    if code < 0 {
        Err(failure(code))
    } else {
        owned.ok_or_else(|| failure(E_UNEXPECTED))
    }
}
// propidlbase.h: four 16-bit header fields followed by the native value union.
// Count/pointer pairs occupy 16 bytes on x64 or 8 on x86; u64 imposes 8-byte
// alignment. The outer DECIMAL union fits this layout (24 / 16 bytes respectively).
#[repr(C, align(8))]
union PropertyData {
    string: *mut u16,
    storage: [usize; 2],
}
#[repr(C)]
struct PropertyValue {
    kind: u16,
    reserved: [u16; 3],
    data: PropertyData,
}
impl PropertyValue {
    fn new() -> Self {
        // Zero is VT_EMPTY. These fields cover every byte of the native layout,
        // including the entire value union; initialize them before entering COM.
        Self {
            kind: 0,
            reserved: [0; 3],
            data: PropertyData { storage: [0; 2] },
        }
    }
    fn string(&self, code: i32, limit: usize) -> io::Result<Option<String>> {
        if code < 0 {
            return Err(failure(code));
        }
        match self.kind {
            0 => Ok(None), // VT_EMPTY: a successfully read but absent property.
            31 => {
                // VT_LPWSTR selects the valid native union member. The property's
                // allocation stays owned here until after the copied String returns.
                unsafe { read_string(self.data.string, limit).map(Some) }
            }
            _ => Err(failure(E_INVALIDARG)), // PKEY_AppUserModel_ID is a string.
        }
    }
}
impl Drop for PropertyValue {
    fn drop(&mut self) {
        // Clear every native output, including any partially filled failed result.
        // In particular, VT_LPWSTR is released here rather than by TaskString.
        unsafe { PropVariantClear((self as *mut Self).cast()) };
    }
}

// Caller guarantees a readable NUL-terminated Shell string and keeps its owner
// alive. This helper borrows the pointer and never releases or retains it.
unsafe fn read_string(pointer: *const u16, limit: usize) -> io::Result<String> {
    if pointer.is_null() {
        return Err(failure(E_UNEXPECTED));
    }
    unsafe {
        for len in 0..=limit {
            if *pointer.add(len) == 0 {
                return String::from_utf16(std::slice::from_raw_parts(pointer, len))
                    .map_err(|_| failure(E_INVALIDARG));
            }
        }
    }
    Err(failure(E_INVALIDARG))
}

struct TaskString(*mut u16);
impl TaskString {
    fn read(&self, limit: usize) -> io::Result<String> {
        // The Shell contract returns a NUL-terminated task allocation. The bound
        // prevents retaining unreasonably large provider strings; no pointer escapes.
        unsafe { read_string(self.0, limit) }
    }
}
impl Drop for TaskString {
    fn drop(&mut self) {
        // All GetDisplayName results use the COM task allocator, including
        // any nonnull output from a failed call. CoTaskMemFree accepts null.
        unsafe { CoTaskMemFree(self.0.cast()) };
    }
}

fn package_family(id: &str) -> Option<&str> {
    if id.contains('\0') || id.encode_utf16().count() > MAX_ID_UNITS {
        return None;
    }
    let (family, application) = id.split_once('!')?;
    (!family.is_empty() && !application.is_empty() && !application.contains('!')).then_some(family)
}
fn registered_id(id: &str, installed: impl FnOnce(&str) -> io::Result<bool>) -> io::Result<bool> {
    match package_family(id) {
        Some(family) => installed(family),
        None => Ok(false),
    }
}
fn installed_family(family: &str) -> io::Result<bool> {
    let mut count = 0;
    let mut units = 0;
    // The sizing call is enough to verify a registered current-user package. No
    // manifests, install paths, full-name arrays or package metadata are retained.
    let code = unsafe {
        GetPackagesByPackageFamily(
            wide(family).as_ptr(),
            &mut count,
            null_mut(),
            &mut units,
            null_mut(),
        )
    };
    match code {
        0 | 122 => Ok(count > 0),
        87 => Ok(false), // An ordinary Win32 custom AppID need not name a package.
        _ => Err(failure(win32_hresult(code))),
    }
}
fn app_id(item: &ComObject) -> io::Result<Option<String>> {
    unsafe {
        let query: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
            std::mem::transmute(item.method(0));
        let mut pointer = null_mut();
        let code = query(item.0, &SHELL_ITEM_2, &mut pointer);
        let item2 = object(code, pointer)?;
        let get: unsafe extern "system" fn(
            *mut c_void,
            *const PropertyKey,
            *mut PropertyValue,
        ) -> i32 = std::mem::transmute(item2.method(13)); // IShellItem2::GetProperty.
        let mut value = PropertyValue::new();
        let code = get(item2.0, &APP_USER_MODEL_ID, &mut value);
        let Some(id) = value.string(code, MAX_ID_UNITS)? else {
            // AppsFolder also contains ordinary Win32 entries. Only a successfully
            // read absent property is skipped; errors trigger old-source recovery.
            return Ok(None);
        };
        Ok(registered_id(&id, installed_family)?.then_some(id))
    }
}
fn display_name(item: &ComObject) -> io::Result<String> {
    unsafe {
        let get: unsafe extern "system" fn(*mut c_void, u32, *mut *mut u16) -> i32 =
            std::mem::transmute(item.method(5));
        let mut pointer = null_mut();
        let code = get(item.0, 0, &mut pointer); // SIGDN_NORMALDISPLAY: localized Shell name.
        let text = TaskString(pointer);
        if code < 0 {
            return Err(failure(code));
        }
        let name = text.read(MAX_NAME_UNITS)?;
        if name.trim().is_empty() {
            return Err(failure(E_INVALIDARG));
        }
        Ok(name)
    }
}
fn entries(mut records: Vec<(String, String)>) -> Vec<AppEntry> {
    // Deduplicate only the opaque launch identity, never same-name distinct apps.
    // Sorting also makes a duplicate identity's display-name choice deterministic.
    records.sort_unstable_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    records.dedup_by(|a, b| a.1 == b.1);
    records
        .into_iter()
        .map(|(name, id)| AppEntry::new(name, LaunchTarget::AppUserModelId(id)))
        .collect()
}

pub(super) fn discover() -> io::Result<Vec<AppEntry>> {
    // Caller initializes COM STA and holds no mutable Runtime/controller borrow:
    // Shell calls may dispatch messages. Every object is released on this thread.
    let mut pointer = null_mut();
    let code = unsafe {
        SHCreateItemFromParsingName(
            wide("shell:AppsFolder").as_ptr(),
            null_mut(),
            &SHELL_ITEM,
            &mut pointer,
        )
    };
    let folder = object(code, pointer)?;
    let mut pointer = null_mut();
    let enumerator = unsafe {
        let bind: unsafe extern "system" fn(
            *mut c_void,
            *mut c_void,
            *const Guid,
            *const Guid,
            *mut *mut c_void,
        ) -> i32 = std::mem::transmute(folder.method(3));
        object(
            bind(
                folder.0,
                null_mut(),
                &ENUM_ITEMS,
                &ENUM_SHELL_ITEMS,
                &mut pointer,
            ),
            pointer,
        )?
    };
    let next: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void, *mut u32) -> i32 =
        unsafe { std::mem::transmute(enumerator.method(3)) };
    let mut records = Vec::new();
    let mut bytes = 0;
    let mut visited = 0;
    loop {
        let mut pointer = null_mut();
        let mut fetched = 0;
        let code = unsafe { next(enumerator.0, 1, &mut pointer, &mut fetched) };
        let item = (!pointer.is_null()).then(|| ComObject(pointer));
        if code == 1 && fetched == 0 && item.is_none() {
            break; // S_FALSE is the successful end, not an empty/failed source.
        }
        if code < 0 {
            return Err(failure(code));
        }
        if code != 0 || fetched != 1 || visited == MAX_ITEMS {
            return Err(failure(E_UNEXPECTED));
        }
        visited += 1;
        let item = item.ok_or_else(|| failure(E_UNEXPECTED))?;
        let Some(id) = app_id(&item)? else {
            continue;
        };
        // A known registered packaged identity with unreadable name means the
        // source is incomplete; the caller can preserve the previous snapshot.
        let name = display_name(&item)?;
        bytes += name.capacity() + id.capacity() + std::mem::size_of::<(String, String)>();
        if bytes > BYTE_BUDGET {
            return Err(failure(E_UNEXPECTED));
        }
        records.push((name, id));
    }
    drop(enumerator);
    drop(folder);
    // Release Shell's scan objects before generating the pinyin keys.
    Ok(entries(records))
}

pub(super) fn launch(id: &str) -> io::Result<()> {
    if package_family(id).is_none() {
        return Err(io::Error::other(Failure::Packaged(E_INVALIDARG)));
    }
    if super::super::requires_launch_isolation()? {
        return super::super::launch::detached_activation(id);
    }
    launch_direct(id)
}
pub(super) fn launch_direct(id: &str) -> io::Result<()> {
    if package_family(id).is_none() {
        return Err(io::Error::other(Failure::Packaged(E_INVALIDARG)));
    }
    // A long-lived launcher can activate in-process. This avoids launching
    // explorer.exe or an external script; Windows owns the packaged app contract.
    let mut pointer = null_mut();
    let code = unsafe {
        CoCreateInstance(
            &ACTIVATION_MANAGER,
            null_mut(),
            1, // CLSCTX_INPROC_SERVER
            &APPLICATION_ACTIVATION_MANAGER,
            &mut pointer,
        )
    };
    let manager = object(code, pointer).map_err(|_| {
        io::Error::other(Failure::Packaged(if code < 0 {
            code
        } else {
            E_UNEXPECTED
        }))
    })?;
    let mut process = 0;
    let code = unsafe {
        let activate: unsafe extern "system" fn(
            *mut c_void,
            *const u16,
            *const u16,
            u32,
            *mut u32,
        ) -> i32 = std::mem::transmute(manager.method(3));
        // AO_NOERRORUI: the launcher reports the HRESULT in its existing status.
        // No arguments or splash/debug/prelaunch flags alter normal launch semantics.
        activate(manager.0, wide(id).as_ptr(), null_mut(), 2, &mut process)
    };
    if code < 0 {
        Err(io::Error::other(Failure::Packaged(code)))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_property_failure(result: io::Result<Option<String>>, expected: i32) {
        let error = result.unwrap_err();
        assert!(error.get_ref().is_some_and(|cause| {
            matches!(
                cause.downcast_ref::<Failure>(),
                Some(Failure::PackagedDiscovery(code)) if *code == expected
            )
        }));
    }

    #[test]
    fn property_layout_matches_windows_sdk() {
        // SDK PROPVARIANT's DECIMAL/value arms require 8-byte alignment on both
        // Windows targets. This checks the layout on the target being built.
        let value_bytes = if std::mem::size_of::<usize>() == 8 {
            16
        } else {
            8
        };
        assert_eq!(std::mem::size_of::<PropertyData>(), value_bytes);
        assert_eq!(std::mem::align_of::<PropertyData>(), 8);
        assert_eq!(std::mem::size_of::<PropertyValue>(), value_bytes + 8);
        assert_eq!(std::mem::align_of::<PropertyValue>(), 8);
        assert_eq!(std::mem::offset_of!(PropertyValue, kind), 0);
        assert_eq!(std::mem::offset_of!(PropertyValue, data), 8);
    }

    #[test]
    fn missing_property_is_distinct_from_native_failure_or_wrong_type() {
        let empty = PropertyValue::new();
        assert_eq!(empty.string(0, MAX_ID_UNITS).unwrap(), None);
        // Even VT_EMPTY output on failure cannot be treated as a removed app.
        for code in [0x80070005u32 as i32, 0x8007000eu32 as i32, E_UNEXPECTED] {
            assert_property_failure(empty.string(code, MAX_ID_UNITS), code);
        }
        let mut wrong_type = PropertyValue::new();
        wrong_type.kind = 3; // VT_I4 is valid native storage, but not this PKEY's type.
        assert_property_failure(wrong_type.string(0, MAX_ID_UNITS), E_INVALIDARG);
        let mut null_string = PropertyValue::new();
        null_string.kind = 31;
        assert_property_failure(null_string.string(0, MAX_ID_UNITS), E_UNEXPECTED);
    }

    #[test]
    fn property_strings_use_bounded_strict_utf16_without_taking_borrowed_ownership() {
        fn borrowed_string(text: &mut [u16]) -> std::mem::ManuallyDrop<PropertyValue> {
            // Test buffers are Rust-owned; suppress native cleanup only for these
            // borrowed fixtures. Production property outputs always use normal Drop.
            std::mem::ManuallyDrop::new(PropertyValue {
                kind: 31,
                reserved: [0; 3],
                data: PropertyData {
                    string: text.as_mut_ptr(),
                },
            })
        }
        let expected = "Synthetic.商店_1234567890abc!App";
        let mut text: Vec<u16> = expected.encode_utf16().chain([0]).collect();
        assert_eq!(
            borrowed_string(&mut text)
                .string(0, MAX_ID_UNITS)
                .unwrap()
                .as_deref(),
            Some(expected)
        );

        let mut boundary: Vec<u16> = "a".repeat(MAX_ID_UNITS).encode_utf16().chain([0]).collect();
        assert_eq!(
            borrowed_string(&mut boundary)
                .string(0, MAX_ID_UNITS)
                .unwrap()
                .unwrap()
                .len(),
            MAX_ID_UNITS
        );

        let mut oversized: Vec<u16> = "a"
            .repeat(MAX_ID_UNITS + 1)
            .encode_utf16()
            .chain([0])
            .collect();
        assert_property_failure(
            borrowed_string(&mut oversized).string(0, MAX_ID_UNITS),
            E_INVALIDARG,
        );
        assert_property_failure(
            borrowed_string(&mut [0xd800, 0]).string(0, MAX_ID_UNITS),
            E_INVALIDARG,
        );
    }

    #[test]
    fn accepts_only_registered_packaged_launch_identities() {
        let id = "Microsoft.WindowsStore_8wekyb3d8bbwe!App";
        assert!(registered_id(id, |family| {
            assert_eq!(family, "Microsoft.WindowsStore_8wekyb3d8bbwe");
            Ok(true)
        })
        .unwrap());
        assert!(!registered_id(id, |_| Ok(false)).unwrap());
        for id in [
            "",
            "Company.Desktop",
            "!App",
            "Family!",
            "Family!App!Extra",
            "Family!App\0",
        ] {
            assert!(
                !registered_id(id, |_| panic!("malformed IDs must not query packages")).unwrap()
            );
        }
        let boundary = format!("Family!{}", "a".repeat(122));
        assert!(registered_id(&boundary, |_| Ok(true)).unwrap());
        let oversized = format!("{boundary}a");
        assert!(!registered_id(&oversized, |_| panic!("oversized ID")).unwrap());
        assert!(registered_id("Family!App", |_| Err(failure(E_UNEXPECTED))).is_err());
    }

    #[test]
    fn same_identity_merges_but_same_name_distinct_apps_and_chinese_keys_remain() {
        let found = entries(vec![
            ("商店 Store".into(), "Family!First".into()),
            ("商店 Store".into(), "Family!Second".into()),
            ("商店 Store".into(), "Family!First".into()),
        ]);
        assert_eq!(found.len(), 2);
        assert_ne!(found[0].target, found[1].target);
        assert!(found
            .iter()
            .all(|app| app.keys.iter().any(|key| key == "shangdian store")));
        assert!(found
            .iter()
            .all(|app| app.keys.iter().any(|key| key == "sd store")));
    }

    #[test]
    fn invalid_activation_id_returns_before_calling_windows() {
        for id in ["", "Ordinary.Desktop", "Family!App\0"] {
            let error = launch(id).unwrap_err();
            assert!(error.get_ref().is_some_and(|cause| {
                matches!(
                    cause.downcast_ref::<Failure>(),
                    Some(Failure::Packaged(E_INVALIDARG))
                )
            }));
        }
    }
}
