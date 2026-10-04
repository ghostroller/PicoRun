use super::replace_file;
use crate::{i18n::Language, theme::ThemeMode};
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

fn read(path: &Path) -> io::Result<String> {
    // Bound malformed settings independently of file size.
    let mut text = String::new();
    fs::File::open(path)?.take(65).read_to_string(&mut text)?;
    if text.len() > 64 {
        return Err(io::Error::other("setting is too long"));
    }
    Ok(text)
}

pub fn load(path: &Path) -> ThemeMode {
    read(path)
        .ok()
        .and_then(|s| ThemeMode::parse(&s))
        .unwrap_or_default()
}

pub fn load_language(path: &Path) -> Language {
    read(path)
        .ok()
        .and_then(|text| Language::parse(&text))
        .unwrap_or_default()
}
pub fn save_language(path: &Path, language: Language) -> io::Result<()> {
    write(path, language.name())
}

pub fn save(path: &Path, mode: ThemeMode) -> io::Result<()> {
    write(path, mode.name())
}

pub fn load_english(path: &Path) -> bool {
    load_toggle(path)
}

pub fn load_toggle(path: &Path) -> bool {
    read(path).is_ok_and(|text| text.trim() == "on")
}

pub fn save_english(path: &Path, enabled: bool) -> io::Result<()> {
    save_toggle(path, enabled)
}

pub fn save_toggle(path: &Path, enabled: bool) -> io::Result<()> {
    write(path, if enabled { "on" } else { "off" })
}

fn write(path: &Path, value: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, format!("{value}\n"))?;
    replace_file(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_persists_and_invalid_values_fall_back_without_changing_other_settings() {
        let directory =
            std::env::temp_dir().join(format!("picorun-language-test-{}", std::process::id()));
        let path = directory.join("language.txt");
        let theme = directory.join("theme.txt");
        let english = directory.join("english-input.txt");
        assert_eq!(load_language(&path), Language::Chinese);
        save(&theme, ThemeMode::Light).unwrap();
        save_english(&english, true).unwrap();
        for language in [Language::English, Language::Chinese, Language::English] {
            save_language(&path, language).unwrap();
            assert_eq!(load_language(&path), language);
        }
        for invalid in [b"fr".as_slice(), &[255], &[b' '; 4096]] {
            fs::write(&path, invalid).unwrap();
            assert_eq!(load_language(&path), Language::Chinese);
        }
        assert_eq!(load(&theme), ThemeMode::Light);
        assert!(load_english(&english));
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn icons_are_opt_in_and_independent_of_english_and_theme() {
        let directory =
            std::env::temp_dir().join(format!("picorun-icons-test-{}", std::process::id()));
        let icons = directory.join("icons.txt");
        let english = directory.join("english-input.txt");
        let theme = directory.join("theme.txt");
        assert!(!load_toggle(&icons));
        save_english(&english, true).unwrap();
        save(&theme, ThemeMode::Light).unwrap();
        save_toggle(&icons, true).unwrap();
        assert!(load_toggle(&icons));
        save_toggle(&icons, false).unwrap();
        assert!(!load_toggle(&icons));
        assert!(load_english(&english));
        assert_eq!(load(&theme), ThemeMode::Light);
        for invalid in [b"true".as_slice(), &[255], &[b' '; 4096]] {
            fs::write(&icons, invalid).unwrap();
            assert!(!load_toggle(&icons));
        }
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn saved_palette_survives_replacement_and_invalid_settings_fall_back() {
        let directory =
            std::env::temp_dir().join(format!("picorun-theme-test-{}", std::process::id()));
        let path = directory.join("theme.txt");
        assert_eq!(load(&path), ThemeMode::Dark);
        save(&path, ThemeMode::Light).unwrap();
        assert_eq!(load(&path), ThemeMode::Light);
        save(&path, ThemeMode::Dark).unwrap();
        assert_eq!(load(&path), ThemeMode::Dark);
        for invalid in [b"system".as_slice(), &[255], &[b' '; 4096]] {
            fs::write(&path, invalid).unwrap();
            assert_eq!(load(&path), ThemeMode::Dark);
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn english_input_is_opt_in_and_persists_without_changing_palette() {
        let directory =
            std::env::temp_dir().join(format!("picorun-input-test-{}", std::process::id()));
        let input = directory.join("english-input.txt");
        let palette = directory.join("theme.txt");
        assert!(!load_english(&input));
        save(&palette, ThemeMode::Light).unwrap();
        save_english(&input, true).unwrap();
        assert!(load_english(&input));
        assert_eq!(load(&palette), ThemeMode::Light);
        save_english(&input, false).unwrap();
        assert!(!load_english(&input));
        for invalid in [b"true".as_slice(), &[255], &[b' '; 4096]] {
            fs::write(&input, invalid).unwrap();
            assert!(!load_english(&input));
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
