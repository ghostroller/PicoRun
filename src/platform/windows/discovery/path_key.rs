use std::{os::windows::ffi::OsStrExt, path::Path};

/// Scan-only lexical path identity. Keep UTF-16, including unpaired surrogates, intact.
/// ASCII case and separators cover ordinary overlapping sources; non-ASCII case variants
/// remain separate because Unicode lowercase does not describe Windows file identity.
#[derive(Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct PathKey(Vec<u16>);
impl PathKey {
    pub fn new(path: &Path) -> Self {
        Self(
            path.as_os_str()
                .encode_wide()
                .map(|unit| match unit {
                    0x41..=0x5a => unit + 0x20,
                    0x2f => 0x5c,
                    _ => unit,
                })
                .collect(),
        )
    }
    pub fn owned_bytes(&self) -> usize {
        self.0.capacity() * std::mem::size_of::<u16>()
    }
    pub fn is_within(&self, directory: &Self) -> bool {
        self.0.starts_with(&directory.0)
            && (self.0.len() == directory.0.len()
                || directory.0.last() == Some(&0x5c)
                || self.0.get(directory.0.len()) == Some(&0x5c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};

    #[test]
    fn ascii_case_and_separators_match_without_unicode_folding() {
        assert_eq!(
            PathKey::new(Path::new("C:/Apps/APP.exe")),
            PathKey::new(Path::new("c:\\apps\\app.EXE"))
        );
        for (first, second) in [
            ("C:\\İ.exe", "C:\\i\u{0307}.exe"),
            ("C:\\K.exe", "C:\\K.exe"),
            ("C:\\É.exe", "C:\\é.exe"),
        ] {
            assert_ne!(
                PathKey::new(Path::new(first)),
                PathKey::new(Path::new(second))
            );
        }
    }

    #[test]
    fn invalid_utf16_units_remain_distinct() {
        let first = PathBuf::from(OsString::from_wide(&[0x43, 0x3a, 0x5c, 0xd800]));
        let second = PathBuf::from(OsString::from_wide(&[0x43, 0x3a, 0x5c, 0xd801]));
        assert_eq!(first.to_string_lossy(), second.to_string_lossy());
        assert_ne!(PathKey::new(&first), PathKey::new(&second));
    }

    #[test]
    fn failed_directory_membership_respects_component_boundaries() {
        let directory = PathKey::new(Path::new("C:\\APPS"));
        assert!(PathKey::new(Path::new("c:/apps/app.lnk")).is_within(&directory));
        assert!(PathKey::new(Path::new("c:/apps")).is_within(&directory));
        assert!(!PathKey::new(Path::new("c:/apps-other/app.lnk")).is_within(&directory));
        assert!(PathKey::new(Path::new("c:/app.lnk")).is_within(&PathKey::new(Path::new("C:\\"))));
    }
}
