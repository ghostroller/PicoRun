//! Opt-in per-user logon registration. No writes during normal startup, no timers or service.
use super::{ffi::Handle, wide, Options};
use crate::i18n::Text;
use std::{
    io,
    path::Path,
    ptr::{null, null_mut},
};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "PicoRun";
const HKCU: Handle = -2147483647isize as Handle;
const QUERY: u32 = 1;
const SET: u32 = 2;
const REG_SZ: u32 = 1;
const NOT_FOUND: i32 = 2;
const MAX_COMMAND: usize = 260;

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(
        key: Handle,
        subkey: *const u16,
        options: u32,
        access: u32,
        result: *mut Handle,
    ) -> i32;
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
    fn RegQueryValueExW(
        key: Handle,
        name: *const u16,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        size: *mut u32,
    ) -> i32;
    fn RegSetValueExW(
        key: Handle,
        name: *const u16,
        reserved: u32,
        kind: u32,
        data: *const u8,
        size: u32,
    ) -> i32;
    fn RegDeleteValueW(key: Handle, name: *const u16) -> i32;
    fn RegCloseKey(key: Handle) -> i32;
    fn RegDeleteKeyW(key: Handle, subkey: *const u16) -> i32;
}
struct Key(Handle);
impl Drop for Key {
    fn drop(&mut self) {
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

pub struct Registration {
    key: String,
    command: Vec<u16>, // Includes the final NUL. All arguments use Windows CRT quoting.
}
impl Registration {
    pub fn new(options: &Options) -> io::Result<Self> {
        Self::for_executable(options, &std::env::current_exe()?)
    }
    pub(super) fn for_executable(options: &Options, exe: &Path) -> io::Result<Self> {
        let key = if let Some(token) = &options.startup_probe {
            if token.is_empty()
                || token.len() > 40
                || !token.bytes().all(|b| b.is_ascii_digit() || b == b'-')
            {
                return Err(io::Error::other("invalid startup verification token"));
            }
            format!(r"Software\PicoRun\Verification\Startup-{token}")
        } else {
            RUN.into()
        };
        Ok(Self {
            key,
            command: command(exe, options)?,
        })
    }
    fn open(&self, access: u32) -> io::Result<Option<Key>> {
        let mut result = null_mut();
        // Strings and output handle outlive this synchronous call. Only HKCU is ever opened.
        let code = unsafe { RegOpenKeyExW(HKCU, wide(&self.key).as_ptr(), 0, access, &mut result) };
        if code == NOT_FOUND {
            return Ok(None);
        }
        checked(code)?;
        Ok(Some(Key(result)))
    }
    pub fn read(&self) -> io::Result<Option<Vec<u16>>> {
        let Some(key) = self.open(QUERY)? else {
            return Ok(None);
        };
        // Bound malformed/externally changed values. RegQueryValueEx need not return a NUL;
        // only initialized units inside the returned byte count are examined.
        let mut data = [0u16; 2048];
        let mut bytes = std::mem::size_of_val(&data) as u32;
        let mut kind = 0;
        let code = unsafe {
            RegQueryValueExW(
                key.0,
                wide(VALUE).as_ptr(),
                null_mut(),
                &mut kind,
                data.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if code == NOT_FOUND {
            return Ok(None);
        }
        checked(code)?;
        if kind != REG_SZ
            || !bytes.is_multiple_of(2)
            || bytes as usize > std::mem::size_of_val(&data)
        {
            return Err(io::Error::other(Text::StartupInvalid));
        }
        let units = bytes as usize / 2;
        let length = data[..units].iter().position(|u| *u == 0).unwrap_or(units);
        if data[length..units].iter().any(|u| *u != 0) {
            return Err(io::Error::other(Text::StartupExtra));
        }
        Ok(Some(data[..length].to_vec()))
    }
    pub fn enabled(&self) -> io::Result<bool> {
        Ok(self
            .read()?
            .is_some_and(|value| value == self.command[..self.command.len() - 1]))
    }
    pub fn set(&self, enabled: bool) -> io::Result<()> {
        if !enabled {
            let Some(key) = self.open(SET)? else {
                return Ok(());
            };
            let code = unsafe { RegDeleteValueW(key.0, wide(VALUE).as_ptr()) };
            return if code == NOT_FOUND {
                Ok(())
            } else {
                checked(code)
            };
        }
        if self.command.len() - 1 > MAX_COMMAND {
            return Err(io::Error::other(Text::StartupLong));
        }
        let mut result = null_mut();
        checked(unsafe {
            RegCreateKeyExW(
                HKCU,
                wide(&self.key).as_ptr(),
                0,
                null_mut(),
                0,
                SET,
                null(),
                &mut result,
                null_mut(),
            )
        })?;
        let key = Key(result);
        // REG_SZ includes its UTF-16 NUL. Key RAII closes the only owned registry handle.
        checked(unsafe {
            RegSetValueExW(
                key.0,
                wide(VALUE).as_ptr(),
                0,
                REG_SZ,
                self.command.as_ptr().cast(),
                (self.command.len() * 2) as u32,
            )
        })
    }
    pub(super) fn cleanup_probe(&self) -> io::Result<()> {
        // Verification can only remove its own non-Run leaf; never the production Run key.
        if !self
            .key
            .starts_with(r"Software\PicoRun\Verification\Startup-")
        {
            return Err(io::Error::other("not an isolated startup verification key"));
        }
        self.set(false)?;
        let code = unsafe { RegDeleteKeyW(HKCU, wide(&self.key).as_ptr()) };
        if code == NOT_FOUND {
            Ok(())
        } else {
            checked(code)
        }
    }
}
fn argument(output: &mut Vec<u16>, units: &[u16]) -> io::Result<()> {
    if units.contains(&0) {
        return Err(io::Error::other(Text::StartupNul));
    }
    if !output.is_empty() {
        output.push(32);
    }
    output.push(34);
    let mut slashes = 0;
    for &unit in units {
        if unit == 92 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            92,
            if unit == 34 { 2 * slashes + 1 } else { slashes },
        ));
        output.push(unit);
        slashes = 0;
    }
    output.extend(std::iter::repeat_n(92, 2 * slashes));
    output.push(34);
    Ok(())
}
fn command(exe: &Path, options: &Options) -> io::Result<Vec<u16>> {
    let mut output = Vec::new();
    let mut add = |value: &std::ffi::OsStr| {
        let text = wide(value);
        argument(&mut output, &text[..text.len() - 1])
    };
    add(std::path::absolute(exe)?.as_os_str())?;
    add("--hidden".as_ref())?;
    add("--hotkey".as_ref())?;
    add(options.hotkey.as_ref())?;
    if let Some(path) = &options.data_dir {
        add("--data-dir".as_ref())?;
        add(std::path::absolute(path)?.as_os_str())?;
    }
    for path in &options.sources {
        add("--source".as_ref())?;
        add(std::path::absolute(path)?.as_os_str())?;
    }
    // Temporary theme/icons overrides and verification flags must not become persistent options.
    output.push(0);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn CommandLineToArgvW(line: *const u16, count: *mut i32) -> *mut *mut u16;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: Handle) -> Handle;
    }
    fn parse(line: &[u16]) -> Vec<Vec<u16>> {
        unsafe {
            let mut count = 0;
            let args = CommandLineToArgvW(line.as_ptr(), &mut count);
            assert!(!args.is_null());
            let output = std::slice::from_raw_parts(args, count as usize)
                .iter()
                .map(|arg| {
                    let mut length = 0;
                    while *arg.add(length) != 0 {
                        length += 1;
                    }
                    std::slice::from_raw_parts(*arg, length).to_vec()
                })
                .collect();
            LocalFree(args.cast());
            output
        }
    }
    struct Cleanup(Registration);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.cleanup_probe();
        }
    }
    #[test]
    fn windows_arguments_round_trip_spaces_unicode_quotes_and_trailing_slashes() {
        let args = [
            r"D:\启动 器\PicoRun.exe",
            "",
            "with \"quoted\" text",
            r"C:\数据 目录\",
            r#"a\\\"b"#,
        ];
        let mut line = Vec::new();
        for arg in args {
            argument(&mut line, &arg.encode_utf16().collect::<Vec<_>>()).unwrap();
        }
        line.push(0);
        assert_eq!(
            parse(&line),
            args.map(|arg| arg.encode_utf16().collect::<Vec<_>>())
        );
        assert!(argument(&mut Vec::new(), &[0]).is_err());
    }
    #[test]
    fn registration_is_opt_in_idempotent_and_bounded_in_isolated_key() {
        let options = Options {
            startup_probe: Some(format!("{}-1", std::process::id())),
            ..Options::default()
        };
        let cleanup = Cleanup(Registration::new(&options).unwrap());
        let registration = &cleanup.0;
        registration.set(false).unwrap();
        assert!(!registration.enabled().unwrap());
        registration.set(true).unwrap();
        registration.set(true).unwrap();
        assert!(registration.enabled().unwrap());
        let longer = Registration {
            key: registration.key.clone(),
            command: vec![32; MAX_COMMAND + 2],
        };
        assert!(longer.set(true).is_err());
        assert!(registration.enabled().unwrap());
        let mut boundary = Registration {
            key: registration.key.clone(),
            command: vec![32; MAX_COMMAND],
        };
        boundary.command.push(0);
        boundary.set(true).unwrap();
        assert_eq!(registration.read().unwrap().unwrap().len(), MAX_COMMAND);
        assert!(!registration.enabled().unwrap());
        registration.set(true).unwrap();
        // Unregister must leave unrelated values inside the same key intact.
        let key = registration.open(SET | QUERY).unwrap().unwrap();
        let other = wide("UnrelatedVerificationValue");
        let value = wide("keep me");
        unsafe {
            checked(RegSetValueExW(
                key.0,
                other.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            ))
            .unwrap();
        }
        registration.set(false).unwrap();
        registration.set(false).unwrap();
        assert!(!registration.enabled().unwrap());
        let mut bytes = 0;
        unsafe {
            checked(RegQueryValueExW(
                key.0,
                other.as_ptr(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut bytes,
            ))
            .unwrap();
        }
        assert_eq!(bytes as usize, value.len() * 2);
        drop(key);
        registration.cleanup_probe().unwrap();
    }
    #[test]
    fn registry_reader_bounds_malformed_and_unterminated_values() {
        let options = Options {
            startup_probe: Some(format!("{}-2", std::process::id())),
            ..Options::default()
        };
        let cleanup = Cleanup(Registration::new(&options).unwrap());
        let registration = &cleanup.0;
        registration.set(true).unwrap();
        let key = registration.open(SET).unwrap().unwrap();
        let write = |kind, data: &[u16], bytes| unsafe {
            checked(RegSetValueExW(
                key.0,
                wide(VALUE).as_ptr(),
                0,
                kind,
                data.as_ptr().cast(),
                bytes,
            ))
            .unwrap();
        };
        let command = &registration.command[..registration.command.len() - 1];
        write(REG_SZ, command, (command.len() * 2) as u32);
        assert!(registration.enabled().unwrap()); // The API does not promise a terminator.
        write(3, &[65, 0], 4);
        assert!(registration.read().is_err());
        write(REG_SZ, &[65, 0], 3);
        assert!(registration.read().is_err());
        write(REG_SZ, &[65, 0, 66, 0], 8);
        assert!(registration.read().is_err());
        write(REG_SZ, &[65; 2049], 4098);
        assert_eq!(registration.read().unwrap_err().raw_os_error(), Some(234));
        drop(key);
        registration.cleanup_probe().unwrap();
    }
    #[test]
    fn startup_retains_runtime_paths_and_hotkey_but_not_temporary_overrides() {
        let options = Options {
            hotkey: "Ctrl+Alt+F11".into(),
            data_dir: Some("runtime/测试 数据".into()),
            sources: vec!["runtime/应用 来源".into()],
            icons: Some(true),
            theme: Some(crate::theme::ThemeMode::Light),
            measure_icons: true,
            ..Options::default()
        };
        let line = command(Path::new(r"D:\Pico Run\picorun.exe"), &options).unwrap();
        let parsed = parse(&line);
        let text: Vec<_> = parsed
            .iter()
            .map(|arg| String::from_utf16(arg).unwrap())
            .collect();
        assert_eq!(
            &text[..4],
            &[
                r"D:\Pico Run\picorun.exe",
                "--hidden",
                "--hotkey",
                "Ctrl+Alt+F11"
            ]
        );
        assert_eq!(text[4], "--data-dir");
        assert!(Path::new(&text[5]).is_absolute());
        assert_eq!(text[6], "--source");
        assert!(Path::new(&text[7]).is_absolute());
        assert_eq!(text.len(), 8);
    }
}
