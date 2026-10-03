//! Two real processes: an original native Edit and the launcher. No user apps are opened.
use super::*;
use std::cell::Cell;

const CLASS: &str = "PicoRun.Verification.InputSource.v1";
thread_local! {
    static EDIT: Cell<Hwnd> = const { Cell::new(null_mut()) };
}
unsafe extern "system" fn source_proc(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        0x8001 | 0x312 => {
            ShowWindow(hwnd, 5);
            SetForegroundWindow(hwnd);
            SetFocus(EDIT.get());
            return 0;
        }
        0x8002 => {
            ActivateKeyboardLayout(lp as Handle, 0);
            return 0;
        }
        0x8003 => {
            let context = ImmGetContext(EDIT.get());
            if !context.is_null() {
                ImmSetConversionStatus(context, lp as u32, 0);
                ImmSetOpenStatus(context, wp as i32);
                ImmReleaseContext(EDIT.get(), context);
            }
            return 0;
        }
        0x10 => {
            DestroyWindow(hwnd);
            return 0;
        }
        0x2 => {
            UnregisterHotKey(hwnd, 1);
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
pub(super) fn source() -> io::Result<()> {
    unsafe {
        let instance = GetModuleHandleW(null());
        let name = wide(CLASS);
        let class = WndClass {
            size: std::mem::size_of::<WndClass>() as u32,
            style: 0,
            proc: Some(source_proc),
            class_extra: 0,
            window_extra: 0,
            instance,
            icon: null_mut(),
            cursor: LoadCursorW(null_mut(), 32512usize as *const u16),
            background: 6usize as Handle,
            menu: null(),
            name: name.as_ptr(),
            small_icon: null_mut(),
        };
        if RegisterClassExW(&class) == 0 {
            return Err(io::Error::last_os_error());
        }
        let hwnd = CreateWindowExW(
            0,
            name.as_ptr(),
            wide("PicoRun input restoration verification").as_ptr(),
            0x00cf0000,
            50,
            50,
            520,
            180,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        if hwnd.is_null() {
            return Err(io::Error::last_os_error());
        }
        let edit = CreateWindowExW(
            0,
            wide("Edit").as_ptr(),
            wide("").as_ptr(),
            0x50010080,
            15,
            20,
            460,
            40,
            hwnd,
            null_mut(),
            instance,
            null_mut(),
        );
        if edit.is_null() {
            DestroyWindow(hwnd);
            return Err(io::Error::last_os_error());
        }
        EDIT.set(edit);
        if RegisterHotKey(hwnd, 1, 0x4003, 0x79) == 0 {
            DestroyWindow(hwnd);
            return Err(io::Error::other(
                "controlled source Ctrl+Alt+F10 is occupied",
            ));
        }
        SendMessageW(hwnd, 0x8001, 0, 0);
        let mut msg = Message::default();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
fn wait_source(child: &mut Running) -> io::Result<(Hwnd, Hwnd)> {
    let start = Instant::now();
    loop {
        let hwnd = unsafe { FindWindowW(wide(CLASS).as_ptr(), null()) };
        let mut pid = 0;
        let edit = unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
            FindWindowExW(hwnd, null_mut(), wide("Edit").as_ptr(), null())
        };
        if pid == child.0.id() && !edit.is_null() {
            return Ok((hwnd, edit));
        }
        if child.0.try_wait()?.is_some() || start.elapsed() > Duration::from_secs(10) {
            return Err(io::Error::other("controlled source window not created"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
fn layout(hwnd: Hwnd) -> Handle {
    let mut pid = 0;
    unsafe { GetKeyboardLayout(GetWindowThreadProcessId(hwnd, &mut pid)) }
}
#[derive(Debug, PartialEq, Eq)]
struct InputState {
    layout: usize,
    open: isize,
    conversion: isize,
    sentence: isize,
}
impl InputState {
    fn same_typing_mode(&self, other: &Self) -> bool {
        // Chinese/English and full/half width. TSF can normalize its auxiliary
        // CHARCODE and sentence flags on layout activation; record the raw values too.
        self.layout == other.layout
            && self.open == other.open
            && (self.open == 0 || self.conversion & 9 == other.conversion & 9)
    }
}
fn input_state(hwnd: Hwnd, edit: Hwnd) -> InputState {
    unsafe {
        let ime = ImmGetDefaultIMEWnd(edit);
        InputState {
            layout: layout(hwnd) as usize,
            open: SendMessageW(ime, 0x283, 5, 0),
            conversion: SendMessageW(ime, 0x283, 1, 0),
            sentence: SendMessageW(ime, 0x283, 3, 0),
        }
    }
}
fn focus_source(hwnd: Hwnd, chinese: Handle, open: bool) -> io::Result<()> {
    return_source(hwnd)?;
    unsafe {
        SendMessageW(hwnd, 0x8002, 0, chinese as isize);
        SendMessageW(hwnd, 0x8003, usize::from(open), 1);
    }
    thread::sleep(Duration::from_millis(150));
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err(io::Error::other(
            "controlled source is not foreground; keyboard test aborted",
        ));
    }
    Ok(())
}
fn return_source(hwnd: Hwnd) -> io::Result<()> {
    unsafe { SendMessageW(hwnd, 0x8001, 0, 0) };
    if unsafe { GetForegroundWindow() } != hwnd {
        // The background driver cannot steal focus. A real registered hotkey grants
        // our controlled source the same activation permission as the launcher.
        unsafe {
            keybd_event(0x11, 0, 0, 0);
            keybd_event(0x12, 0, 0, 0);
            keybd_event(0x79, 0, 0, 0);
            keybd_event(0x79, 0, 2, 0);
            keybd_event(0x12, 0, 2, 0);
            keybd_event(0x11, 0, 2, 0);
        }
        thread::sleep(Duration::from_millis(150));
    }
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err(io::Error::other("controlled source lost foreground"));
    }
    Ok(())
}
fn invoke(hwnd: Hwnd, edit: Hwnd, checks: &mut String) -> io::Result<()> {
    actual_hotkey();
    let start = Instant::now();
    while unsafe { GetForegroundWindow() } != hwnd && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    expect(
        unsafe { GetForegroundWindow() } == hwnd,
        &format!(
            "real hotkey focuses launcher (foreground={:p}, own_visible={})",
            unsafe { GetForegroundWindow() },
            unsafe { IsWindowVisible(hwnd) }
        ),
        checks,
    )?;
    expect(
        layout(hwnd) as usize & 0x3ff == 9 || input_state(hwnd, edit).open == 0,
        "input session uses English mode",
        checks,
    )?;
    ime_probe::type_keys(hwnd, "weixin")?;
    expect(
        ime_probe::text(edit) == "weixin",
        "real keys form literal English query",
        checks,
    )
}
fn key(hwnd: Hwnd, value: u8) -> io::Result<()> {
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err(io::Error::other("keyboard target lost foreground"));
    }
    unsafe {
        keybd_event(value, 0, 0, 0);
        keybd_event(value, 0, 2, 0);
    }
    thread::sleep(Duration::from_millis(150));
    Ok(())
}
pub(super) fn run() -> io::Result<()> {
    let root = std::env::current_dir()?.join("runtime/probe-input-session");
    fs::create_dir_all(&root)?;
    let data = root.join("data");
    let source_dir = root.join("source");
    fs::create_dir_all(&data)?;
    fs::create_dir_all(&source_dir)?;
    let probe = std::env::current_exe()?;
    let exe = probe.with_file_name("picorun.exe");
    let marker = root.join("child.txt");
    unsafe {
        if CoInitializeEx(null_mut(), 2) < 0 {
            return Err(io::Error::other("probe COM initialization failed"));
        }
    }
    let shortcut = create_shortcut(
        &source_dir.join("微信.lnk"),
        &probe,
        &format!(
            "--controlled-child \"{}\" \"参数 with spaces\" 0",
            marker.display()
        ),
        &root,
    );
    unsafe { CoUninitialize() };
    shortcut?;
    super::super::settings::save_english(&data.join("english-input.txt"), true)?;
    let mut original = spawn(&probe, &["--input-source".into()])?;
    let (source, source_edit) = wait_source(&mut original)?;
    let mut layouts = [null_mut(); 64];
    let n = unsafe { GetKeyboardLayoutList(64, layouts.as_mut_ptr()) }.clamp(0, 64) as usize;
    let chinese = layouts[..n]
        .iter()
        .copied()
        .find(|p| *p as usize & 0xffff == 0x804)
        .ok_or_else(|| {
            io::Error::other("installed Chinese IME required; none installed by probe")
        })?;
    let args = [
        "--hidden".into(),
        "--hotkey".into(),
        "Ctrl+Alt+F11".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
        "--source".into(),
        source_dir.as_os_str().to_owned(),
    ];
    let mut child = spawn(&exe, &args)?;
    let (hwnd, edit) = wait_window(&mut child)?;
    println!(
        "Controlled source PID={} launcher PID={}",
        original.0.id(),
        child.0.id()
    );
    let mut checks = String::new();
    let mut memory = String::from("stage,private_bytes,working_set_bytes,peak_commit_bytes,peak_working_set_bytes,gdi_handles,user_handles,cpu_100ns\n");
    sample(child.0.id(), "hidden", &mut memory)?;
    for open in [true, false] {
        for leave in ["Escape", "hotkey", "blur", "Enter", "disable"] {
            focus_source(source, chinese, open)?;
            let before = input_state(source, source_edit);
            expect(
                before.open == isize::from(open),
                "source IME accepts Chinese/English mode setup",
                &mut checks,
            )?;
            checks.push_str(&format!("BEFORE mode={open} leave={leave} {before:?}\n"));
            fs::write(root.join("checks-progress.txt"), &checks)?;
            invoke(hwnd, edit, &mut checks)?;
            // Repeated invocation must not replace the original mode with temporary English.
            unsafe { SendMessageW(hwnd, 0x8001, 0, 0) };
            expect(
                layout(hwnd) as usize & 0x3ff == 9 || input_state(hwnd, edit).open == 0,
                "repeated show remains English",
                &mut checks,
            )?;
            match leave {
                "Escape" => key(hwnd, 0x1b)?,
                "hotkey" => actual_hotkey(),
                "blur" => return_source(source)?,
                "Enter" => {
                    let _ = fs::remove_file(&marker);
                    unsafe { set_control_text(edit, wide("wx").as_ptr()) };
                    key(hwnd, 0x0d)?;
                    expect(
                        wait_marker(&marker)?.contains("参数 with spaces|0"),
                        "Enter opens controlled shortcut",
                        &mut checks,
                    )?;
                }
                "disable" => unsafe {
                    SendMessageW(hwnd, 0x111, tray::ENGLISH as usize, 0);
                },
                _ => unreachable!(),
            }
            barrier(hwnd);
            expect(
                layout(hwnd) as usize == before.layout,
                "launcher restores invoking window layout",
                &mut checks,
            )?;
            if leave == "disable" {
                let restored = input_state(hwnd, edit);
                expect(
                    restored.same_typing_mode(&before),
                    &format!("disabling restores mode in the active Edit (actual={restored:?})"),
                    &mut checks,
                )?;
            }
            return_source(source)?;
            thread::sleep(Duration::from_millis(100));
            let after = input_state(source, source_edit);
            if after != before {
                checks.push_str(&format!(
                    "OBSERVE source TSF flags before={before:?} after={after:?}\n"
                ));
            }
            expect(
                after.same_typing_mode(&before),
                &format!(
                    "source mode restored after {leave} (actual={:?})",
                    input_state(source, source_edit)
                ),
                &mut checks,
            )?;
            if leave == "disable" {
                unsafe {
                    SendMessageW(hwnd, 0x111, tray::ENGLISH as usize, 0);
                };
            }
            fs::write(root.join("checks-progress.txt"), &checks)?;
        }
        unsafe {
            set_control_text(source_edit, wide("").as_ptr());
        }
        ime_probe::type_keys(source, "weixin")?;
        key(source, 0x20)?;
        expect(
            ime_probe::text(source_edit) == if open { "微信" } else { "weixin " },
            &format!(
                "original window really types {} after returning (actual={:?})",
                if open { "Chinese" } else { "English" },
                ime_probe::text(source_edit)
            ),
            &mut checks,
        )?;
    }
    sample(child.0.id(), "after_sessions", &mut memory)?;
    focus_source(source, chinese, true)?;
    let before = input_state(source, source_edit);
    invoke(hwnd, edit, &mut checks)?;
    unsafe {
        SendMessageW(hwnd, 0x111, tray::EXIT as usize, 0);
    };
    expect(
        child.0.wait()?.success(),
        "normal exit with active English session",
        &mut checks,
    )?;
    return_source(source)?;
    thread::sleep(Duration::from_millis(100));
    expect(
        input_state(source, source_edit).same_typing_mode(&before),
        "normal exit restores source mode",
        &mut checks,
    )?;
    unsafe {
        SendMessageW(source, 0x10, 0, 0);
    };
    expect(
        original.0.wait()?.success(),
        "controlled source exits normally",
        &mut checks,
    )?;
    fs::write(root.join("checks.txt"), &checks)?;
    fs::write(root.join("memory.csv"), &memory)?;
    println!(
        "Input session verification: {} checks; evidence {}",
        checks.lines().filter(|l| l.starts_with("PASS")).count(),
        root.display()
    );
    Ok(())
}
