//! Native system boundary; no discovery or Shell calls occur on the search path.
mod discovery;
pub mod ffi;
mod ime;
mod input_language;
mod settings;
mod tray;
pub mod verification;
mod window;
use ffi::*;
use std::{io, path::Path, ptr::null_mut};

pub const DEFAULT_HOTKEY: &str = "Alt+Space";
pub use window::{run, Options};

pub(crate) fn wide(value: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
pub fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    // Buffers outlive the synchronous call; replacement preserves a complete old/new snapshot.
    if unsafe { MoveFileExW(wide(source).as_ptr(), wide(destination).as_ptr(), 1 | 8) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
pub fn error_box(message: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(message).as_ptr(),
            wide("PicoRun").as_ptr(),
            0x10,
        );
    }
}
pub fn attach_console() {
    unsafe {
        AttachConsole(u32::MAX);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub modifiers: u32,
    pub key: u32,
}
impl Hotkey {
    pub fn parse(text: &str) -> io::Result<Self> {
        let mut modifiers = 0;
        let mut key = None;
        for part in text.split('+').map(str::trim) {
            let upper = part.to_ascii_uppercase();
            match upper.as_str() {
                "ALT" => modifiers |= 1,
                "CTRL" | "CONTROL" => modifiers |= 2,
                "SHIFT" => modifiers |= 4,
                "WIN" => modifiers |= 8,
                "SPACE" if key.is_none() => key = Some(0x20),
                value
                    if value.len() == 1
                        && value.as_bytes()[0].is_ascii_alphanumeric()
                        && key.is_none() =>
                {
                    key = Some(u32::from(value.as_bytes()[0]))
                }
                value if value.starts_with('F') && key.is_none() => {
                    let n = value[1..].parse::<u32>().unwrap_or(0);
                    if !(1..=24).contains(&n) {
                        return Err(io::Error::other("热键功能键应为 F1–F24"));
                    }
                    key = Some(0x6f + n);
                }
                _ => return Err(io::Error::other("热键格式例如 Alt+Space 或 Ctrl+Alt+P")),
            }
        }
        if modifiers == 0 || key.is_none() {
            return Err(io::Error::other("热键需要修饰键和一个按键"));
        }
        Ok(Self {
            modifiers: modifiers | 0x4000,
            key: key.unwrap(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hotkey_validation() {
        assert_eq!(
            Hotkey::parse("Alt+Space").unwrap(),
            Hotkey {
                modifiers: 0x4001,
                key: 32
            }
        );
        assert_eq!(Hotkey::parse("Ctrl+Alt+p").unwrap().key, 80);
        for invalid in ["", "Space", "Alt", "Alt+P+Q", "Ctrl+F25", "Ctrl+😀"] {
            assert!(Hotkey::parse(invalid).is_err());
        }
    }
}
