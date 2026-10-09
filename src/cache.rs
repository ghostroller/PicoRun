//! Bounded, versioned snapshot. Corruption is an ordinary cache miss, never an empty index.
use crate::{
    catalog::Catalog,
    model::{
        valid_registered_app, AppEntry, LaunchTarget, RegisteredApp, MAX_ALIAS_HEAP_BYTES,
        MAX_ALIAS_KEY_BYTES, MAX_REGISTERED_PATH_BYTES, MAX_REGISTERED_PATH_UNITS, MAX_SEARCH_KEYS,
    },
};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8; 8] = b"PRCA\x03\0\0\0";
const VERSION_TWO_MAGIC: &[u8; 8] = b"PRCA\x02\0\0\0";
const LEGACY_MAGIC: &[u8; 8] = b"PRCA\x01\0\0\0";
const LIMIT: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;
const SHELL_PATH: u8 = 0;
const APP_USER_MODEL_ID: u8 = 1;
const APP_PATH: u8 = 2;
// SDK minappmodel.h: 64-unit package family + '!' + 64-unit relative app ID.
// APPLICATION_USER_MODEL_ID_MAX_LENGTH includes an additional NUL terminator.
const MAX_APP_USER_MODEL_ID_UNITS: usize = 129;

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid PicoRun cache")
}
fn hash(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn put(data: &mut Vec<u8>, bytes: &[u8]) {
    data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    data.extend_from_slice(bytes);
}
fn take<'a>(data: &mut &'a [u8]) -> io::Result<&'a [u8]> {
    if data.len() < 4 {
        return Err(invalid());
    }
    let size = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
    *data = &data[4..];
    if size > data.len() {
        return Err(invalid());
    }
    let value = &data[..size];
    *data = &data[size..];
    Ok(value)
}
fn string(data: &mut &[u8]) -> io::Result<String> {
    String::from_utf8(take(data)?.to_vec()).map_err(|_| invalid())
}
fn valid_app_user_model_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_APP_USER_MODEL_ID_UNITS * 4
        && !id.as_bytes().contains(&0)
        && id.encode_utf16().count() <= MAX_APP_USER_MODEL_ID_UNITS
}
fn app_user_model_id(data: &mut &[u8]) -> io::Result<String> {
    let bytes = take(data)?;
    // Reject oversized values before allocating an owned string. UTF-8 requires at
    // most four bytes per character; the actual Windows limit is in UTF-16 units.
    if bytes.len() > MAX_APP_USER_MODEL_ID_UNITS * 4 {
        return Err(invalid());
    }
    let id = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    if !valid_app_user_model_id(id) {
        return Err(invalid());
    }
    Ok(id.to_owned())
}

#[cfg(windows)]
fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}
#[cfg(windows)]
fn decode_path(data: &[u8]) -> io::Result<PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    if !data.len().is_multiple_of(2) {
        return Err(invalid());
    }
    let wide: Vec<_> = data
        .chunks_exact(2)
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .collect();
    if wide.contains(&0) {
        return Err(invalid());
    }
    Ok(PathBuf::from(OsString::from_wide(&wide)))
}
#[cfg(not(windows))]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}
#[cfg(not(windows))]
fn decode_path(data: &[u8]) -> io::Result<PathBuf> {
    Ok(PathBuf::from(
        std::str::from_utf8(data).map_err(|_| invalid())?,
    ))
}

fn registered_app(data: &mut &[u8]) -> io::Result<Box<RegisteredApp>> {
    let bytes = take(data)?;
    #[cfg(windows)]
    let max_executable_bytes = MAX_REGISTERED_PATH_UNITS * 2;
    #[cfg(not(windows))]
    let max_executable_bytes = MAX_REGISTERED_PATH_BYTES;
    if bytes.len() > max_executable_bytes {
        return Err(invalid());
    }
    let executable = decode_path(bytes)?;
    let path_flag = *data.first().ok_or_else(invalid)?;
    *data = &data[1..];
    let path = match path_flag {
        0 => None,
        1 => {
            let bytes = take(data)?;
            if bytes.len() > MAX_REGISTERED_PATH_BYTES {
                return Err(invalid());
            }
            let path = std::str::from_utf8(bytes).map_err(|_| invalid())?;
            if path.is_empty()
                || path.as_bytes().contains(&0)
                || path.encode_utf16().count() > MAX_REGISTERED_PATH_UNITS
            {
                return Err(invalid());
            }
            Some(path.to_owned())
        }
        _ => return Err(invalid()),
    };
    let app = RegisteredApp { executable, path };
    if !valid_registered_app(&app) {
        return Err(invalid());
    }
    Ok(Box::new(app))
}
pub fn load(path: &Path) -> io::Result<Catalog> {
    if fs::metadata(path)?.len() > LIMIT as u64 {
        return Err(invalid());
    }
    // Bound the read even if another process changes the file after metadata().
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() < 20 || bytes.len() > LIMIT {
        return Err(invalid());
    }
    let version = if &bytes[..8] == LEGACY_MAGIC {
        1
    } else if &bytes[..8] == VERSION_TWO_MAGIC {
        2
    } else if &bytes[..8] == MAGIC {
        3
    } else {
        return Err(invalid());
    };
    let checksum = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    if hash(&bytes[16..]) != checksum {
        return Err(invalid());
    }
    let count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    if count > MAX_ENTRIES {
        return Err(invalid());
    }
    let mut data = &bytes[20..];
    let mut entries = Vec::with_capacity(count.min(data.len() / 16));
    for _ in 0..count {
        let name = string(&mut data)?;
        let target = if version == 1 {
            LaunchTarget::ShellPath(decode_path(take(&mut data)?)?)
        } else {
            let tag = *data.first().ok_or_else(invalid)?;
            data = &data[1..];
            match tag {
                SHELL_PATH => LaunchTarget::ShellPath(decode_path(take(&mut data)?)?),
                APP_USER_MODEL_ID => LaunchTarget::AppUserModelId(app_user_model_id(&mut data)?),
                APP_PATH if version == 3 => LaunchTarget::AppPath(registered_app(&mut data)?),
                _ => return Err(invalid()),
            }
        };
        // Version 3 records how many keys belong to the original display name.
        // This permits checking extra-key budgets before allocating, without
        // converting any cached candidate names to pinyin during loading.
        let base_count = if version == 3 {
            let count = *data.first().ok_or_else(invalid)? as usize;
            data = &data[1..];
            count
        } else {
            0
        };
        let key_count = *data.first().ok_or_else(invalid)? as usize;
        data = &data[1..];
        if name.is_empty()
            || key_count == 0
            || key_count > if version == 3 { MAX_SEARCH_KEYS } else { 3 }
            || (version == 3
                && (base_count == 0
                    || base_count > 3
                    || base_count > key_count
                    || key_count - base_count > 6))
        {
            return Err(invalid());
        }
        let mut keys = Vec::with_capacity(key_count);
        let mut extra_bytes = if version == 3 {
            keys.capacity().saturating_sub(base_count) * std::mem::size_of::<String>()
        } else {
            0
        };
        for index in 0..key_count {
            let bytes = take(&mut data)?;
            let extra = version == 3 && index >= base_count;
            if extra && (bytes.is_empty() || bytes.len() > MAX_ALIAS_KEY_BYTES) {
                return Err(invalid());
            }
            let key = std::str::from_utf8(bytes).map_err(|_| invalid())?;
            if extra && keys.iter().any(|previous| previous == key) {
                return Err(invalid());
            }
            let key = key.to_owned();
            if extra {
                extra_bytes += key.capacity();
                if extra_bytes > MAX_ALIAS_HEAP_BYTES {
                    return Err(invalid());
                }
            }
            keys.push(key);
        }
        entries.push(AppEntry { name, target, keys });
    }
    if !data.is_empty() {
        return Err(invalid());
    }
    Ok(Catalog::new(entries))
}

pub fn save(path: &Path, catalog: &Catalog) -> io::Result<()> {
    if catalog.entries().len() > MAX_ENTRIES {
        return Err(invalid());
    }
    let mut bytes = Vec::from(*MAGIC);
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&(catalog.entries().len() as u32).to_le_bytes());
    for entry in catalog.entries() {
        if entry.name.is_empty() || entry.keys.is_empty() {
            return Err(invalid());
        }
        let (base_count, _) = entry.alias_layout().ok_or_else(invalid)?;
        put(&mut bytes, entry.name.as_bytes());
        match &entry.target {
            LaunchTarget::ShellPath(path) => {
                bytes.push(SHELL_PATH);
                put(&mut bytes, &path_bytes(path));
            }
            LaunchTarget::AppUserModelId(id) => {
                if !valid_app_user_model_id(id) {
                    return Err(invalid());
                }
                bytes.push(APP_USER_MODEL_ID);
                put(&mut bytes, id.as_bytes());
            }
            LaunchTarget::AppPath(app) => {
                if !valid_registered_app(app) {
                    return Err(invalid());
                }
                bytes.push(APP_PATH);
                put(&mut bytes, &path_bytes(&app.executable));
                if let Some(path) = &app.path {
                    bytes.push(1);
                    put(&mut bytes, path.as_bytes());
                } else {
                    bytes.push(0);
                }
            }
        }
        bytes.push(base_count as u8);
        bytes.push(entry.keys.len() as u8);
        for key in &entry.keys {
            put(&mut bytes, key.as_bytes());
        }
        if bytes.len() > LIMIT {
            return Err(invalid());
        }
    }
    let checksum = hash(&bytes[16..]);
    bytes[8..16].copy_from_slice(&checksum.to_le_bytes());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    #[cfg(windows)]
    crate::platform::windows::replace_file(&temporary, path)?;
    #[cfg(not(windows))]
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestCache {
        directory: PathBuf,
        path: PathBuf,
    }
    impl TestCache {
        fn new() -> Self {
            static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
            let directory = std::env::temp_dir().join(format!(
                "picorun-cache-test-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&directory).unwrap();
            let path = directory.join("apps.bin");
            Self { directory, path }
        }
    }
    impl Drop for TestCache {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_file(self.path.with_extension("tmp"));
            let _ = fs::remove_dir(&self.directory);
        }
    }

    fn snapshot(magic: &[u8; 8], body: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::from(*magic);
        bytes.extend_from_slice(&hash(body).to_le_bytes());
        bytes.extend_from_slice(body);
        bytes
    }

    fn target_snapshot(tag: u8, target: &[u8]) -> Vec<u8> {
        let mut body = Vec::from(1_u32.to_le_bytes());
        put(&mut body, b"ChatGPT");
        body.push(tag);
        put(&mut body, target);
        body.push(1);
        body.push(1);
        put(&mut body, b"chatgpt");
        snapshot(MAGIC, &body)
    }

    #[test]
    fn roundtrip_replace_and_corruption() {
        let cache = TestCache::new();
        let mut shortcut = AppEntry::new(
            "微信 QQ",
            LaunchTarget::ShellPath(PathBuf::from("中文 空格/微信.lnk")),
        );
        assert!(shortcut.add_alias("qq.exe"));
        assert!(shortcut.add_alias("聊天工具"));
        let catalog = Catalog::new(vec![
            shortcut,
            AppEntry::new(
                "ChatGPT",
                LaunchTarget::AppUserModelId("Synthetic.ChatGPT_abc123!App".into()),
            ),
            AppEntry::new(
                "Microsoft 商店",
                LaunchTarget::AppUserModelId("Synthetic.Store_xyz789!App".into()),
            ),
        ]);
        save(&cache.path, &catalog).unwrap();
        save(&cache.path, &catalog).unwrap();
        let loaded = load(&cache.path).unwrap();
        assert_eq!(loaded.entries().len(), catalog.entries().len());
        for (actual, expected) in loaded.entries().iter().zip(catalog.entries()) {
            assert_eq!(actual.name, expected.name);
            assert_eq!(actual.target, expected.target);
            assert_eq!(actual.keys, expected.keys);
        }
        let mut bytes = fs::read(&cache.path).unwrap();
        assert_eq!(&bytes[..8], MAGIC);
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&cache.path, bytes).unwrap();
        assert!(load(&cache.path).is_err());
        fs::write(&cache.path, b"PRCA").unwrap();
        assert!(load(&cache.path).is_err());
    }

    #[test]
    fn legacy_paths_load_and_upgrade_on_save() {
        let cache = TestCache::new();
        let entry = AppEntry::new(
            "中文 Legacy App",
            LaunchTarget::ShellPath(PathBuf::from("中文 空格/Legacy.lnk")),
        );
        let mut body = Vec::from(1_u32.to_le_bytes());
        put(&mut body, entry.name.as_bytes());
        let LaunchTarget::ShellPath(path) = &entry.target else {
            unreachable!();
        };
        put(&mut body, &path_bytes(path));
        body.push(entry.keys.len() as u8);
        for key in &entry.keys {
            put(&mut body, key.as_bytes());
        }
        fs::write(&cache.path, snapshot(LEGACY_MAGIC, &body)).unwrap();
        let catalog = load(&cache.path).unwrap();
        assert_eq!(catalog.entries()[0].name, entry.name);
        assert_eq!(catalog.entries()[0].target, entry.target);
        assert_eq!(catalog.entries()[0].keys, entry.keys);
        save(&cache.path, &catalog).unwrap();
        assert_eq!(&fs::read(&cache.path).unwrap()[..8], MAGIC);
        assert_eq!(load(&cache.path).unwrap().entries()[0].target, entry.target);
    }

    #[test]
    fn rejects_unknown_target_tag_and_missing_target() {
        let cache = TestCache::new();
        fs::write(&cache.path, target_snapshot(255, b"Synthetic.App!App")).unwrap();
        assert_eq!(
            load(&cache.path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        let mut body = Vec::from(1_u32.to_le_bytes());
        put(&mut body, b"ChatGPT");
        fs::write(&cache.path, snapshot(MAGIC, &body)).unwrap();
        assert_eq!(
            load(&cache.path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn rejects_invalid_app_user_model_ids_on_load_and_save() {
        let cache = TestCache::new();
        let invalid_ids = [
            String::new(),
            "Synthetic.App\0!App".into(),
            "a".repeat(MAX_APP_USER_MODEL_ID_UNITS + 1),
            "🦀".repeat(MAX_APP_USER_MODEL_ID_UNITS / 2 + 1),
            "a".repeat(MAX_APP_USER_MODEL_ID_UNITS * 4 + 1),
        ];
        for id in invalid_ids {
            fs::write(
                &cache.path,
                target_snapshot(APP_USER_MODEL_ID, id.as_bytes()),
            )
            .unwrap();
            assert_eq!(
                load(&cache.path).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            let catalog =
                Catalog::new(vec![AppEntry::new("App", LaunchTarget::AppUserModelId(id))]);
            assert_eq!(
                save(&cache.path, &catalog).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        fs::write(&cache.path, target_snapshot(APP_USER_MODEL_ID, &[0xff])).unwrap();
        assert_eq!(
            load(&cache.path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn accepts_app_user_model_ids_at_utf16_limit() {
        let cache = TestCache::new();
        let entries = [
            "a".repeat(MAX_APP_USER_MODEL_ID_UNITS),
            "中".repeat(MAX_APP_USER_MODEL_ID_UNITS),
            format!("{}a", "🦀".repeat(MAX_APP_USER_MODEL_ID_UNITS / 2)),
        ]
        .into_iter()
        .map(|id| AppEntry::new("App", LaunchTarget::AppUserModelId(id)))
        .collect();
        let catalog = Catalog::new(entries);
        save(&cache.path, &catalog).unwrap();
        let loaded = load(&cache.path).unwrap();
        for (actual, expected) in loaded.entries().iter().zip(catalog.entries()) {
            assert_eq!(actual.target, expected.target);
        }
    }

    fn key_snapshot(magic: &[u8; 8], base_count: u8, keys: &[String]) -> Vec<u8> {
        let mut body = Vec::from(1_u32.to_le_bytes());
        put(&mut body, b"App");
        if magic != LEGACY_MAGIC {
            body.push(SHELL_PATH);
        }
        put(&mut body, &path_bytes(Path::new("demo/App.lnk")));
        if magic == MAGIC {
            body.push(base_count);
        }
        body.push(keys.len() as u8);
        for key in keys {
            put(&mut body, key.as_bytes());
        }
        snapshot(magic, &body)
    }

    #[test]
    fn version_two_mixed_targets_load_and_upgrade_on_save() {
        let cache = TestCache::new();
        let entries = vec![
            AppEntry::new("普通 App", LaunchTarget::ShellPath("demo/App.lnk".into())),
            AppEntry::new(
                "商店 Store",
                LaunchTarget::AppUserModelId("Synthetic.Store!App".into()),
            ),
        ];
        let mut body = Vec::from((entries.len() as u32).to_le_bytes());
        for entry in &entries {
            put(&mut body, entry.name.as_bytes());
            match &entry.target {
                LaunchTarget::ShellPath(path) => {
                    body.push(SHELL_PATH);
                    put(&mut body, &path_bytes(path));
                }
                LaunchTarget::AppUserModelId(id) => {
                    body.push(APP_USER_MODEL_ID);
                    put(&mut body, id.as_bytes());
                }
                LaunchTarget::AppPath(_) => unreachable!(),
            }
            body.push(entry.keys.len() as u8);
            for key in &entry.keys {
                put(&mut body, key.as_bytes());
            }
        }
        fs::write(&cache.path, snapshot(VERSION_TWO_MAGIC, &body)).unwrap();
        let loaded = load(&cache.path).unwrap();
        for (actual, expected) in loaded.entries().iter().zip(entries) {
            assert_eq!(actual.target, expected.target);
            assert_eq!(actual.keys, expected.keys);
        }
        save(&cache.path, &loaded).unwrap();
        assert_eq!(&fs::read(&cache.path).unwrap()[..8], MAGIC);
        assert_eq!(load(&cache.path).unwrap().entries().len(), 2);
    }

    #[test]
    fn cache_versions_reject_invalid_key_counts() {
        let cache = TestCache::new();
        for magic in [LEGACY_MAGIC, VERSION_TWO_MAGIC] {
            for count in [0, 4] {
                let keys = vec!["app".into(); count];
                fs::write(&cache.path, key_snapshot(magic, 1, &keys)).unwrap();
                assert!(load(&cache.path).is_err());
            }
        }
        for (base, count) in [(0, 1), (4, 4), (2, 1), (1, 0), (3, 10), (1, 8)] {
            let keys = (0..count)
                .map(|index| format!("key{index}"))
                .collect::<Vec<_>>();
            fs::write(&cache.path, key_snapshot(MAGIC, base, &keys)).unwrap();
            assert!(load(&cache.path).is_err());
        }
        for base in [1, 2, 3] {
            let keys = (0..base + 6)
                .map(|index| format!("key{index}"))
                .collect::<Vec<_>>();
            fs::write(&cache.path, key_snapshot(MAGIC, base, &keys)).unwrap();
            assert_eq!(
                load(&cache.path).unwrap().entries()[0].keys.len(),
                keys.len()
            );
        }
    }

    #[test]
    fn version_three_alias_keys_have_byte_and_capacity_limits() {
        let cache = TestCache::new();
        let invalid = [
            vec!["app".into(), "a".repeat(MAX_ALIAS_KEY_BYTES + 1)],
            vec!["app".into(), "😀".repeat(MAX_ALIAS_KEY_BYTES / 4 + 1)],
            vec!["app".into(), String::new()],
            vec!["app".into(), "app".into()],
            std::iter::once("app".into())
                .chain((0..5).map(|index| format!("{index}{}", "a".repeat(399))))
                .collect(),
        ];
        for keys in invalid {
            fs::write(&cache.path, key_snapshot(MAGIC, 1, &keys)).unwrap();
            assert_eq!(
                load(&cache.path).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        let keys = vec!["app".into(), "😀".repeat(MAX_ALIAS_KEY_BYTES / 4)];
        fs::write(&cache.path, key_snapshot(MAGIC, 1, &keys)).unwrap();
        assert_eq!(load(&cache.path).unwrap().entries()[0].keys, keys);

        // Existing display-name keys remain readable even if they exceed the new
        // limit for added aliases. No Unicode code point is shortened or split.
        let entry = AppEntry::new(
            "X".repeat(700),
            LaunchTarget::ShellPath("demo/App.lnk".into()),
        );
        let catalog = Catalog::new(vec![entry]);
        save(&cache.path, &catalog).unwrap();
        assert_eq!(load(&cache.path).unwrap().entries()[0].keys[0].len(), 700);
    }

    #[test]
    fn saving_rejects_noncanonical_or_over_capacity_aliases() {
        let cache = TestCache::new();
        let mut third_alias = AppEntry::new("App", LaunchTarget::ShellPath("demo/App.lnk".into()));
        third_alias
            .keys
            .extend(["one".into(), "two".into(), "three".into()]);
        assert_eq!(
            save(&cache.path, &Catalog::new(vec![third_alias]))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        let mut oversized = AppEntry::new("App", LaunchTarget::ShellPath("demo/App.lnk".into()));
        assert!(oversized.add_alias("alias"));
        oversized.keys[1].reserve(MAX_ALIAS_HEAP_BYTES);
        assert_eq!(
            save(&cache.path, &Catalog::new(vec![oversized]))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    fn registered_snapshot(
        magic: &[u8; 8],
        executable: &[u8],
        flag: u8,
        path: Option<&[u8]>,
    ) -> Vec<u8> {
        let mut body = Vec::from(1_u32.to_le_bytes());
        put(&mut body, b"App");
        body.push(APP_PATH);
        put(&mut body, executable);
        body.push(flag);
        if let Some(path) = path {
            put(&mut body, path);
        }
        body.push(1);
        body.push(1);
        put(&mut body, b"app");
        snapshot(magic, &body)
    }

    #[test]
    fn registered_targets_and_clickonce_roundtrip_with_aliases() {
        let cache = TestCache::new();
        let executable = std::env::current_dir().unwrap().join("demo/中文 App.exe");
        let mut entries: Vec<_> = [None, Some("C:\\支持目录;C:\\Extra Path".into())]
            .into_iter()
            .map(|path| {
                AppEntry::new(
                    "注册应用",
                    LaunchTarget::AppPath(Box::new(RegisteredApp {
                        executable: executable.clone(),
                        path,
                    })),
                )
            })
            .collect();
        for entry in &mut entries {
            assert!(entry.add_alias("registeredapp"));
        }
        entries.push(AppEntry::new(
            "ClickOnce",
            LaunchTarget::ShellPath("demo/ClickOnce.appref-ms".into()),
        ));
        let catalog = Catalog::new(entries);
        save(&cache.path, &catalog).unwrap();
        let loaded = load(&cache.path).unwrap();
        for (actual, expected) in loaded.entries().iter().zip(catalog.entries()) {
            assert_eq!(actual.target, expected.target);
            assert_eq!(actual.keys, expected.keys);
            assert_eq!(actual.name, expected.name);
        }
    }

    #[test]
    fn registered_targets_reject_invalid_path_data_before_allocation() {
        let cache = TestCache::new();
        let executable = std::env::current_dir().unwrap().join("demo/App.exe");
        let encoded = path_bytes(&executable);
        for (flag, path) in [
            (2, None),
            (1, Some(Vec::new())),
            (1, Some(b"path\0suffix".to_vec())),
            (1, Some(vec![0xff])),
            (1, Some(vec![b'a'; MAX_REGISTERED_PATH_BYTES + 1])),
            (
                1,
                Some("中".repeat(MAX_REGISTERED_PATH_UNITS + 1).into_bytes()),
            ),
            (
                1,
                Some("😀".repeat(MAX_REGISTERED_PATH_UNITS / 2 + 1).into_bytes()),
            ),
        ] {
            fs::write(
                &cache.path,
                registered_snapshot(MAGIC, &encoded, flag, path.as_deref()),
            )
            .unwrap();
            assert_eq!(
                load(&cache.path).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        for path in [
            PathBuf::from("relative.exe"),
            std::env::current_dir().unwrap().join("demo/App.dll"),
        ] {
            fs::write(
                &cache.path,
                registered_snapshot(MAGIC, &path_bytes(&path), 0, None),
            )
            .unwrap();
            assert!(load(&cache.path).is_err());
        }
        // The tag remains unknown in the older wire format even when all its
        // payload fields would be valid in version 3.
        fs::write(
            &cache.path,
            registered_snapshot(VERSION_TWO_MAGIC, &encoded, 0, None),
        )
        .unwrap();
        assert!(load(&cache.path).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn registered_targets_reject_invalid_windows_executable_encoding() {
        let cache = TestCache::new();
        let mut invalid = "C:\\App"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        invalid.extend(0xd800_u16.to_le_bytes());
        invalid.extend(".exe".encode_utf16().flat_map(u16::to_le_bytes));
        for bytes in [
            vec![0xff],
            vec![b'a'; MAX_REGISTERED_PATH_UNITS * 2 + 2],
            invalid,
        ] {
            fs::write(&cache.path, registered_snapshot(MAGIC, &bytes, 0, None)).unwrap();
            assert!(load(&cache.path).is_err());
        }
    }

    #[test]
    fn registered_path_utf16_boundary_is_accepted_on_load_and_save() {
        let cache = TestCache::new();
        let app = RegisteredApp {
            executable: std::env::current_dir().unwrap().join("demo/App.exe"),
            path: Some("中".repeat(MAX_REGISTERED_PATH_UNITS)),
        };
        let catalog = Catalog::new(vec![AppEntry::new(
            "App",
            LaunchTarget::AppPath(Box::new(app)),
        )]);
        save(&cache.path, &catalog).unwrap();
        assert_eq!(
            load(&cache.path).unwrap().entries()[0].target,
            catalog.entries()[0].target
        );
        let invalid = RegisteredApp {
            executable: "relative.exe".into(),
            path: None,
        };
        assert!(save(
            &cache.path,
            &Catalog::new(vec![AppEntry::new(
                "App",
                LaunchTarget::AppPath(Box::new(invalid))
            )])
        )
        .is_err());
    }
}
