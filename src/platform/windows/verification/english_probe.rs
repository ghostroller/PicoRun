//! Own-window layout, native menu and persistence regressions.
use super::*;

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
fn show(hwnd: Hwnd) {
    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
    }
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
    show(hwnd);
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
    let mut prior = prior_layout(hwnd);
    show(hwnd);
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
    show(hwnd);
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
    show(hwnd);
    expect(is_english(hwnd), "next show uses English again", checks)?;
    if keyboard {
        if unsafe { GetForegroundWindow() } != hwnd {
            actual_hotkey();
            prior = prior_layout(hwnd);
            actual_hotkey();
        }
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
    show(hwnd);
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
    show(hwnd);
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

pub(super) fn restarted(hwnd: Hwnd, checks: &mut String) -> io::Result<()> {
    let other = alternate()?;
    choose(hwnd, other);
    let prior = prior_layout(hwnd);
    show(hwnd);
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
