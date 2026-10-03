//! Real registry/menu/restart verification in a fixed non-autorun HKCU namespace.
use super::super::{startup::Registration, Options};
use super::*;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetMenuState(menu: Handle, item: u32, flags: u32) -> u32;
}
#[link(name = "shell32")]
unsafe extern "system" {
    fn CommandLineToArgvW(line: *const u16, count: *mut i32) -> *mut *mut u16;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LocalFree(memory: Handle) -> Handle;
}
struct Cleanup(Registration);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = self.0.cleanup_probe();
    }
}
fn parsed_command(mut line: Vec<u16>) -> io::Result<Vec<OsString>> {
    line.push(0);
    unsafe {
        // The OS owns the argv allocation until LocalFree. Copy each NUL-terminated argument.
        let mut count = 0;
        let args = CommandLineToArgvW(line.as_ptr(), &mut count);
        if args.is_null() {
            return Err(io::Error::last_os_error());
        }
        let output = std::slice::from_raw_parts(args, count as usize)
            .iter()
            .map(|arg| {
                let mut length = 0;
                while *arg.add(length) != 0 {
                    length += 1;
                }
                OsString::from_wide(std::slice::from_raw_parts(*arg, length))
            })
            .collect();
        LocalFree(args.cast());
        Ok(output)
    }
}
fn menu(
    hwnd: Hwnd,
    pid: u32,
    registration: &Registration,
    checked: bool,
    toggle: bool,
    image: Option<&Path>,
    checks: &mut String,
) -> io::Result<()> {
    let popup = open_tray_menu(hwnd, pid)?;
    let handle = unsafe { SendMessageW(popup, 0x1e1, 0, 0) } as Handle;
    let flags = unsafe { GetMenuState(handle, tray::STARTUP, 0) };
    expect(
        flags != u32::MAX && (flags & 8 != 0) == checked,
        "native startup checkbox matches live registry",
        checks,
    )?;
    if let Some(path) = image {
        screenshot(popup, path)?;
    }
    if toggle {
        unsafe {
            PostMessageW(popup, 0x102, 's' as usize, 1);
        }
        let start = Instant::now();
        while registration.enabled()? == checked {
            if start.elapsed() > Duration::from_secs(3) {
                return Err(io::Error::other("startup popup toggle timed out"));
            }
            thread::sleep(Duration::from_millis(10));
        }
    } else {
        unsafe {
            SendMessageW(hwnd, 0x1f, 0, 0);
        }
    }
    barrier(hwnd);
    Ok(())
}
fn close(child: &mut Running, hwnd: Hwnd) -> io::Result<()> {
    unsafe {
        PostMessageW(hwnd, 0x10, 0, 0);
    }
    let start = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other("launcher exit failed"))
            };
        }
        if start.elapsed() > Duration::from_secs(3) {
            return Err(io::Error::other("launcher exit timed out"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
pub(super) fn run() -> io::Result<()> {
    super::super::window::enable_dpi();
    let root = std::env::current_dir()?.join("runtime/probe-startup");
    let data = root.join("数据 缓存");
    let source = root.join("应用 来源");
    fs::create_dir_all(&data)?;
    fs::create_dir_all(&source)?;
    let exe = std::env::current_exe()?.with_file_name("picorun.exe");
    let real = Registration::for_executable(&Options::default(), &exe)?;
    let real_before = real.read()?;
    fs::write(data.join("theme.txt"), "light\n")?;
    fs::write(data.join("english-input.txt"), "on\n")?;
    fs::write(data.join("icons.txt"), "off\n")?;
    let _ = fs::remove_file(data.join("apps-v1.bin"));
    unsafe {
        if CoInitializeEx(null_mut(), 2) < 0 {
            return Err(io::Error::other("probe COM init failed"));
        }
    }
    let shortcuts = (0..500).try_for_each(|index| {
        let title = if index == 0 {
            "重庆银行".into()
        } else {
            format!("Synthetic App {index:04}")
        };
        create_shortcut(
            &source.join(format!("{title}.lnk")),
            &std::env::current_exe()?,
            "",
            &root,
        )
    });
    unsafe {
        CoUninitialize();
    }
    shortcuts?;
    let token = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_millis()
    );
    let options = Options {
        hotkey: "Ctrl+Alt+F11".into(),
        data_dir: Some(data.clone()),
        sources: vec![source.clone()],
        hidden: true,
        startup_probe: Some(token.clone()),
        ..Options::default()
    };
    let cleanup = Cleanup(Registration::for_executable(&options, &exe)?);
    let registration = &cleanup.0;
    registration.set(false)?;
    let args = vec![
        "--hidden".into(),
        "--hotkey".into(),
        options.hotkey.clone().into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
        "--source".into(),
        source.as_os_str().to_owned(),
        "--startup-probe".into(),
        token.into(),
        "--measure-icons".into(),
    ];
    let mut checks = String::new();
    let mut memory = String::from("stage,private_bytes,working_set_bytes,peak_commit_bytes,peak_working_set_bytes,gdi_handles,user_handles,cpu_100ns\n");
    let mut timings = Vec::new();
    let mut stored_args = Vec::new();
    for run in 0..3 {
        let started = Instant::now();
        let mut child = if run == 2 {
            spawn(Path::new(&stored_args[0]), &stored_args[1..])?
        } else {
            spawn(&exe, &args)?
        };
        let (hwnd, edit) = wait_window(&mut child)?;
        timings.push(started.elapsed().as_secs_f64() * 1000.0);
        barrier(hwnd);
        expect(
            unsafe { IsWindowVisible(hwnd) } == 0 && tray_rect(hwnd).is_some(),
            "hidden startup creates tray without showing launcher",
            &mut checks,
        )?;
        sample(child.0.id(), &format!("run{run}_hidden"), &mut memory)?;
        if run == 0 {
            expect(
                registration.read()?.is_none(),
                "normal startup writes no registry value",
                &mut checks,
            )?;
            menu(
                hwnd,
                child.0.id(),
                registration,
                false,
                true,
                None,
                &mut checks,
            )?;
            expect(
                unsafe { IsWindowVisible(hwnd) } == 0,
                "enable while hidden keeps launcher hidden",
                &mut checks,
            )?;
            let line = registration
                .read()?
                .ok_or_else(|| io::Error::other("missing startup value"))?;
            expect(
                line.len() <= 260,
                "registered command is within Run length limit",
                &mut checks,
            )?;
            fs::write(
                root.join("registered-command.txt"),
                String::from_utf16_lossy(&line),
            )?;
            stored_args = parsed_command(line)?;
            let expected: Vec<OsString> = vec![
                std::path::absolute(&exe)?.into_os_string(),
                "--hidden".into(),
                "--hotkey".into(),
                options.hotkey.clone().into(),
                "--data-dir".into(),
                std::path::absolute(&data)?.into_os_string(),
                "--source".into(),
                std::path::absolute(&source)?.into_os_string(),
            ];
            expect(
                stored_args == expected,
                "stored command retains Unicode paths and hotkey, excludes verification flags",
                &mut checks,
            )?;
            menu(
                hwnd,
                child.0.id(),
                registration,
                true,
                false,
                Some(&root.join("startup-menu.bmp")),
                &mut checks,
            )?;
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
                set_control_text(edit, wide("cqyh").as_ptr());
                SendMessageW(hwnd, 0x8003, 0, 0);
            }
            menu(
                hwnd,
                child.0.id(),
                registration,
                true,
                true,
                None,
                &mut checks,
            )?;
            expect(
                ime_probe::text(edit) == "cqyh",
                "disable preserves query",
                &mut checks,
            )?;
            expect(
                fs::read_to_string(data.join("theme.txt"))? == "light\n"
                    && fs::read_to_string(data.join("english-input.txt"))? == "on\n"
                    && fs::read_to_string(data.join("icons.txt"))? == "off\n",
                "startup toggle preserves independent theme/input/icon settings",
                &mut checks,
            )?;
            // External changes must be visible on the next popup, without polling.
            registration.set(true)?;
            menu(
                hwnd,
                child.0.id(),
                registration,
                true,
                false,
                None,
                &mut checks,
            )?;
            registration.set(false)?;
            menu(
                hwnd,
                child.0.id(),
                registration,
                false,
                true,
                None,
                &mut checks,
            )?;
        } else if run == 1 {
            menu(
                hwnd,
                child.0.id(),
                registration,
                true,
                true,
                None,
                &mut checks,
            )?;
            expect(
                registration.read()?.is_none(),
                "restart reads registration and disable deletes only own value",
                &mut checks,
            )?;
            for _ in 0..20 {
                unsafe {
                    SendMessageW(hwnd, 0x111, tray::STARTUP as usize, 0);
                    SendMessageW(hwnd, 0x111, tray::STARTUP as usize, 0);
                }
            }
            expect(
                registration.read()?.is_none(),
                "repeated toggles leave no registered value",
                &mut checks,
            )?;
        } else {
            expect(
                cache::load(&data.join("apps-v1.bin"))?.entries().len() == 500,
                "executing stored command uses preserved source and data directory",
                &mut checks,
            )?;
        }
        unsafe {
            SendMessageW(hwnd, 0x6, 0, 0);
        }
        barrier(hwnd);
        let before = sample(child.0.id(), &format!("run{run}_idle_before"), &mut memory)?;
        thread::sleep(Duration::from_secs(1));
        let after = sample(child.0.id(), &format!("run{run}_idle_after"), &mut memory)?;
        expect(
            after.saturating_sub(before) <= 200_000,
            "hidden idle has no ongoing startup work",
            &mut checks,
        )?;
        close(&mut child, hwnd)?;
        if run == 0 {
            expect(
                registration.enabled()?,
                "registration remains enabled after process exits",
                &mut checks,
            )?;
        }
    }
    expect(
        real.read()? == real_before,
        "production Run value unchanged by verification",
        &mut checks,
    )?;
    registration.cleanup_probe()?;
    expect(
        registration.read()?.is_none(),
        "isolated registry leaf cleaned up",
        &mut checks,
    )?;
    fs::write(root.join("checks.txt"), &checks)?;
    fs::write(root.join("memory.csv"), memory)?;
    fs::write(root.join("startup-ms.txt"), format!("{timings:?}\n"))?;
    println!(
        "{checks}startup_ms={timings:?}\nEvidence: {}",
        root.display()
    );
    Ok(())
}
