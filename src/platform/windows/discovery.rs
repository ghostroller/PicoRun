use super::{ffi::*, wide};
use crate::{
    catalog::Catalog,
    model::{AppEntry, LaunchTarget},
};
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
    let mut ptr = null_mut();
    unsafe {
        if SHGetKnownFolderPath(id, 0, null_mut(), &mut ptr) < 0 {
            return Err(io::Error::other("无法读取系统已知目录"));
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
    let mut paths = Vec::new();
    let mut failures = 0;
    for id in FOLDERS {
        match known_folder(&id) {
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
            return Err(io::Error::other("无法创建 Shell Link"));
        }
        let link = ComObject(object);
        let query: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
            std::mem::transmute(link.method(0));
        let mut persist = null_mut();
        if query(link.0, &PERSIST_FILE, &mut persist) < 0 {
            return Err(io::Error::other("无法读取 Shell Link"));
        }
        Ok((link, ComObject(persist)))
    }
}
struct ShortcutReader {
    link: ComObject,
    persist: ComObject,
    buffer: Vec<u16>,
}
impl ShortcutReader {
    fn new() -> io::Result<Self> {
        let (link, persist) = shell_link()?;
        Ok(Self {
            link,
            persist,
            buffer: vec![0; 32768],
        })
    }
    fn is_application(&mut self, path: &Path) -> bool {
        unsafe {
            let load: unsafe extern "system" fn(*mut c_void, *const u16, u32) -> i32 =
                std::mem::transmute(self.persist.method(5));
            if load(self.persist.0, wide(path).as_ptr(), 0) < 0 {
                return false;
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
                return false;
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
        }
    }
}
pub struct Scan {
    pub entries: Vec<AppEntry>,
    pub failed: Vec<PathBuf>,
}
impl Scan {
    pub fn preserve_unreadable(&mut self, previous: &Catalog) {
        if self.failed.is_empty() {
            return;
        }
        let mut seen: HashSet<_> = self
            .entries
            .iter()
            .filter_map(|e| {
                if let LaunchTarget::ShellPath(path) = &e.target {
                    Some(path.to_string_lossy().to_lowercase())
                } else {
                    None
                }
            })
            .collect();
        for entry in previous.entries() {
            if let LaunchTarget::ShellPath(path) = &entry.target {
                if self.failed.iter().any(|root| path.starts_with(root))
                    && seen.insert(path.to_string_lossy().to_lowercase())
                {
                    self.entries.push(entry.clone());
                }
            }
        }
        self.entries
            .sort_by_cached_key(|entry| entry.name.to_lowercase());
    }
}
pub fn discover(roots: &[PathBuf], previous: Option<&Catalog>) -> io::Result<Scan> {
    if roots.is_empty() {
        return Err(io::Error::other(
            "无法确定应用目录；保留原有索引，可按 F5 重试",
        ));
    }
    let mut reader = ShortcutReader::new()?;
    let mut entries = Vec::new();
    let mut failed = Vec::new();
    let mut stack: Vec<_> = roots.iter().map(|p| (p.clone(), 0)).collect();
    let mut seen = HashSet::new();
    while let Some((directory, depth)) = stack.pop() {
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
                    stack.push((path, depth + 1));
                } else {
                    failed.push(path);
                }
                continue;
            }
            let Some(extension) = path.extension() else {
                continue;
            };
            if !(extension.eq_ignore_ascii_case("exe")
                || extension.eq_ignore_ascii_case("lnk") && reader.is_application(&path))
            {
                continue;
            }
            if !seen.insert(path.to_string_lossy().to_lowercase()) {
                continue;
            }
            let Some(name) = path.file_stem() else {
                continue;
            };
            entries.push(AppEntry::new(
                name.to_string_lossy().into_owned(),
                LaunchTarget::ShellPath(path),
            ));
        }
    }
    entries.sort_by_cached_key(|entry| entry.name.to_lowercase());
    let mut scan = Scan { entries, failed };
    if let Some(previous) = previous {
        scan.preserve_unreadable(previous);
    }
    let Scan { entries, failed } = scan;
    if !roots.is_empty() && roots.iter().all(|root| failed.contains(root)) && entries.is_empty() {
        return Err(io::Error::other("所有应用目录均无法读取；保留原有索引"));
    }
    Ok(Scan { entries, failed })
}

pub fn launch(hwnd: Hwnd, target: &LaunchTarget) -> io::Result<()> {
    let LaunchTarget::ShellPath(path) = target else {
        return Err(io::Error::other("此版本尚未支持 Store/UWP 应用"));
    };
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "入口已失效，请按 F5 刷新索引",
        ));
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
        Err(io::Error::other(format!(
            "打开失败（Windows 错误 {}），可按 F5 刷新",
            unsafe { GetLastError() }
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
