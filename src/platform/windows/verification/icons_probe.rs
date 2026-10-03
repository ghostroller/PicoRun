//! Actual popup selection, persistence, active-load cancellation and resource release.
use super::*;
#[link(name = "user32")]
unsafe extern "system" {
    fn GetMenuState(menu: Handle, item: u32, flags: u32) -> u32;
}
fn stat(hwnd: Hwnd, key: usize) -> isize {
    unsafe { SendMessageW(hwnd, 0x8006, key, 0) }
}
fn ready(hwnd: Hwnd) -> io::Result<()> {
    let started = Instant::now();
    while stat(hwnd, 0) != 1 {
        if started.elapsed() > Duration::from_secs(15) {
            return Err(io::Error::other("icons probe completion timed out"));
        }
        thread::sleep(Duration::from_millis(1));
    }
    barrier(hwnd);
    Ok(())
}
fn choose_menu(hwnd: Hwnd, pid: u32, checked: bool, checks: &mut String) -> io::Result<()> {
    let menu = open_tray_menu(hwnd, pid)?;
    // MN_GETHMENU returns this owned popup's HMENU while its nested loop is running.
    let handle = unsafe { SendMessageW(menu, 0x1e1, 0, 0) } as Handle;
    let flags = unsafe { GetMenuState(handle, tray::ICONS, 0) };
    expect(
        flags != u32::MAX && (flags & 8 != 0) == checked,
        "native icon menu checked state matches setting",
        checks,
    )?;
    unsafe {
        PostMessageW(menu, 0x102, 'i' as usize, 1);
    }
    let started = Instant::now();
    while (stat(hwnd, 17) != 0) == checked {
        if started.elapsed() > Duration::from_secs(3) {
            return Err(io::Error::other("icon popup selection timed out"));
        }
        thread::sleep(Duration::from_millis(10));
    }
    barrier(hwnd);
    Ok(())
}
pub(super) fn features(
    hwnd: Hwnd,
    edit: Hwnd,
    pid: u32,
    data: &Path,
    root: &Path,
    checks: &mut String,
) -> io::Result<()> {
    let settings = data.join("icons.txt");
    expect(
        stat(hwnd, 17) == 0 && stat(hwnd, 18) == 0,
        "default icons off starts no worker",
        checks,
    )?;
    choose_menu(hwnd, pid, false, checks)?;
    expect(
        super::super::settings::load_toggle(&settings) && stat(hwnd, 18) == 0,
        "enable while hidden persists without loading icons",
        checks,
    )?;
    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
    }
    ready(hwnd)?;
    expect(
        stat(hwnd, 18) == 1 && stat(hwnd, 6) == 12,
        "first show loads only visible icons",
        checks,
    )?;
    screenshot(hwnd, &root.join("icons-on.bmp"))?;
    unsafe {
        set_control_text(edit, wide("cqyh").as_ptr());
        SendMessageW(hwnd, 0x8003, 0, 0);
    }
    ready(hwnd)?;
    choose_menu(hwnd, pid, true, checks)?;
    expect(
        !super::super::settings::load_toggle(&settings) && ime_probe::text(edit) == "cqyh",
        "native disable persists and preserves query",
        checks,
    )?;
    expect(
        (1..=15)
            .filter(|key| *key != 4 && *key != 5)
            .all(|key| stat(hwnd, key) == 0)
            && stat(hwnd, 18) == 0,
        "disable releases worker, snapshots and cache",
        checks,
    )?;
    screenshot(hwnd, &root.join("icons-off.bmp"))?;
    // Toggle off while a request may still be in-flight. Late completions must not restore icons.
    for index in 0..20 {
        unsafe {
            SendMessageW(hwnd, 0x111, tray::ICONS as usize, 0);
            SendMessageW(hwnd, 0x8007, index, 0);
            SendMessageW(hwnd, 0x111, tray::ICONS as usize, 0);
        }
        expect(
            stat(hwnd, 18) == 0 && stat(hwnd, 6) == 0,
            "in-flight disable leaves no worker or icons",
            checks,
        )?;
    }
    thread::sleep(Duration::from_millis(100));
    barrier(hwnd);
    expect(
        stat(hwnd, 6) == 0,
        "late worker notifications do not repopulate disabled view",
        checks,
    )?;
    unsafe {
        SendMessageW(hwnd, 0x111, tray::ICONS as usize, 0);
    }
    ready(hwnd)?;
    let parses = stat(hwnd, 8);
    unsafe {
        SendMessageW(hwnd, 0x8002, 0, 0);
    }
    ready(hwnd)?;
    expect(
        stat(hwnd, 15) == 1 && stat(hwnd, 8) > parses,
        "F5 invalidates icon metadata and resources",
        checks,
    )?;
    expect(
        stat(hwnd, 1) <= 48 && stat(hwnd, 10) <= 512 && stat(hwnd, 11) <= 131072,
        "re-enabled icon cache stays bounded",
        checks,
    )?;
    // Leave icons enabled for the existing complete keyboard/IME/theme/launch regression probe.
    unsafe {
        SendMessageW(hwnd, 0x6, 0, 0);
    }
    Ok(())
}
