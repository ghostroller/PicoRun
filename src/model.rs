use std::path::PathBuf;

/// Keep shortcut paths intact so the shell can preserve arguments and working directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    ShellPath(PathBuf),
    AppUserModelId(String),
    AppPath(Box<RegisteredApp>),
}

/// App Paths entries launch the exact executable, with their optional private
/// search path passed only to the child process that performs Shell activation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegisteredApp {
    pub executable: PathBuf,
    pub path: Option<String>,
}

pub const MAX_REGISTERED_PATH_UNITS: usize = 32_767;
pub const MAX_REGISTERED_PATH_BYTES: usize = MAX_REGISTERED_PATH_UNITS * 4;

pub fn valid_registered_app(app: &RegisteredApp) -> bool {
    if !app.executable.is_absolute()
        || !app
            .executable
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return false;
    }
    #[cfg(windows)]
    let valid_executable = {
        use std::os::windows::ffi::OsStrExt;
        let mut count = 0;
        let valid_units = app.executable.as_os_str().encode_wide().all(|unit| {
            count += 1;
            unit != 0 && count <= MAX_REGISTERED_PATH_UNITS
        });
        valid_units
            && char::decode_utf16(app.executable.as_os_str().encode_wide())
                .all(|character| character.is_ok())
    };
    #[cfg(not(windows))]
    let valid_executable = app.executable.to_str().is_some_and(|path| {
        !path.as_bytes().contains(&0) && path.encode_utf16().count() <= MAX_REGISTERED_PATH_UNITS
    });
    valid_executable
        && app.path.as_ref().is_none_or(|path| {
            !path.is_empty()
                && path.len() <= MAX_REGISTERED_PATH_BYTES
                && !path.as_bytes().contains(&0)
                && path.encode_utf16().count() <= MAX_REGISTERED_PATH_UNITS
        })
}
pub(crate) const MAX_ALIAS_NAMES: usize = 2;
pub(crate) const MAX_ALIAS_KEY_BYTES: usize = 512;
pub(crate) const MAX_ALIAS_HEAP_BYTES: usize = 2048;
pub(crate) const MAX_SEARCH_KEYS: usize = 9;

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub target: LaunchTarget,
    pub(crate) keys: Vec<String>,
}

fn name_keys(name: &str) -> Vec<String> {
    let normalized = normalize(name);
    let aliases = crate::pinyin::aliases(name);
    let mut keys = vec![normalized];
    for key in [aliases.full, aliases.initials] {
        if !key.is_empty() && !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

impl AppEntry {
    /// Owned heap capacity only; allocator metadata, dictionary pages and process costs are excluded.
    pub fn heap_bytes(&self) -> usize {
        self.name.capacity()
            + self.keys.capacity() * std::mem::size_of::<String>()
            + self.keys.iter().map(String::capacity).sum::<usize>()
            + match &self.target {
                LaunchTarget::ShellPath(path) => path.capacity(),
                LaunchTarget::AppUserModelId(id) => id.capacity(),
                LaunchTarget::AppPath(app) => {
                    std::mem::size_of::<RegisteredApp>()
                        + app.executable.capacity()
                        + app.path.as_ref().map_or(0, String::capacity)
                }
            }
    }
    /// Alias preparation belongs to indexing, never the keystroke search path.
    pub fn new(name: impl Into<String>, target: LaunchTarget) -> Self {
        let name = name.into();
        let keys = name_keys(&name);
        Self { name, target, keys }
    }

    /// Add at most two alternate names while indexing. Each contributes its normalized
    /// spelling, full pinyin and initials, with duplicate keys omitted. No raw aliases
    /// or counters remain resident; each appended group starts with its normalized name.
    /// A group that exceeds the byte or owned-capacity budget is skipped in full.
    pub fn add_alias(&mut self, alias: &str) -> bool {
        let alias = alias.trim();
        if alias.len() > MAX_ALIAS_KEY_BYTES {
            return false;
        }
        let normalized = normalize(alias);
        if normalized.is_empty()
            || normalized.len() > MAX_ALIAS_KEY_BYTES
            || self.keys.contains(&normalized)
        {
            return false;
        }
        let Some((base_count, alias_count)) = self.alias_layout() else {
            return false;
        };
        if alias_count == MAX_ALIAS_NAMES {
            return false;
        }
        // Derive the group from its normalized spelling, so grouping can be recovered
        // during later indexing without retaining a separate copy of the alias name.
        let aliases = crate::pinyin::aliases(&normalized);
        let mut added = vec![normalized];
        for key in [aliases.full, aliases.initials] {
            if key.len() > MAX_ALIAS_KEY_BYTES {
                return false;
            }
            if !key.is_empty() && !self.keys.contains(&key) && !added.contains(&key) {
                added.push(key);
            }
        }
        // Bound retained capacity, not just UTF-8 length, after pinyin construction.
        for key in &mut added {
            *key = std::mem::take(key).into_boxed_str().into_string();
        }
        let length = self.keys.len() + added.len();
        if length > MAX_SEARCH_KEYS {
            return false;
        }
        let string_bytes = self.keys[base_count..]
            .iter()
            .chain(&added)
            .map(String::capacity)
            .sum::<usize>();
        let mut keys = Vec::with_capacity(length);
        let alias_bytes = string_bytes
            + keys.capacity().saturating_sub(base_count) * std::mem::size_of::<String>();
        if alias_bytes > MAX_ALIAS_HEAP_BYTES {
            return false;
        }
        keys.extend(std::mem::take(&mut self.keys));
        keys.extend(added);
        self.keys = keys;
        true
    }

    /// Only used when indexing or saving, never during search. Recover canonical
    /// groups instead of adding a resident per-entry alias counter.
    pub(crate) fn alias_layout(&self) -> Option<(usize, usize)> {
        let base = name_keys(&self.name);
        let base_count = base.len();
        if !self.keys.starts_with(&base) || self.keys.len() > MAX_SEARCH_KEYS {
            return None;
        }
        let mut offset = base_count;
        let mut count = 0;
        while offset < self.keys.len() {
            count += 1;
            if count > MAX_ALIAS_NAMES {
                return None;
            }
            let normalized = &self.keys[offset];
            if normalized.is_empty()
                || normalized.len() > MAX_ALIAS_KEY_BYTES
                || normalize(normalized) != *normalized
                || self.keys[..offset].contains(normalized)
            {
                return None;
            }
            let aliases = crate::pinyin::aliases(normalized);
            offset += 1;
            for key in [aliases.full, aliases.initials] {
                if key.len() > MAX_ALIAS_KEY_BYTES {
                    return None;
                }
                if !key.is_empty() && !self.keys[..offset].contains(&key) {
                    if self.keys.get(offset) != Some(&key) {
                        return None;
                    }
                    offset += 1;
                }
            }
        }
        let owned = self.keys[base_count..]
            .iter()
            .map(String::capacity)
            .sum::<usize>()
            + self.keys.capacity().saturating_sub(base_count) * std::mem::size_of::<String>();
        (owned <= MAX_ALIAS_HEAP_BYTES).then_some((base_count, count))
    }
}

pub(crate) fn normalize(text: &str) -> String {
    text.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> AppEntry {
        AppEntry::new(name, LaunchTarget::ShellPath(PathBuf::from("demo/App.lnk")))
    }

    #[test]
    fn alternate_names_are_precomputed_and_deduplicated() {
        let mut entry = app("Editor");
        assert!(entry.add_alias("  微信 QQ  "));
        assert_eq!(entry.keys, ["editor", "微信 qq", "weixin qq", "wx qq"]);
        assert!(!entry.add_alias("微信 QQ"));
        assert!(!entry.add_alias("WEIXIN QQ"));
        assert!(entry.add_alias("editor.exe"));
        assert!(!entry.add_alias("thirdname"));
        assert_eq!(entry.alias_layout(), Some((1, 2)));
        assert_eq!(entry.keys.len(), 5);
    }

    #[test]
    fn alias_groups_share_keys_and_preserve_original_name_keys() {
        let mut entry = app(" 微信 ");
        let original = entry.keys.clone();
        assert!(!entry.add_alias("微信"));
        assert_eq!(entry.keys, original);
        assert!(entry.add_alias("腾讯QQ"));
        assert!(entry.add_alias("腾讯qq工具"));
        assert_eq!(entry.alias_layout(), Some((3, 2)));
        assert_eq!(entry.keys.len(), MAX_SEARCH_KEYS);
        assert_eq!(&entry.keys[..3], &original);
    }

    #[test]
    fn long_utf8_and_expanding_pinyin_are_skipped_without_partial_keys() {
        let mut entry = app("Editor");
        for alias in ["中".repeat(171), "重".repeat(171), "İ".repeat(256)] {
            let before = entry.keys.clone();
            assert!(!entry.add_alias(&alias));
            assert_eq!(entry.keys, before);
        }
        // This fits the normalized UTF-8 limit but full pinyin exceeds 512 bytes.
        assert!(!entry.add_alias(&"重".repeat(129)));
        assert_eq!(entry.keys, ["editor"]);
        let exact = "😀".repeat(MAX_ALIAS_KEY_BYTES / 4);
        assert!(entry.add_alias(&exact));
        assert_eq!(entry.keys[1].len(), MAX_ALIAS_KEY_BYTES);
        assert_eq!(entry.keys[1], exact);
        assert_eq!(entry.alias_layout(), Some((1, 1)));
    }

    #[test]
    fn owned_alias_capacity_has_a_per_entry_budget() {
        let mut entry = app("Editor");
        let first = "八".repeat(170);
        assert!(entry.add_alias(&first));
        assert_eq!(entry.alias_layout(), Some((1, 1)));
        let before = entry.keys.clone();
        assert!(!entry.add_alias(&"怕".repeat(170)));
        assert_eq!(entry.keys, before);
        assert_eq!(entry.alias_layout(), Some((1, 1)));
    }

    #[test]
    fn resident_entry_layout_does_not_gain_alias_metadata() {
        struct OriginalEntry {
            _name: String,
            _target: LaunchTarget,
            _keys: Vec<String>,
        }
        assert_eq!(
            std::mem::size_of::<AppEntry>(),
            std::mem::size_of::<OriginalEntry>()
        );
    }

    #[test]
    fn registered_app_validation_bounds_executable_and_private_path() {
        let executable = std::env::current_dir().unwrap().join("demo/App.EXE");
        let mut app = RegisteredApp {
            executable,
            path: None,
        };
        assert!(valid_registered_app(&app));
        app.path = Some("中".repeat(MAX_REGISTERED_PATH_UNITS));
        assert!(valid_registered_app(&app));
        for path in [
            String::new(),
            "a\0b".into(),
            "中".repeat(MAX_REGISTERED_PATH_UNITS + 1),
            "😀".repeat(MAX_REGISTERED_PATH_UNITS / 2 + 1),
        ] {
            app.path = Some(path);
            assert!(!valid_registered_app(&app));
        }
        app.path = None;
        app.executable = "relative.exe".into();
        assert!(!valid_registered_app(&app));
        app.executable = std::env::current_dir().unwrap().join("demo/App.dll");
        assert!(!valid_registered_app(&app));
    }

    #[cfg(windows)]
    #[test]
    fn registered_executable_rejects_unpaired_utf16_and_nul() {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        for unit in [0, 0xd800] {
            let mut path: Vec<_> = "C:\\App".encode_utf16().collect();
            path.push(unit);
            path.extend(".exe".encode_utf16());
            let app = RegisteredApp {
                executable: PathBuf::from(OsString::from_wide(&path)),
                path: None,
            };
            assert!(!valid_registered_app(&app));
        }
    }

    #[test]
    fn registered_app_owned_heap_includes_the_box_and_path() {
        let app = RegisteredApp {
            executable: std::env::current_dir().unwrap().join("demo/App.exe"),
            path: Some("C:\\App\\Support".into()),
        };
        let owned = std::mem::size_of::<RegisteredApp>()
            + app.executable.capacity()
            + app.path.as_ref().unwrap().capacity();
        let entry = AppEntry::new("App", LaunchTarget::AppPath(Box::new(app)));
        assert_eq!(
            entry.heap_bytes(),
            entry.name.capacity()
                + entry.keys.capacity() * std::mem::size_of::<String>()
                + entry.keys.iter().map(String::capacity).sum::<usize>()
                + owned
        );
    }
}
