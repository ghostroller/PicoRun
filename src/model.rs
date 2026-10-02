use std::path::PathBuf;

/// Keep shortcut paths intact so the shell can preserve arguments and working directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    ShellPath(PathBuf),
    AppUserModelId(String),
}

#[derive(Debug)]
pub struct AppEntry {
    pub name: String,
    pub target: LaunchTarget,
    pub(crate) keys: Vec<String>,
}

impl AppEntry {
    /// Alias preparation belongs to indexing, never the keystroke search path.
    pub fn new(name: impl Into<String>, target: LaunchTarget) -> Self {
        let name = name.into();
        let normalized = normalize(&name);
        let aliases = crate::pinyin::aliases(&name);
        let mut keys = vec![normalized];
        for key in [aliases.full, aliases.initials] {
            if !key.is_empty() && !keys.contains(&key) {
                keys.push(key);
            }
        }
        Self { name, target, keys }
    }
}

pub(crate) fn normalize(text: &str) -> String {
    text.trim().to_lowercase()
}
