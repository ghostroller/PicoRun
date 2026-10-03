//! IME regression scenarios: injected ordering tests and optional installed-IME keyboard tests.
use super::*;

pub(super) fn text(edit: Hwnd) -> String {
    let mut buffer = [0u16; 1025];
    let len =
        unsafe { get_control_text(edit, buffer.as_mut_ptr(), buffer.len() as i32) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..len.min(1024)])
}
fn no_launch(marker: &Path, label: &str, checks: &mut String) -> io::Result<()> {
    thread::sleep(Duration::from_millis(120));
    expect(!marker.exists(), label, checks)
}
pub(super) fn injected(
    hwnd: Hwnd,
    edit: Hwnd,
    marker: &Path,
    checks: &mut String,
) -> io::Result<()> {
    let _ = fs::remove_file(marker);
    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
        SendMessageW(edit, 0x10d, 0, 0);
        set_control_text(edit, wide("微信").as_ptr());
        SendMessageW(edit, 0x100, 0x0d, 1);
    }
    no_launch(marker, "injected composition Enter does not launch", checks)?;
    unsafe {
        SendMessageW(edit, 0x10e, 0, 0);
        for _ in 0..10 {
            SendMessageW(edit, 0x100, 0x0d, 0x40000001);
        }
    }
    no_launch(
        marker,
        "composition ends before held Enter repeats; repeats do not launch",
        checks,
    )?;
    unsafe {
        SendMessageW(edit, 0x101, 0x0d, 0xc0000001u32 as isize);
        SendMessageW(edit, 0x100, 0x0d, 1);
    }
    expect(
        wait_marker(marker)?.contains("参数 with spaces|0"),
        "fresh Enter after IME confirmation release opens committed query",
        checks,
    )?;

    let _ = fs::remove_file(marker);
    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
        set_control_text(edit, wide("wx").as_ptr());
        SendMessageW(hwnd, 0x8003, 0, 0);
        SendMessageW(edit, 0x290, 0x0d, 1); // Legacy IME may forward a normal key after composition is gone.
        SendMessageW(edit, 0x100, 0x0d, 1);
    }
    no_launch(
        marker,
        "WM_IME_KEYDOWN forwarded Enter remains owned by IME",
        checks,
    )?;
    unsafe {
        SendMessageW(edit, 0x291, 0x0d, 0xc0000001u32 as isize);
        SendMessageW(edit, 0x100, 0x0d, 1);
    }
    expect(
        wait_marker(marker)?.contains("参数 with spaces|0"),
        "legacy IME key release restores launcher Enter",
        checks,
    )?;

    let _ = fs::remove_file(marker);
    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
        set_control_text(edit, wide("Synthetic App 000").as_ptr());
        SendMessageW(hwnd, 0x8003, 0, 0);
        SendMessageW(edit, 0x282, 5, 3); // Two candidate lists, with no start-composition notification.
        SendMessageW(edit, 0x100, 0x28, 1);
        SendMessageW(edit, 0x101, 0x28, 0xc0000001u32 as isize);
        SendMessageW(edit, 0x282, 4, 1);
        SendMessageW(edit, 0x100, 0x0d, 1);
        SendMessageW(edit, 0x101, 0x0d, 0xc0000001u32 as isize);
    }
    no_launch(
        marker,
        "remaining candidate list keeps Enter assigned to IME",
        checks,
    )?;
    unsafe {
        SendMessageW(edit, 0x282, 4, 2);
        SendMessageW(edit, 0x100, 0x0d, 1);
    }
    expect(
        wait_marker(marker)?.contains("参数 with spaces|4"),
        "candidate arrows do not change launcher selection",
        checks,
    )?;

    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
        SendMessageW(edit, 0x10d, 0, 0);
        SendMessageW(edit, 0x100, 0x1b, 1);
    }
    expect(
        unsafe { IsWindowVisible(hwnd) != 0 },
        "first composition Escape leaves launcher visible",
        checks,
    )?;
    unsafe {
        SendMessageW(edit, 0x10e, 0, 0);
        SendMessageW(edit, 0x101, 0x1b, 0xc0000001u32 as isize);
        SendMessageW(edit, 0x100, 0x1b, 1);
    }
    expect(
        unsafe { IsWindowVisible(hwnd) == 0 },
        "fresh Escape after composition ends hides launcher",
        checks,
    )?;

    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
        SendMessageW(edit, 0x10d, 0, 0);
        SendMessageW(hwnd, 0x6, 0, 0); // Hide without relying on an end-composition message.
        SendMessageW(hwnd, 0x8001, 0, 0);
        set_control_text(edit, wide("jsb").as_ptr());
    }
    let _ = fs::remove_file(marker);
    unsafe {
        SendMessageW(edit, 0x100, 0x0d, 1);
    }
    expect(
        wait_marker(marker)?.contains("参数 with spaces|1"),
        "hide/reopen resets stale IME state",
        checks,
    )?;
    Ok(())
}

fn input_focus(hwnd: Hwnd) -> io::Result<()> {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground != hwnd {
        let mut owner = 0;
        unsafe {
            GetWindowThreadProcessId(foreground, &mut owner);
        }
        return Err(io::Error::other(format!(
            "own launcher lost foreground before IME keyboard injection (foreground={foreground:p}, pid={owner}, own_visible={})",
            unsafe { IsWindowVisible(hwnd) }
        )));
    }
    Ok(())
}
fn press(hwnd: Hwnd, key: u8) -> io::Result<()> {
    input_focus(hwnd)?;
    unsafe {
        keybd_event(key, 0, 0, 0);
        keybd_event(key, 0, 2, 0);
    }
    thread::sleep(Duration::from_millis(50));
    Ok(())
}
pub(super) fn type_keys(hwnd: Hwnd, keys: &str) -> io::Result<()> {
    for byte in keys.bytes() {
        press(hwnd, byte.to_ascii_uppercase())?;
    }
    thread::sleep(Duration::from_millis(700)); // Let the installed IME/UI finish cold initialization.
    Ok(())
}
fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while !condition() {
        if started.elapsed() > Duration::from_secs(3) {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
    true
}
fn focus(hwnd: Hwnd) -> io::Result<()> {
    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
    }
    if unsafe { GetForegroundWindow() } != hwnd {
        actual_hotkey();
        actual_hotkey();
    }
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err(io::Error::other(
            "IME keyboard test requires own launcher foreground",
        ));
    }
    Ok(())
}
struct RestoreInput {
    hwnd: Hwnd,
    ime: Hwnd,
    layout: Handle,
    open: isize,
    conversion: isize,
}
impl Drop for RestoreInput {
    fn drop(&mut self) {
        unsafe {
            SendMessageW(self.ime, 0x283, 2, self.conversion);
            SendMessageW(self.ime, 0x283, 6, self.open);
            SendMessageW(self.hwnd, 0x50, 0, self.layout as isize);
        }
    }
}
pub(super) fn installed(
    hwnd: Hwnd,
    edit: Hwnd,
    marker: &Path,
    root: &Path,
    checks: &mut String,
) -> io::Result<()> {
    let mut layouts = [null_mut(); 32];
    let count = unsafe { GetKeyboardLayoutList(layouts.len() as i32, layouts.as_mut_ptr()) }.max(0)
        as usize;
    let chinese = layouts[..count.min(layouts.len())]
        .iter()
        .copied()
        .find(|p| *p as usize & 0xffff == 0x804)
        .ok_or_else(|| {
            io::Error::other(
                "no installed Simplified Chinese layout; no input method installed by this probe",
            )
        })?;
    let mut pid = 0;
    let tid = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    let layout = unsafe { GetKeyboardLayout(tid) };
    focus(hwnd)?;
    unsafe {
        SendMessageW(hwnd, 0x50, 0, chinese as isize);
    }
    let ime = unsafe { ImmGetDefaultIMEWnd(edit) };
    if ime.is_null() {
        return Err(io::Error::other("native Edit has no default IME window"));
    }
    let _restore = RestoreInput {
        hwnd,
        ime,
        layout,
        open: unsafe { SendMessageW(ime, 0x283, 5, 0) },
        conversion: unsafe { SendMessageW(ime, 0x283, 1, 0) },
    };
    unsafe {
        SendMessageW(ime, 0x283, 6, 1);
        SendMessageW(ime, 0x283, 2, 1);
    }
    checks.push_str(&format!(
        "installed_ime layout={:#x} open={} conversion={}\n",
        chinese as usize,
        unsafe { SendMessageW(ime, 0x283, 5, 0) },
        unsafe { SendMessageW(ime, 0x283, 1, 0) }
    ));

    focus(hwnd)?;
    type_keys(hwnd, "weixin")?;
    press(hwnd, 0x28)?;
    press(hwnd, 0x26)?;
    capture(hwnd, &root.join("ime-preedit.bmp"), true)?;
    let before = text(edit);
    let _ = fs::remove_file(marker);
    unsafe {
        keybd_event(0x0d, 0, 0, 0);
    }
    thread::sleep(Duration::from_millis(100));
    for _ in 0..5 {
        unsafe {
            keybd_event(0x0d, 0, 0, 0);
        }
        thread::sleep(Duration::from_millis(30));
    }
    unsafe {
        keybd_event(0x0d, 0, 2, 0);
    }
    no_launch(
        marker,
        "installed IME Enter confirms; held repeats do not open app",
        checks,
    )?;
    expect(
        wait_until(|| text(edit) == "weixin"),
        "installed IME Enter commits literal pinyin to native Edit",
        checks,
    )?;
    checks.push_str(&format!(
        "installed_ime Enter preedit={before:?} committed={:?}\n",
        text(edit)
    ));
    press(hwnd, 0x0d)?;
    expect(
        wait_marker(marker)?.contains("参数 with spaces|0"),
        "installed IME fresh Enter opens confirmed query",
        checks,
    )?;

    focus(hwnd)?;
    type_keys(hwnd, "weixin")?;
    press(hwnd, 0x20)?;
    thread::sleep(Duration::from_millis(150));
    let committed = text(edit);
    expect(
        committed == "微信",
        &format!("installed IME Space commits Chinese text (actual {committed:?})"),
        checks,
    )?;
    screenshot(hwnd, &root.join("ime-committed.bmp"))?;
    let _ = fs::remove_file(marker);
    press(hwnd, 0x0d)?;
    expect(
        wait_marker(marker)?.contains("参数 with spaces|0"),
        "Space commit followed by immediate Enter opens without extra confirmation",
        checks,
    )?;
    fs::write(root.join("ime-checks-progress.txt"), &*checks)?;

    focus(hwnd)?;
    type_keys(hwnd, "weixin")?;
    press(hwnd, 0x1b)?;
    expect(
        unsafe { IsWindowVisible(hwnd) != 0 } && text(edit).is_empty(),
        "installed IME first Escape cancels preedit and keeps launcher visible",
        checks,
    )?;
    fs::write(root.join("ime-checks-progress.txt"), &*checks)?;
    press(hwnd, 0x1b)?;
    expect(
        wait_until(|| unsafe { IsWindowVisible(hwnd) == 0 }),
        "installed IME second Escape hides launcher",
        checks,
    )?;
    focus(hwnd)?;
    type_keys(hwnd, "weixin")?;
    unsafe {
        SendMessageW(hwnd, 0x6, 0, 0);
    }
    expect(
        unsafe { IsWindowVisible(hwnd) == 0 },
        "hide cancels installed IME composition",
        checks,
    )?;
    focus(hwnd)?;
    unsafe {
        set_control_text(edit, wide("jsb").as_ptr());
    }
    let _ = fs::remove_file(marker);
    press(hwnd, 0x0d)?;
    expect(
        wait_marker(marker)?.contains("参数 with spaces|1"),
        "reopen after installed IME cancel can launch another query",
        checks,
    )?;
    Ok(())
}
