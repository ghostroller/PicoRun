use super::{ffi::*, wide};
use crate::i18n::Text;
mod dedup;
mod packaged;
mod path_key;
use crate::{
    catalog::Catalog,
    model::{AppEntry, LaunchTarget},
};
use path_key::PathKey;
use std::{
    collections::HashSet,
    ffi::c_void,
    fs, io,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};

// Known Folder IDs: Shell handles redirected paths and allocates the UTF-16 result.
const FOLDERS: [Guid; 4] = [
    Guid {
        a: 0xa77f5d77,
        b: 0x2e2b,
        c: 0x44c3,
        d: [0xa6, 0xa2, 0xab, 0xa6, 0x01, 0x05, 0x4a, 0x51],
    },
    Guid {
        a: 0x0139d44e,
        b: 0x6afe,
        c: 0x49f2,
        d: [0x86, 0x90, 0x3d, 0xaf, 0xca, 0xe6, 0xff, 0xb8],
    },
    Guid {
        a: 0xb4bfcc3a,
        b: 0xdb2c,
        c: 0x424c,
        d: [0xb0, 0x29, 0x7f, 0xe9, 0x9a, 0x87, 0xc6, 0x41],
    },
    Guid {
        a: 0xc4aa340d,
        b: 0xf20f,
        c: 0x4863,
        d: [0xaf, 0xef, 0xf8, 0x7e, 0xf2, 0xe6, 0xba, 0x25],
    },
];
pub fn known_folder(id: &Guid) -> io::Result<PathBuf> {
    known_folder_with_flags(id, 0)
}
fn known_folder_with_flags(id: &Guid, flags: u32) -> io::Result<PathBuf> {
    let mut ptr = null_mut();
    unsafe {
        // Shell owns the allocation until it returns; its contract requires freeing the
        // returned pointer on failure too. No window/controller borrow crosses this call.
        if SHGetKnownFolderPath(id, flags, null_mut(), &mut ptr) < 0 || ptr.is_null() {
            CoTaskMemFree(ptr.cast());
            return Err(io::Error::other(Text::KnownFolders));
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        let path = PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(ptr, len)));
        CoTaskMemFree(ptr.cast());
        Ok(path)
    }
}
pub fn data_directory() -> io::Result<PathBuf> {
    known_folder(&Guid {
        a: 0xf1b32785,
        b: 0x6fba,
        c: 0x4fcf,
        d: [0x9d, 0x55, 0x7b, 0x8e, 0x7f, 0x15, 0x70, 0x91],
    })
    .map(|p| p.join("PicoRun"))
}
pub fn roots() -> (Vec<PathBuf>, usize) {
    source_roots(known_folder_with_flags)
}
fn source_roots(
    mut resolve: impl FnMut(&Guid, u32) -> io::Result<PathBuf>,
) -> (Vec<PathBuf>, usize) {
    let mut paths = Vec::new();
    let mut failures = 0;
    for id in FOLDERS {
        // KF_FLAG_DONT_VERIFY: retrieve the configured location even when a redirected
        // directory is offline. read_dir then records its path for old-entry preservation.
        // The LocalAppData query above retains its existing verified-path behavior.
        match resolve(&id, 0x4000) {
            Ok(path) => paths.push(path),
            Err(_) => failures += 1,
        }
    }
    (paths, failures)
}

// COM wrappers own one reference; calls run on the initialized UI STA.
pub(super) struct ComObject(pub *mut c_void);
impl ComObject {
    pub unsafe fn method(&self, index: usize) -> *const c_void {
        *(*(self.0 as *const *const *const c_void)).add(index)
    }
}
impl Drop for ComObject {
    fn drop(&mut self) {
        unsafe {
            let release: unsafe extern "system" fn(*mut c_void) -> u32 =
                std::mem::transmute(self.method(2));
            release(self.0);
        }
    }
}
pub(super) const SHELL_LINK: Guid = Guid {
    a: 0x00021401,
    b: 0,
    c: 0,
    d: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
pub(super) const SHELL_LINK_W: Guid = Guid {
    a: 0x000214f9,
    b: 0,
    c: 0,
    d: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
pub(super) const PERSIST_FILE: Guid = Guid {
    a: 0x0000010b,
    b: 0,
    c: 0,
    d: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
pub(super) fn shell_link() -> io::Result<(ComObject, ComObject)> {
    unsafe {
        let mut object = null_mut();
        if CoCreateInstance(&SHELL_LINK, null_mut(), 1, &SHELL_LINK_W, &mut object) < 0 {
            return Err(io::Error::other(Text::LinkCreate));
        }
        let link = ComObject(object);
        let query: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
            std::mem::transmute(link.method(0));
        let mut persist = null_mut();
        if query(link.0, &PERSIST_FILE, &mut persist) < 0 {
            return Err(io::Error::other(Text::LinkRead));
        }
        Ok((link, ComObject(persist)))
    }
}
struct ShortcutReader {
    link: ComObject,
    persist: ComObject,
    buffer: Vec<u16>,
    metadata: Vec<u8>,
}
impl ShortcutReader {
    fn new() -> io::Result<Self> {
        let (link, persist) = shell_link()?;
        Ok(Self {
            link,
            persist,
            buffer: vec![0; 32768],
            metadata: Vec::new(),
        })
    }
    fn application_target(&mut self, path: &Path) -> Option<String> {
        unsafe {
            let load: unsafe extern "system" fn(*mut c_void, *const u16, u32) -> i32 =
                std::mem::transmute(self.persist.method(5));
            if load(self.persist.0, wide(path).as_ptr(), 0) < 0 {
                return None;
            }
            let get: unsafe extern "system" fn(
                *mut c_void,
                *mut u16,
                i32,
                *mut c_void,
                u32,
            ) -> i32 = std::mem::transmute(self.link.method(3));
            self.buffer[0] = 0;
            if get(
                self.link.0,
                self.buffer.as_mut_ptr(),
                self.buffer.len() as i32,
                null_mut(),
                4,
            ) < 0
            {
                return None;
            }
            let len = self
                .buffer
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(self.buffer.len());
            let target = String::from_utf16_lossy(&self.buffer[..len]);
            // Inspection excludes document/folder/URL shortcuts; launching still uses the .lnk.
            Path::new(&target)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
                .then_some(target)
        }
    }
}
fn sort_entries(entries: &mut [AppEntry]) {
    // The first precomputed search key is the normalized display name. No sort-time strings.
    entries.sort_unstable_by(|a, b| {
        a.keys[0]
            .cmp(&b.keys[0])
            .then_with(|| match (&a.target, &b.target) {
                (LaunchTarget::ShellPath(a), LaunchTarget::ShellPath(b)) => a.cmp(b),
                (LaunchTarget::AppUserModelId(a), LaunchTarget::AppUserModelId(b)) => a.cmp(b),
                (LaunchTarget::ShellPath(_), LaunchTarget::AppUserModelId(_)) => {
                    std::cmp::Ordering::Less
                }
                _ => std::cmp::Ordering::Greater,
            })
    });
}
pub struct Scan {
    pub entries: Vec<AppEntry>,
    pub failed: Vec<PathBuf>,
    pub packaged_failed: bool,
}
impl Scan {
    pub fn failure_count(&self) -> usize {
        self.failed.len() + usize::from(self.packaged_failed)
    }
    pub fn preserve_unreadable(&mut self, previous: &Catalog) {
        if self.failed.is_empty() && !self.packaged_failed {
            return;
        }
        let mut seen: HashSet<_> = self
            .entries
            .iter()
            .filter_map(|e| {
                if let LaunchTarget::ShellPath(path) = &e.target {
                    Some(PathKey::new(path))
                } else {
                    None
                }
            })
            .collect();
        let failed: Vec<_> = self.failed.iter().map(|root| PathKey::new(root)).collect();
        for entry in previous.entries() {
            if let LaunchTarget::ShellPath(path) = &entry.target {
                let key = PathKey::new(path);
                if failed.iter().any(|root| key.is_within(root)) && seen.insert(key) {
                    self.entries.push(entry.clone());
                }
            }
        }
        if self.packaged_failed {
            let mut ids: HashSet<_> = self
                .entries
                .iter()
                .filter_map(|entry| match &entry.target {
                    LaunchTarget::AppUserModelId(id) => Some(id.clone()),
                    _ => None,
                })
                .collect();
            for entry in previous.entries() {
                if let LaunchTarget::AppUserModelId(id) = &entry.target {
                    if ids.insert(id.clone()) {
                        self.entries.push(entry.clone());
                    }
                }
            }
        }
        // Failed sources have unknown current launch metadata: retain them conservatively.
        sort_entries(&mut self.entries);
    }
}
pub fn discover(roots: &[PathBuf], previous: Option<&Catalog>) -> io::Result<Scan> {
    discover_sources(roots, previous, false)
}
/// Only default sources include the Shell's packaged apps. Explicit --source remains isolated.
pub fn discover_sources(
    roots: &[PathBuf],
    previous: Option<&Catalog>,
    include_packaged: bool,
) -> io::Result<Scan> {
    if roots.is_empty() && !include_packaged {
        return Err(io::Error::other(Text::RootsMissing));
    }
    let mut reader = ShortcutReader::new()?;
    let mut deduper = dedup::Collector::default();
    let mut failed = Vec::new();
    // Known folders arrive in user Programs, common Programs, user/public Desktop order.
    // Explicit --source roots use their supplied order. Priority travels through recursion.
    let mut stack: Vec<_> = roots
        .iter()
        .enumerate()
        .rev()
        .map(|(priority, p)| (p.clone(), 0, priority))
        .collect();
    let mut seen = HashSet::new();
    while let Some((directory, depth, priority)) = stack.pop() {
        let children = match fs::read_dir(&directory) {
            Ok(children) => children,
            Err(_) => {
                failed.push(directory);
                continue;
            }
        };
        for child in children {
            let child = match child {
                Ok(child) => child,
                Err(_) => {
                    failed.push(directory.clone());
                    continue;
                }
            };
            let path = child.path();
            let kind = match child.file_type() {
                Ok(kind) => kind,
                Err(_) => {
                    failed.push(directory.clone());
                    continue;
                }
            };
            use std::os::windows::fs::MetadataExt;
            if kind.is_symlink()
                || child
                    .metadata()
                    .is_ok_and(|m| m.file_attributes() & 0x400 != 0)
            {
                continue;
            }
            if kind.is_dir() {
                if depth < 16 {
                    stack.push((path, depth + 1, priority));
                } else {
                    failed.push(path);
                }
                continue;
            }
            let Some(extension) = path.extension() else {
                continue;
            };
            let shortcut = extension.eq_ignore_ascii_case("lnk");
            if !shortcut && !extension.eq_ignore_ascii_case("exe") {
                continue;
            }
            // Skip overlapping roots before COM loads the same shortcut again.
            if !seen.insert(PathKey::new(&path)) {
                continue;
            }
            let target = if shortcut {
                let Some(target) = reader.application_target(&path) else {
                    continue;
                };
                Some(target)
            } else {
                None
            };
            let Some(name) = path.file_stem() else {
                continue;
            };
            deduper.insert(
                name.to_string_lossy().into_owned(),
                path,
                priority,
                target,
                &mut reader,
            );
        }
    }
    drop(seen);
    drop(reader);
    // Release all grouping keys before allocating pinyin aliases for the survivors.
    let mut entries = deduper.into_entries();
    let mut packaged_failed = false;
    if include_packaged {
        match packaged::discover() {
            Ok(apps) => entries.extend(apps),
            Err(_) => packaged_failed = true,
        }
    }
    sort_entries(&mut entries);
    let mut scan = Scan {
        entries,
        failed,
        packaged_failed,
    };
    if let Some(previous) = previous {
        scan.preserve_unreadable(previous);
    }
    let Scan {
        entries,
        failed,
        packaged_failed,
    } = scan;
    if roots.iter().all(|root| failed.contains(root))
        && entries.is_empty()
        && (!include_packaged || packaged_failed)
    {
        return Err(io::Error::other(Text::RootsUnreadable));
    }
    Ok(Scan {
        entries,
        failed,
        packaged_failed,
    })
}

pub fn launch(hwnd: Hwnd, target: &LaunchTarget) -> io::Result<()> {
    let path = match target {
        LaunchTarget::ShellPath(path) => path,
        LaunchTarget::AppUserModelId(id) => return packaged::launch(id),
    };
    if !path.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, Text::EntryMissing));
    }
    // No .lnk parameter or working-directory overrides.
    let file = wide(path);
    let verb = wide("open");
    let mut info = ShellExecuteInfo {
        size: std::mem::size_of::<ShellExecuteInfo>() as u32,
        // Wait for Shell activation, give our own inline errors; security prompts remain enabled.
        mask: 0x100 | 0x400,
        hwnd,
        verb: verb.as_ptr(),
        file: file.as_ptr(),
        parameters: null(),
        directory: null(),
        show: 1,
        instance: null_mut(),
        id_list: null_mut(),
        class: null(),
        class_key: null_mut(),
        hotkey: 0,
        icon_or_monitor: null_mut(),
        process: null_mut(),
    };
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        Err(io::Error::other(crate::i18n::Failure::Launch(unsafe {
            GetLastError()
        })))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_failure_preserves_only_previous_packaged_entries_without_duplicates() {
        let packaged =
            |name: &str, id: &str| AppEntry::new(name, LaunchTarget::AppUserModelId(id.into()));
        let previous = Catalog::new(vec![
            packaged("旧名称", "Sample.App_1234567890abc!App"),
            packaged("保留", "Other.App_1234567890abc!App"),
            AppEntry::new(
                "删除",
                LaunchTarget::ShellPath("readable/deleted.lnk".into()),
            ),
        ]);
        let mut scan = Scan {
            entries: vec![packaged("新名称", "Sample.App_1234567890abc!App")],
            failed: Vec::new(),
            packaged_failed: true,
        };
        scan.preserve_unreadable(&previous);
        assert_eq!(scan.failure_count(), 1);
        assert_eq!(scan.entries.len(), 2);
        assert!(scan.entries.iter().any(|entry| entry.name == "新名称"));
        assert!(scan.entries.iter().any(|entry| entry.name == "保留"));
        assert!(!scan.entries.iter().any(|entry| entry.name == "旧名称"));

        let mut successful = Scan {
            entries: Vec::new(),
            failed: vec!["readable".into()],
            packaged_failed: false,
        };
        successful.preserve_unreadable(&previous);
        assert_eq!(successful.entries.len(), 1);
        assert_eq!(successful.entries[0].name, "删除");
    }

    #[test]
    fn same_name_packaged_entries_have_stable_id_order() {
        let mut entries = vec![
            AppEntry::new("同名", LaunchTarget::AppUserModelId("B!App".into())),
            AppEntry::new("同名", LaunchTarget::AppUserModelId("A!App".into())),
            AppEntry::new("同名", LaunchTarget::ShellPath("same.lnk".into())),
        ];
        sort_entries(&mut entries);
        assert!(matches!(entries[0].target, LaunchTarget::ShellPath(_)));
        assert_eq!(
            entries[1].target,
            LaunchTarget::AppUserModelId("A!App".into())
        );
        assert_eq!(
            entries[2].target,
            LaunchTarget::AppUserModelId("B!App".into())
        );
    }

    #[test]
    fn source_lookup_keeps_offline_location_for_real_scan_recovery() {
        unsafe {
            assert!(CoInitializeEx(null_mut(), 2) >= 0);
        }
        let root =
            std::env::temp_dir().join(format!("picorun-offline-source-{}", std::process::id()));
        let okay = root.join("okay");
        let offline = root.join("offline");
        fs::create_dir_all(&okay).unwrap();
        fs::write(okay.join("current.exe"), "synthetic; never executed").unwrap();
        let configured = [okay.clone(), offline.clone(), okay.clone(), okay];
        let mut next = 0;
        // Model Shell's documented verified-path failure without changing the user's
        // Known Folder registry. The subsequent directory scan is real filesystem IO.
        let (roots, unavailable) = source_roots(|_, flags| {
            let path = configured[next].clone();
            next += 1;
            if flags & 0x4000 == 0 && !path.is_dir() {
                Err(io::Error::new(io::ErrorKind::NotFound, "offline source"))
            } else {
                Ok(path)
            }
        });
        assert_eq!(unavailable, 0);
        assert!(roots.contains(&offline));
        let previous = Catalog::new(vec![AppEntry::new(
            "old",
            LaunchTarget::ShellPath(offline.join("old.lnk")),
        )]);
        let scan = discover(&roots, Some(&previous)).unwrap();
        assert_eq!(scan.failed, [offline]);
        assert_eq!(scan.entries.len(), 2);
        assert!(scan.entries.iter().any(|entry| entry.name == "old"));
        fs::remove_dir_all(root).unwrap();
        unsafe {
            CoUninitialize();
        }
    }

    #[test]
    fn overlapping_sources_keep_distinct_unicode_files() {
        unsafe {
            assert!(CoInitializeEx(null_mut(), 2) >= 0);
        }
        let root =
            std::env::temp_dir().join(format!("picorun-unicode-scan-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let first = root.join("İ.exe");
        let second = root.join("i\u{0307}.exe");
        fs::write(&first, "synthetic first; never executed").unwrap();
        fs::write(&second, "synthetic second; never executed").unwrap();
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        let uppercase = PathBuf::from(root.as_os_str().to_ascii_uppercase());
        let scan = discover(&[root.clone(), uppercase], None).unwrap();
        assert!(scan.failed.is_empty());
        assert_eq!(scan.entries.len(), 2);
        for path in [first, second] {
            assert!(scan
                .entries
                .iter()
                .any(|entry| entry.target == LaunchTarget::ShellPath(path.clone())));
        }
        fs::remove_dir_all(root).unwrap();
        unsafe {
            CoUninitialize();
        }
    }

    #[test]
    fn failed_source_recovery_uses_case_and_component_aware_paths() {
        let entry = |path: &str| AppEntry::new("same", LaunchTarget::ShellPath(path.into()));
        let mut scan = Scan {
            entries: vec![entry("c:\\apps\\ascii.exe"), entry("c:\\apps\\İ.exe")],
            failed: vec![PathBuf::from("C:\\APPS")],
            packaged_failed: false,
        };
        let previous = Catalog::new(vec![
            entry("C:\\Apps\\ASCII.exe"),
            entry("C:\\Apps\\i\u{0307}.exe"),
            entry("C:\\Apps-other\\outside.exe"),
        ]);
        scan.preserve_unreadable(&previous);
        assert_eq!(scan.entries.len(), 3);
        assert!(scan.entries.iter().any(|entry| {
            entry.target == LaunchTarget::ShellPath("C:\\Apps\\i\u{0307}.exe".into())
        }));
    }

    #[test]
    fn partial_failure_preserves_only_unreadable_entries() {
        unsafe {
            assert!(CoInitializeEx(null_mut(), 2) >= 0);
        }
        let root = std::env::temp_dir().join(format!("picorun-scan-test-{}", std::process::id()));
        let okay = root.join("okay");
        let missing = root.join("missing");
        fs::create_dir_all(&okay).unwrap();
        fs::write(okay.join("新应用.exe"), "synthetic; never executed").unwrap();
        fs::write(okay.join("document.txt"), "not application").unwrap();
        let previous = Catalog::new(vec![
            AppEntry::new("保留", LaunchTarget::ShellPath(missing.join("保留.lnk"))),
            AppEntry::new("删除", LaunchTarget::ShellPath(okay.join("deleted.exe"))),
        ]);
        let scan = discover(&[okay, missing.clone()], Some(&previous)).unwrap();
        assert_eq!(scan.entries.len(), 2);
        assert!(scan.entries.iter().any(|e| e.name == "保留"));
        assert!(!scan.entries.iter().any(|e| e.name == "删除"));
        assert_eq!(scan.failed.as_slice(), std::slice::from_ref(&missing));
        assert!(discover(&[missing], None).is_err());
        assert!(discover(&[], Some(&previous)).is_err());
        fs::remove_dir_all(root).unwrap();
        unsafe {
            CoUninitialize();
        }
    }
}
