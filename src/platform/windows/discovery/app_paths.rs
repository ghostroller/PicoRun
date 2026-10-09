//! Bounded startup/F5 discovery of registered desktop applications.
//! Registry handles and scratch buffers are released before search starts.
use super::path_key::PathKey;
use crate::{
    model::{
        valid_registered_app, AppEntry, LaunchTarget, RegisteredApp, MAX_REGISTERED_PATH_UNITS,
    },
    platform::windows::{
        ffi::{AllowSetForegroundWindow, Handle, Hwnd},
        wide,
    },
};
use std::{
    collections::HashSet,
    ffi::OsString,
    io,
    os::windows::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    ptr::{null, null_mut},
};

const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
const HKCU: Handle = -2147483647isize as Handle;
const HKLM: Handle = -2147483646isize as Handle;
const QUERY: u32 = 1;
const ENUMERATE: u32 = 8;
const VIEW_64: u32 = 0x100;
const VIEW_32: u32 = 0x200;
const REG_SZ: u32 = 1;
const REG_EXPAND_SZ: u32 = 2;
const NOT_FOUND: i32 = 2;
const PATH_NOT_FOUND: i32 = 3;
const MORE_DATA: i32 = 234;
const NO_MORE_ITEMS: i32 = 259;
const MAX_UNITS: usize = 32768;
const MAX_SUBKEYS: u32 = 20_000;
const BYTE_BUDGET: usize = 16 * 1024 * 1024;

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(
        key: Handle,
        subkey: *const u16,
        options: u32,
        access: u32,
        result: *mut Handle,
    ) -> i32;
    fn RegEnumKeyExW(
        key: Handle,
        index: u32,
        name: *mut u16,
        units: *mut u32,
        reserved: *mut u32,
        class: *mut u16,
        class_units: *mut u32,
        modified: *mut u64,
    ) -> i32;
    fn RegQueryValueExW(
        key: Handle,
        name: *const u16,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        size: *mut u32,
    ) -> i32;
    fn RegCloseKey(key: Handle) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn ExpandEnvironmentStringsW(source: *const u16, output: *mut u16, units: u32) -> u32;
}

struct Key(Handle);
impl Drop for Key {
    fn drop(&mut self) {
        // Only successful RegOpenKeyEx allocations are owned, never predefined hive handles.
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
fn checked(code: i32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code))
    }
}
fn open(hive: Handle, subkey: &str, access: u32) -> io::Result<Option<Key>> {
    let subkey = wide(subkey);
    let mut key = null_mut();
    // Input strings and the output slot outlive the synchronous registry call.
    let code = unsafe { RegOpenKeyExW(hive, subkey.as_ptr(), 0, access, &mut key) };
    if matches!(code, NOT_FOUND | PATH_NOT_FOUND) {
        return Ok(None);
    }
    checked(code)?;
    Ok(Some(Key(key)))
}

enum Value {
    Missing,
    Invalid,
    Text(String),
}
struct Reader {
    value: Vec<u16>,
    expanded: Vec<u16>,
}
impl Reader {
    fn new() -> Self {
        Self {
            value: vec![0; MAX_UNITS],
            expanded: Vec::new(),
        }
    }
    fn value(&mut self, key: &Key, name: Option<&str>) -> io::Result<Value> {
        let name = name.map(wide);
        let mut bytes = (self.value.len() * 2) as u32;
        let mut kind = 0;
        // RegQueryValueEx need not terminate strings. Decode only the returned byte count,
        // and reject embedded NULs, malformed UTF-16, unexpected types and oversized data.
        let code = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ref().map_or(null(), |name| name.as_ptr()),
                null_mut(),
                &mut kind,
                self.value.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if matches!(code, NOT_FOUND | PATH_NOT_FOUND) {
            return Ok(Value::Missing);
        }
        if code == MORE_DATA {
            return Ok(Value::Invalid);
        }
        checked(code)?;
        if !matches!(kind, REG_SZ | REG_EXPAND_SZ)
            || !bytes.is_multiple_of(2)
            || bytes as usize > self.value.len() * 2
        {
            return Ok(Value::Invalid);
        }
        let Some(text) = decode(&self.value[..bytes as usize / 2]) else {
            return Ok(Value::Invalid);
        };
        if kind != REG_EXPAND_SZ {
            return Ok(Value::Text(text));
        }
        let source = wide(&text);
        let units = unsafe { ExpandEnvironmentStringsW(source.as_ptr(), null_mut(), 0) };
        if units == 0 {
            return Err(io::Error::last_os_error());
        }
        if units as usize > MAX_UNITS {
            return Ok(Value::Invalid);
        }
        self.expanded.resize(units as usize, 0);
        let actual = unsafe {
            ExpandEnvironmentStringsW(source.as_ptr(), self.expanded.as_mut_ptr(), units)
        };
        if actual == 0 {
            return Err(io::Error::last_os_error());
        }
        if actual > units {
            return Ok(Value::Invalid);
        }
        Ok(decode(&self.expanded[..actual as usize]).map_or(Value::Invalid, Value::Text))
    }
}
fn decode(units: &[u16]) -> Option<String> {
    let length = units
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(units.len());
    if units[length..].iter().any(|unit| *unit != 0) {
        return None;
    }
    String::from_utf16(&units[..length]).ok()
}
fn executable(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    let value = if value.starts_with('"') {
        value.strip_prefix('"')?.strip_suffix('"')?
    } else {
        value
    };
    if value.contains(['"', '\0']) {
        return None;
    }
    let path = PathBuf::from(value);
    (path.is_absolute()
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe")))
    .then_some(path)
}
fn application_key(name: &str) -> bool {
    !name.contains(['\\', '/', '\0'])
        && Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        && Path::new(name)
            .file_stem()
            .is_some_and(|stem| !stem.is_empty())
}

/// Every configured hive/view must be readable before replacing the previous App Paths
/// snapshot. A missing base key is an empty source; malformed or stale registrations are
/// skipped. Any source I/O/enumeration failure lets the caller preserve previous entries.
pub(super) fn discover() -> io::Result<Vec<AppEntry>> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut reader = Reader::new();
    let mut bytes = 0usize;
    // Keep registrations with different executable/Path settings independently launchable.
    // Identical HKCU views and matching HKCU/HKLM registrations only need one snapshot entry.
    for (hive, view) in [
        (HKCU, VIEW_64),
        (HKLM, VIEW_64),
        (HKCU, VIEW_32),
        (HKLM, VIEW_32),
    ] {
        collect(
            hive,
            APP_PATHS,
            view,
            &mut reader,
            &mut entries,
            &mut seen,
            &mut bytes,
        )?;
    }
    Ok(entries)
}

type Identity = (String, PathKey, Option<String>);
fn collect(
    hive: Handle,
    base: &str,
    view: u32,
    reader: &mut Reader,
    entries: &mut Vec<AppEntry>,
    seen: &mut HashSet<Identity>,
    bytes: &mut usize,
) -> io::Result<()> {
    let Some(base) = open(hive, base, ENUMERATE | view)? else {
        return Ok(());
    };
    let mut name = [0u16; 256];
    for index in 0..=MAX_SUBKEYS {
        let mut units = name.len() as u32;
        let code = unsafe {
            RegEnumKeyExW(
                base.0,
                index,
                name.as_mut_ptr(),
                &mut units,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if code == NO_MORE_ITEMS {
            return Ok(());
        }
        checked(code)?;
        if index == MAX_SUBKEYS || units as usize > name.len() {
            return Err(io::Error::other("App Paths source exceeds scan limits"));
        }
        let Ok(name) = String::from_utf16(&name[..units as usize]) else {
            continue;
        };
        if !application_key(&name) {
            continue;
        }
        let Some(key) = open(base.0, &name, QUERY | view)? else {
            continue;
        };
        let Value::Text(value) = reader.value(&key, None)? else {
            continue;
        };
        let Some(executable) = executable(&value).filter(|path| path.is_file()) else {
            continue;
        };
        let path = match reader.value(&key, Some("Path"))? {
            Value::Missing => None,
            Value::Text(path) if path.is_empty() => None,
            Value::Text(path) => Some(path),
            Value::Invalid => continue,
        };
        let identity = (
            name.to_ascii_lowercase(),
            PathKey::new(&executable),
            path.clone(),
        );
        if seen.contains(&identity) {
            continue;
        }
        let Some(display_name) = Path::new(&name).file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        let app = RegisteredApp { executable, path };
        if !valid_registered_app(&app) {
            continue;
        }
        let executable_name = app
            .executable
            .file_stem()
            .and_then(|name| name.to_str())
            .map(str::to_owned);
        let mut entry = AppEntry::new(display_name, LaunchTarget::AppPath(Box::new(app)));
        if let Some(executable_name) = executable_name {
            entry.add_alias(&executable_name);
        }
        *bytes += std::mem::size_of::<AppEntry>()
            + std::mem::size_of::<Identity>()
            + entry.heap_bytes()
            + identity.0.capacity()
            + identity.1.owned_bytes()
            + identity.2.as_ref().map_or(0, String::capacity);
        if *bytes > BYTE_BUDGET {
            return Err(io::Error::other(
                "App Paths source exceeds scan memory budget",
            ));
        }
        seen.insert(identity);
        entries.push(entry);
    }
    unreachable!()
}

/// Open the exact registered executable through the Shell. A Path registration needs
/// a short-lived copy of PicoRun with its own environment: ShellExecuteEx has no
/// environment-block argument, and changing this process's PATH would race other work.
/// The helper initializes only COM, executes the absolute path, and exits. The saved
/// snapshot remains authoritative until F5; executable lookup never uses cwd or PATH.
pub(super) fn launch(hwnd: Hwnd, app: &RegisteredApp) -> io::Result<()> {
    if !valid_registered_app(app) || !app.executable.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            crate::i18n::Text::EntryMissing,
        ));
    }
    if super::super::requires_launch_isolation()? {
        return super::super::launch::detached_helper(&app.executable, app.path.as_deref());
    }
    let Some(prefix) = &app.path else {
        return super::launch_shell_path(hwnd, &app.executable);
    };
    let path = child_path(prefix, std::env::var_os("PATH"))?;
    let child = Command::new(std::env::current_exe()?)
        .arg("--app-path-helper")
        .arg(&app.executable)
        .env("PATH", path)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW: helper never allocates a console.
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    // Best effort only: a foreground launcher may delegate activation permission to its
    // helper. No window/controller borrow or registry handle survives the synchronous wait.
    unsafe {
        AllowSetForegroundWindow(child.id());
    }
    // The trusted helper emits one bounded static diagnostic (<1 KiB) and then exits;
    // waiting also collects and releases the helper's process handle on every result.
    let output = child.wait_with_output()?;
    if output.status.success() {
        Ok(())
    } else {
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        Err(io::Error::other(format!(
            "App Paths activation failed: {}",
            diagnostic.trim()
        )))
    }
}
fn child_path(prefix: &str, existing: Option<OsString>) -> io::Result<OsString> {
    let mut path = OsString::from(prefix);
    if let Some(existing) = existing.filter(|path| !path.is_empty()) {
        path.push(";");
        path.push(existing);
    }
    // The Windows environment-variable bound includes the terminating NUL. Existing
    // PATH may contain unpaired UTF-16 units; retain them exactly with OsString.
    if path.encode_wide().count() >= MAX_REGISTERED_PATH_UNITS {
        return Err(io::Error::other(
            "Registered application PATH exceeds the Windows limit",
        ));
    }
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_values_preserve_unicode_and_reject_embedded_nul_or_surrogates() {
        assert_eq!(decode(&[0x5fae, 0x4fe1, 0]), Some("微信".into()));
        assert_eq!(decode(&[65, 66]), Some("AB".into()));
        assert_eq!(decode(&[65, 0, 0]), Some("A".into()));
        assert_eq!(decode(&[65, 0, 66]), None);
        assert_eq!(decode(&[0xd800]), None);
        assert_eq!(decode(&[0xdc00]), None);
    }

    #[test]
    fn registration_default_accepts_only_one_absolute_executable() {
        assert_eq!(
            executable(r#"  "C:\应用 软件\app.EXE"  "#),
            Some(r"C:\应用 软件\app.EXE".into())
        );
        for value in [
            r"C:\App.exe --argument",
            r#""C:\App.exe" --argument"#,
            r"app.exe",
            r"C:app.exe",
            r"\app.exe",
            r"C:\file.txt",
            r#"C:\bad"name.exe"#,
            "C:\\nul\0.exe",
        ] {
            assert!(executable(value).is_none(), "{value:?}");
        }
        for name in ["app.exe", "中文 App.EXE"] {
            assert!(application_key(name));
        }
        for name in [".exe", "app", "dir\\app.exe", "dir/app.exe", "app.exe\0"] {
            assert!(!application_key(name));
        }
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegCreateKeyExW(
            key: Handle,
            subkey: *const u16,
            reserved: u32,
            class: *mut u16,
            options: u32,
            access: u32,
            security: *const std::ffi::c_void,
            result: *mut Handle,
            disposition: *mut u32,
        ) -> i32;
        fn RegSetValueExW(
            key: Handle,
            name: *const u16,
            reserved: u32,
            kind: u32,
            data: *const u8,
            bytes: u32,
        ) -> i32;
        fn RegDeleteTreeW(key: Handle, subkey: *const u16) -> i32;
    }

    struct Fixture {
        registry: String,
        directory: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let token = format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let registry = format!(r"Software\PicoRun\Verification\AppPaths-{token}");
            assert!(open(HKCU, &registry, QUERY | VIEW_64).unwrap().is_none());
            let directory = std::env::temp_dir().join(format!("picorun-app-paths-{token}"));
            std::fs::create_dir_all(&directory).unwrap();
            Self {
                registry,
                directory,
            }
        }
        fn application(&self, name: &str) -> PathBuf {
            let path = self.directory.join(name);
            std::fs::write(&path, "synthetic; never executed").unwrap();
            path
        }
        fn write(
            &self,
            application: &str,
            name: Option<&str>,
            kind: u32,
            data: &[u16],
            bytes: usize,
        ) {
            let registry = format!(r"{}\{application}", self.registry);
            let mut key = null_mut();
            checked(unsafe {
                RegCreateKeyExW(
                    HKCU,
                    wide(registry).as_ptr(),
                    0,
                    null_mut(),
                    0,
                    QUERY | 2 | VIEW_64,
                    null(),
                    &mut key,
                    null_mut(),
                )
            })
            .unwrap();
            let key = Key(key);
            let name = name.map(wide);
            checked(unsafe {
                RegSetValueExW(
                    key.0,
                    name.as_ref().map_or(null(), |name| name.as_ptr()),
                    0,
                    kind,
                    data.as_ptr().cast(),
                    bytes as u32,
                )
            })
            .unwrap();
        }
        fn text(&self, application: &str, name: Option<&str>, kind: u32, value: &str) {
            let data = wide(value);
            self.write(application, name, kind, &data, data.len() * 2);
        }
        fn entries(&self, views: &[u32]) -> io::Result<Vec<AppEntry>> {
            let mut reader = Reader::new();
            let mut entries = Vec::new();
            let mut seen = HashSet::new();
            let mut bytes = 0;
            for view in views {
                collect(
                    HKCU,
                    &self.registry,
                    *view,
                    &mut reader,
                    &mut entries,
                    &mut seen,
                    &mut bytes,
                )?;
            }
            Ok(entries)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Tests only delete their own newly allocated nonproduction leaf.
            unsafe {
                RegDeleteTreeW(HKCU, wide(&self.registry).as_ptr());
            }
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn isolated_registry_accepts_quoted_paths_and_retains_registration_aliases() {
        let fixture = Fixture::new();
        let executable = fixture.application("目标 App.exe");
        fixture.text(
            "本地应用.exe",
            None,
            REG_SZ,
            &format!("  \"{}\"  ", executable.display()),
        );
        fixture.text(
            "本地应用.exe",
            Some("Path"),
            REG_SZ,
            r"C:\依赖 一;D:\dependencies",
        );
        let entries = fixture.entries(&[VIEW_64, VIEW_32]).unwrap();
        assert_eq!(entries.len(), 1); // HKCU App Paths is shared across views on supported Windows.
        assert_eq!(entries[0].name, "本地应用");
        assert!(entries[0].keys.iter().any(|key| key == "目标 app"));
        let LaunchTarget::AppPath(app) = &entries[0].target else {
            panic!("wrong source identity");
        };
        assert_eq!(app.executable, executable);
        assert_eq!(app.path.as_deref(), Some(r"C:\依赖 一;D:\dependencies"));
    }

    #[test]
    fn isolated_registry_skips_stale_arguments_invalid_types_and_bad_path_data() {
        let fixture = Fixture::new();
        let executable = fixture.application("real.exe");
        let value = executable.to_str().unwrap();
        fixture.text(
            "arguments.exe",
            None,
            REG_SZ,
            &format!("\"{value}\" --argument"),
        );
        fixture.text(
            "stale.exe",
            None,
            REG_SZ,
            fixture.directory.join("gone.exe").to_str().unwrap(),
        );
        fixture.text("relative.exe", None, REG_SZ, "real.exe");
        fixture.text("binary.exe", None, 3, value);
        fixture.text("bad-path.exe", None, REG_SZ, value);
        fixture.write("bad-path.exe", Some("Path"), REG_SZ, &[65, 0, 66, 0], 8);
        fixture.text("bad-surrogate.exe", None, REG_SZ, value);
        fixture.write("bad-surrogate.exe", Some("Path"), REG_SZ, &[0xd800, 0], 4);
        fixture.write("odd.exe", None, REG_SZ, &[65, 0], 3);
        fixture.write(
            "huge.exe",
            None,
            REG_SZ,
            &vec![65; MAX_UNITS + 1],
            (MAX_UNITS + 1) * 2,
        );
        fixture.text("valid.exe", None, REG_SZ, value);
        let entries = fixture.entries(&[VIEW_64]).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "valid");
    }

    #[test]
    fn expandable_values_and_registry_string_without_terminator_are_supported() {
        let fixture = Fixture::new();
        let executable = fixture.application("literal.exe");
        let data: Vec<u16> = executable.as_os_str().encode_wide().collect();
        fixture.write("literal.exe", None, REG_SZ, &data, data.len() * 2);
        // Read a well-known system executable only; this test never executes it or
        // changes the process environment while other tests might be running.
        let system = std::env::var_os("SystemRoot").unwrap();
        let expanded = PathBuf::from(system).join("System32").join("cmd.exe");
        assert!(expanded.is_file());
        fixture.text(
            "expanded.exe",
            None,
            REG_EXPAND_SZ,
            r"%SystemRoot%\System32\cmd.exe",
        );
        fixture.text(
            "expanded.exe",
            Some("Path"),
            REG_EXPAND_SZ,
            r"%SystemRoot%\System32",
        );
        let entries = fixture.entries(&[VIEW_64]).unwrap();
        assert_eq!(entries.len(), 2);
        let entry = entries
            .iter()
            .find(|entry| entry.name == "expanded")
            .unwrap();
        let LaunchTarget::AppPath(app) = &entry.target else {
            panic!("wrong target");
        };
        assert_eq!(app.executable, expanded);
        assert_eq!(app.path.as_deref(), expanded.parent().unwrap().to_str());
    }

    #[test]
    fn missing_source_is_empty_but_source_open_errors_are_not_silenced() {
        let fixture = Fixture::new();
        assert!(fixture.entries(&[VIEW_64]).unwrap().is_empty());
        let result = collect(
            null_mut(),
            &fixture.registry,
            VIEW_64,
            &mut Reader::new(),
            &mut Vec::new(),
            &mut HashSet::new(),
            &mut 0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn same_registration_in_different_sources_preserves_distinct_launch_environments() {
        let first = Fixture::new();
        let second = Fixture::new();
        let executable = first.application("actual.exe");
        for fixture in [&first, &second] {
            fixture.text("same.exe", None, REG_SZ, executable.to_str().unwrap());
        }
        first.text("same.exe", Some("Path"), REG_SZ, r"C:\First");
        second.text("same.exe", Some("Path"), REG_SZ, r"D:\Second");
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        let mut reader = Reader::new();
        let mut bytes = 0;
        for fixture in [&first, &second, &first] {
            collect(
                HKCU,
                &fixture.registry,
                VIEW_64,
                &mut reader,
                &mut entries,
                &mut seen,
                &mut bytes,
            )
            .unwrap();
        }
        assert_eq!(entries.len(), 2);
        let environments: Vec<_> = entries
            .iter()
            .map(|entry| match &entry.target {
                LaunchTarget::AppPath(app) => app.path.as_deref().unwrap(),
                _ => panic!("wrong source"),
            })
            .collect();
        assert_eq!(environments, [r"C:\First", r"D:\Second"]);
    }

    #[test]
    fn child_environment_is_bounded_and_keeps_parent_utf16_units() {
        use std::os::windows::ffi::OsStringExt;
        let existing = OsString::from_wide(&[0x43, 0x3a, 0x5c, 0xd800]);
        let prepared = child_path(r"D:\registered", Some(existing)).unwrap();
        let units: Vec<_> = prepared.encode_wide().collect();
        assert_eq!(units.last(), Some(&0xd800));
        assert_eq!(
            child_path(r"D:\registered", None).unwrap(),
            OsString::from(r"D:\registered")
        );
        assert!(child_path(&"x".repeat(MAX_REGISTERED_PATH_UNITS), None).is_err());
        assert!(child_path(
            "a",
            Some(OsString::from("x".repeat(MAX_REGISTERED_PATH_UNITS - 2)))
        )
        .is_err());
    }
}
