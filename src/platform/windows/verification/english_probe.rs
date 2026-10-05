//! Own-window layout, native menu and persistence regressions.
use super::*;

#[link(name = "user32")]
unsafe extern "system" {
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn WindowFromPoint(point: Point) -> Hwnd;
    fn mouse_event(flags: u32, x: u32, y: u32, data: u32, extra: usize);
}

fn alternate() -> io::Result<Handle> {
    let mut layouts = [null_mut(); 64];
    let count = unsafe { GetKeyboardLayoutList(64, layouts.as_mut_ptr()) }.clamp(0, 64) as usize;
    if !layouts[..count].iter().any(|p| *p as usize & 0x3ff == 9) {
        return Err(io::Error::other("English layout required for layout probe"));
    }
    layouts[..count]
        .iter()
        .copied()
        .find(|p| *p as usize & 0x3ff != 9)
        .ok_or_else(|| io::Error::other("non-English layout required for restore probe"))
}
fn layout(hwnd: Hwnd) -> Handle {
    let mut pid = 0;
    unsafe { GetKeyboardLayout(GetWindowThreadProcessId(hwnd, &mut pid)) }
}
fn choose(hwnd: Hwnd, value: Handle) {
    unsafe {
        SendMessageW(hwnd, 0x50, 0, value as isize);
    }
}
fn hide(hwnd: Hwnd) {
    unsafe {
        SendMessageW(hwnd, 0x6, 0, 0);
    }
}
fn show(hwnd: Hwnd, pid: u32, prior: &mut Handle, checks: &mut String) -> io::Result<()> {
    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
        if GetForegroundWindow() != hwnd {
            // A direct test message does not transfer foreground permission. End
            // the rejected invocation, then activate only our own parent header.
            // The header is outside both Edit and results, so it cannot type or launch.
            hide(hwnd);
            ShowWindow(hwnd, 4); // SW_SHOWNOACTIVATE: the click grants real activation.
            let edit = FindWindowExW(hwnd, null_mut(), wide("Edit").as_ptr(), null());
            let mut actual_pid = 0;
            let thread = GetWindowThreadProcessId(hwnd, &mut actual_pid);
            let mut edit_bounds = Rect::default();
            let mut client = Rect::default();
            let mut previous = Point::default();
            if thread == 0
                || actual_pid != pid
                || edit.is_null()
                || GetWindowRect(edit, &mut edit_bounds) == 0
                || GetClientRect(hwnd, &mut client) == 0
                || GetCursorPos(&mut previous) == 0
            {
                return Err(io::Error::other("English probe lost its controlled window"));
            }
            let mut point = Point {
                x: client.right / 2,
                y: 0,
            };
            if ClientToScreen(hwnd, &mut point) == 0 {
                return Err(io::Error::last_os_error());
            }
            point.y = edit_bounds.bottom + 2;
            if WindowFromPoint(point) != hwnd || SetCursorPos(point.x, point.y) == 0 {
                return Err(io::Error::other("English probe header is obstructed"));
            }
            // Recheck the target after moving; never inject into another app.
            let clicked = WindowFromPoint(point) == hwnd;
            if clicked {
                mouse_event(2, 0, 0, 0, 0);
                mouse_event(4, 0, 0, 0, 0);
            }
            let started = Instant::now();
            while clicked
                && GetForegroundWindow() != hwnd
                && started.elapsed() < Duration::from_secs(3)
            {
                thread::sleep(Duration::from_millis(10));
            }
            SetCursorPos(previous.x, previous.y);
            if !clicked || GetForegroundWindow() != hwnd {
                return Err(io::Error::other(
                    "English probe could not focus its controlled window",
                ));
            }
            // Keep focus on the parent until the next SHOW starts a new input
            // session. Any Edit focus session from activation must end first.
            SendMessageW(hwnd, 0x6, 1, 0); // WA_ACTIVE, handled by DefWindowProc.
            let mut info = GuiThreadInfo {
                size: std::mem::size_of::<GuiThreadInfo>() as u32,
                ..Default::default()
            };
            if GetGUIThreadInfo(thread, &mut info) == 0 || info.focus != hwnd {
                return Err(io::Error::other(
                    "English probe parent focus precondition failed",
                ));
            }
            // The failed invocation already restored and ended its backup. This
            // new invocation must be checked against its actual own-window layout.
            let retry_prior = prior_layout(hwnd);
            checks.push_str(&format!(
                "OBSERVE foreground retry prior_before={:#x} prior_retry={:#x}\n",
                *prior as usize, retry_prior as usize
            ));
            *prior = retry_prior;
            SendMessageW(hwnd, 0x8001, 0, 0);
        }
    }
    expect(
        unsafe { GetForegroundWindow() } == hwnd,
        "English input invocation has the controlled window in foreground",
        checks,
    )
}
fn toggle(hwnd: Hwnd) {
    unsafe {
        SendMessageW(hwnd, 0x111, tray::ENGLISH as usize, 0);
    }
}
fn is_english(hwnd: Hwnd) -> bool {
    if layout(hwnd) as usize & 0x3ff == 9 {
        return true;
    }
    unsafe {
        let edit = FindWindowExW(hwnd, null_mut(), wide("Edit").as_ptr(), null());
        SendMessageW(ImmGetDefaultIMEWnd(edit), 0x283, 5, 0) == 0
    }
}
fn prior_layout(hwnd: Hwnd) -> Handle {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() || foreground == hwnd {
        layout(hwnd)
    } else {
        layout(foreground)
    }
}

pub(super) fn features(
    hwnd: Hwnd,
    edit: Hwnd,
    pid: u32,
    input: &Path,
    root: &Path,
    keyboard: bool,
    checks: &mut String,
) -> io::Result<()> {
    let original = layout(hwnd);
    let other = alternate()?;
    let probe_layout = unsafe { GetKeyboardLayout(0) };
    hide(hwnd);
    choose(hwnd, other);
    let mut prior = prior_layout(hwnd);
    show(hwnd, pid, &mut prior, checks)?;
    expect(
        layout(hwnd) == other,
        "default off preserves non-English layout",
        checks,
    )?;
    hide(hwnd);
    let menu = open_tray_menu(hwnd, pid)?;
    unsafe {
        PostMessageW(menu, 0x102, 'e' as usize, 1);
    }
    // File replacement happens after the popup loop returns. Wait without injecting more keys.
    let start = Instant::now();
    while !super::super::settings::load_english(input) {
        if start.elapsed() > Duration::from_secs(3) {
            return Err(io::Error::other("native English toggle was not saved"));
        }
        thread::sleep(Duration::from_millis(10));
    }
    barrier(hwnd);
    expect(
        layout(hwnd) == other,
        "enabling while hidden leaves layout until show",
        checks,
    )?;
    prior = prior_layout(hwnd);
    show(hwnd, pid, &mut prior, checks)?;
    expect(
        is_english(hwnd),
        "show starts in English input mode",
        checks,
    )?;
    let menu = open_tray_menu(hwnd, pid)?;
    screenshot(menu, &root.join("english-menu.bmp"))?;
    unsafe {
        SendMessageW(hwnd, 0x1f, 0, 0);
    }
    barrier(hwnd);
    // A menu may end the focus session; a new show backs up its actual invoker again.
    if !is_english(hwnd) {
        prior = prior_layout(hwnd);
    }
    show(hwnd, pid, &mut prior, checks)?;
    unsafe {
        set_control_text(edit, wide("cqyh").as_ptr());
    }
    toggle(hwnd);
    expect(
        layout(hwnd) == prior && ime_probe::text(edit) == "cqyh",
        "disable restores prior layout and preserves committed query",
        checks,
    )?;
    expect(
        !super::super::settings::load_english(input),
        "disable persists off",
        checks,
    )?;
    toggle(hwnd);
    expect(
        is_english(hwnd) && ime_probe::text(edit) == "cqyh",
        "enable while visible switches layout and preserves query",
        checks,
    )?;
    hide(hwnd);
    expect(
        layout(hwnd) == prior,
        "hide restores actual invoking layout",
        checks,
    )?;
    prior = prior_layout(hwnd);
    show(hwnd, pid, &mut prior, checks)?;
    expect(is_english(hwnd), "next show uses English again", checks)?;
    if keyboard {
        expect(
            unsafe { GetForegroundWindow() } == hwnd,
            "real English keys target the controlled foreground window",
            checks,
        )?;
        ime_probe::type_keys(hwnd, "weixin")?;
        expect(
            ime_probe::text(edit) == "weixin",
            "real English keys produce literal pinyin query",
            checks,
        )?;
        capture(hwnd, &root.join("english-query.bmp"), true)?;
    }
    choose(hwnd, other);
    unsafe {
        let ime = ImmGetDefaultIMEWnd(edit);
        SendMessageW(ime, 0x283, 2, 1);
        SendMessageW(ime, 0x283, 6, 1);
    }
    expect(
        layout(hwnd) == other,
        "manual language switch remains available while visible",
        checks,
    )?;
    hide(hwnd);
    expect(
        layout(hwnd) == prior,
        "manual switch is restored when current session ends",
        checks,
    )?;
    prior = prior_layout(hwnd);
    show(hwnd, pid, &mut prior, checks)?;
    expect(
        is_english(hwnd),
        "manual choice is reset on next invocation",
        checks,
    )?;
    hide(hwnd);
    expect(
        layout(hwnd) == prior,
        "hide after manual switch restores original session layout",
        checks,
    )?;
    toggle(hwnd);
    show(hwnd, pid, &mut prior, checks)?;
    expect(
        layout(hwnd) == prior,
        &format!(
            "disabled subsequent invocation keeps prior layout (actual={:#x}, expected={:#x})",
            layout(hwnd) as usize,
            prior as usize
        ),
        checks,
    )?;
    hide(hwnd);
    choose(hwnd, original);
    expect(
        unsafe { GetKeyboardLayout(0) } == probe_layout,
        "launcher switching did not change verification thread layout",
        checks,
    )?;
    Ok(())
}

pub(super) fn restarted(hwnd: Hwnd, pid: u32, checks: &mut String) -> io::Result<()> {
    let other = alternate()?;
    choose(hwnd, other);
    let mut prior = prior_layout(hwnd);
    show(hwnd, pid, &mut prior, checks)?;
    expect(
        is_english(hwnd),
        "restart reads saved English toggle",
        checks,
    )?;
    hide(hwnd);
    expect(
        layout(hwnd) == prior,
        "restarted process restores prior layout on hide",
        checks,
    )?;
    toggle(hwnd); // Restore off in this test's data directory.
    Ok(())
}

pub(super) fn run() -> io::Result<()> {
    let root = std::env::current_dir()?.join("runtime/probe-english-review");
    let data = root.join(format!("data-{}", std::process::id()));
    let source = root.join("empty-source");
    fs::create_dir_all(&data)?;
    fs::create_dir_all(&source)?;
    let input = data.join("english-input.txt");
    super::super::settings::save_english(&input, false)?;
    let exe = std::env::current_exe()?.with_file_name("picorun.exe");
    let args = [
        "--hidden".into(),
        "--hotkey".into(),
        "Ctrl+Alt+F11".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
        "--source".into(),
        source.as_os_str().to_owned(),
        "--icons".into(),
        "off".into(),
    ];
    fn close_child(hwnd: Hwnd, child: &mut Running) -> io::Result<()> {
        unsafe { PostMessageW(hwnd, 0x10, 0, 0) };
        let started = Instant::now();
        loop {
            if let Some(status) = child.0.try_wait()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(io::Error::other("English probe child exit failed"))
                };
            }
            if started.elapsed() > Duration::from_secs(5) {
                return Err(io::Error::other("English probe child exit timed out"));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    let mut checks = String::new();
    let result = (|| -> io::Result<()> {
        let mut child = spawn(&exe, &args)?;
        let (hwnd, edit) = wait_window(&mut child)?;
        features(hwnd, edit, child.0.id(), &input, &root, false, &mut checks)?;
        close_child(hwnd, &mut child)?;
        super::super::settings::save_english(&input, true)?;
        let mut child = spawn(&exe, &args)?;
        let (hwnd, _) = wait_window(&mut child)?;
        restarted(hwnd, child.0.id(), &mut checks)?;
        close_child(hwnd, &mut child)
    })();
    fs::write(root.join("checks.txt"), &checks)?;
    result?;
    println!(
        "English verification: {} checks; evidence {}",
        checks
            .lines()
            .filter(|line| line.starts_with("PASS "))
            .count(),
        root.display()
    );
    Ok(())
}
