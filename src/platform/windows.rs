//! Native system boundary; no discovery or Shell calls occur on the search path.
use crate::i18n::Text;
mod discovery;
pub mod ffi;
pub(crate) mod icons;
mod ime;
mod input_language;
mod instance;
mod launch;
mod settings;
mod startup;
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
/// Use native separators only at filesystem Shell boundaries. Mixed separators
/// triggered Shell teardown faults on the tested Windows build. This preserves
/// UTF-16 and the saved entry; it does not resolve links or inspect the target.
pub(crate) fn shell_file_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .map(|unit| {
            if unit == u16::from(b'/') {
                u16::from(b'\\')
            } else {
                unit
            }
        })
        .chain(Some(0))
        .collect()
}
pub fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    // Buffers outlive the synchronous call; replacement preserves a complete old/new snapshot.
    if unsafe { MoveFileExW(wide(source).as_ptr(), wide(destination).as_ptr(), 1 | 8) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
/// Load the persisted UI language before startup errors or --help are displayed.
pub fn prepare_language(data_dir: Option<&Path>) -> io::Result<()> {
    let data = match data_dir {
        Some(path) => path.to_path_buf(),
        None => discovery::data_directory()?,
    };
    crate::i18n::set(settings::load_language(&data.join("language.txt")));
    Ok(())
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
                        return Err(io::Error::other(Text::HotkeyFunction));
                    }
                    key = Some(0x6f + n);
                }
                _ => return Err(io::Error::other(Text::HotkeyFormat)),
            }
        }
        if modifiers == 0 || key.is_none() {
            return Err(io::Error::other(Text::HotkeyMissing));
        }
        Ok(Self {
            modifiers: modifiers | 0x4000,
            key: key.unwrap(),
        })
    }
}
pub(super) fn requires_launch_isolation() -> io::Result<bool> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleWindow() -> Hwnd;
    }
    // This borrowed window handle is only a console-attachment indicator. No UI
    // state borrow crosses the call, and it is never closed or manipulated here.
    Ok(launch::in_job()? || !unsafe { GetConsoleWindow() }.is_null())
}

/// An isolated one-shot helper accepts only application file entries. It never
/// starts the launcher UI, scans a catalog, or registers a global hotkey.
pub fn shell_launch_helper(entry: &Path) -> io::Result<()> {
    // The helper runs immediately so a terminal dying during process creation
    // cannot leave a permanently suspended orphan. Refuse Shell activation if
    // the OS nevertheless associated this helper with a caller job.
    if launch::in_job()? {
        return Err(io::Error::from_raw_os_error(5));
    }
    if !entry.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("exe")
            || extension.eq_ignore_ascii_case("lnk")
            || extension.eq_ignore_ascii_case("appref-ms")
    }) {
        return Err(io::Error::from_raw_os_error(87));
    }
    clear_child_standard_handles()?;
    if unsafe { CoInitializeEx(std::ptr::null_mut(), 2) } < 0 {
        return Err(io::Error::other(crate::i18n::Text::ShellCom));
    }
    // The helper has verified it is outside a job before activating the entry.
    // Call the Shell directly here; do not recursively create another helper.
    let result = discovery::launch_shell_path_direct(std::ptr::null_mut(), entry);
    unsafe { CoUninitialize() };
    result
}

pub fn packaged_activation_helper(id: &str) -> io::Result<()> {
    if launch::in_job()? {
        return Err(io::Error::from_raw_os_error(5));
    }
    clear_child_standard_handles()?;
    if unsafe { CoInitializeEx(std::ptr::null_mut(), 2) } < 0 {
        return Err(io::Error::other(crate::i18n::Text::ShellCom));
    }
    let result = discovery::activate_packaged_direct(id);
    unsafe { CoUninitialize() };
    result
}
fn clear_child_standard_handles() -> io::Result<()> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(kind: u32) -> Handle;
        fn SetHandleInformation(handle: Handle, mask: u32, flags: u32) -> i32;
    }
    // Do not pass a terminal or helper error pipe to the launched application.
    // A borrowed standard handle is never closed here.
    for kind in [-10i32, -11, -12] {
        let handle = unsafe { GetStdHandle(kind as u32) };
        if !handle.is_null()
            && handle as isize != -1
            && unsafe { SetHandleInformation(handle, 1, 0) } == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
/// Private, one-shot Shell process used only when an App Paths entry supplies PATH.
/// It creates no launcher window, catalog, icon worker, or single-instance handle.
pub fn app_path_helper(executable: &std::path::Path) -> io::Result<()> {
    let app = crate::model::RegisteredApp {
        executable: executable.to_owned(),
        path: None,
    };
    if !crate::model::valid_registered_app(&app) {
        return Err(io::Error::other("invalid registered executable"));
    }
    clear_child_standard_handles()?;
    if unsafe { CoInitializeEx(std::ptr::null_mut(), 2) } < 0 {
        return Err(io::Error::other(crate::i18n::Text::ShellCom));
    }
    let result = discovery::launch_shell_path(std::ptr::null_mut(), executable);
    unsafe {
        CoUninitialize();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_file_paths_preserve_utf16_and_keep_general_strings_unchanged() {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
        let raw = OsString::from_wide(&[68, 58, 47, 0xd800, 47, 97]);
        let path = PathBuf::from(raw.clone());
        assert_eq!(shell_file_path(&path), [68, 58, 92, 0xd800, 92, 97, 0]);
        assert_eq!(wide(&raw), [68, 58, 47, 0xd800, 47, 97, 0]);
        assert_eq!(path.as_os_str(), raw);
        assert_eq!(
            shell_file_path(Path::new(r"\\server\share/app.lnk")),
            wide(r"\\server\share\app.lnk")
        );
    }
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
