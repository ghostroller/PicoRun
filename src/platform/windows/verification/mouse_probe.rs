//! Click real launcher windows; all shortcuts open only the controlled marker writer.
use super::*;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetCursorPos(point: *mut Point) -> i32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn WindowFromPoint(point: Point) -> Hwnd;
    fn mouse_event(flags: u32, x: u32, y: u32, data: u32, extra: usize);
}

fn physical_click(hwnd: Hwnd, x: i32, y: i32) -> io::Result<()> {
    let original = unsafe {
        let mut point = Point { x, y };
        let mut original = Point::default();
        if GetForegroundWindow() != hwnd
            || ClientToScreen(hwnd, &mut point) == 0
            || WindowFromPoint(point) != hwnd
            || GetCursorPos(&mut original) == 0
        {
            return Err(io::Error::other(
                "physical mouse test requires own unobstructed foreground window",
            ));
        }
        if SetCursorPos(point.x, point.y) == 0 {
            return Err(io::Error::last_os_error());
        }
        // Recheck ownership immediately before injecting; never press on another app.
        let owned = GetForegroundWindow() == hwnd && WindowFromPoint(point) == hwnd;
        if owned {
            mouse_event(2, 0, 0, 0, 0);
            mouse_event(4, 0, 0, 0, 0);
        }
        if !owned {
            SetCursorPos(original.x, original.y);
            return Err(io::Error::other(
                "mouse test foreground changed before injection",
            ));
        }
        original
    };
    barrier(hwnd);
    thread::sleep(Duration::from_millis(120));
    unsafe { SetCursorPos(original.x, original.y) };
    Ok(())
}

fn packed(x: i32, y: i32) -> isize {
    (u32::from(x as u16) | u32::from(y as u16) << 16) as isize
}
fn click(hwnd: Hwnd, x: i32, y: i32) {
    unsafe {
        SendMessageW(hwnd, 0x201, 1, packed(x, y));
        SendMessageW(hwnd, 0x202, 0, packed(x, y));
    }
}
fn selected(hwnd: Hwnd) -> isize {
    unsafe { SendMessageW(hwnd, 0x800c, 6, 0) }
}
fn query(hwnd: Hwnd, edit: Hwnd, text: &str) {
    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
        set_control_text(edit, wide(text).as_ptr());
        SendMessageW(hwnd, 0x8003, 0, 0);
    }
}
fn clear(markers: &[PathBuf]) -> io::Result<()> {
    for marker in markers {
        match fs::remove_file(marker) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
fn unopened(markers: &[PathBuf], label: &str, checks: &mut String) -> io::Result<()> {
    thread::sleep(Duration::from_millis(120));
    expect(markers.iter().all(|m| !m.exists()), label, checks)
}
fn opened(
    hwnd: Hwnd,
    markers: &[PathBuf],
    index: usize,
    work: &Path,
    label: &str,
    checks: &mut String,
) -> io::Result<()> {
    let text = wait_marker(&markers[index])?;
    let visible = unsafe { IsWindowVisible(hwnd) != 0 };
    let other_markers: Vec<_> = markers
        .iter()
        .enumerate()
        .filter(|(i, m)| *i != index && m.exists())
        .map(|(i, _)| i)
        .collect();
    expect(
        text.contains(&format!("args=参数 with spaces|{index}\n"))
            && text.lines().find_map(|line| line.strip_prefix("cwd="))
                .is_some_and(|cwd| Path::new(cwd) == work)
            && other_markers.is_empty()
            && !visible,
        &format!("{label}; visible={visible}, other_markers={other_markers:?}, expected_cwd={}, actual={text:?}", work.display()),
        checks,
    )
}

pub(super) fn run() -> io::Result<()> {
    super::super::window::enable_dpi();
    let root = std::env::current_dir()?.join("runtime").join("probe-mouse");
    let source = root.join("应用 入口");
    let work = root.join("工作 目录");
    fs::create_dir_all(&source)?;
    fs::create_dir_all(&work)?;
    let target = root.join("受控 程序.exe");
    fs::copy(std::env::current_exe()?, &target)?;
    let markers: Vec<_> = (0..3)
        .map(|i| root.join(format!("launch-{i}.txt")))
        .collect();
    if unsafe { CoInitializeEx(null_mut(), 2) } < 0 {
        return Err(io::Error::other("mouse probe COM init failed"));
    }
    let links = (0..3).try_for_each(|i| {
        create_shortcut(
            &source.join(format!("Mouse App {i:04}.lnk")),
            &target,
            &format!(
                "--controlled-child \"{}\" \"参数 with spaces\" {i}",
                markers[i].display()
            ),
            &work,
        )
    });
    unsafe { CoUninitialize() };
    links?;
    let exe = std::env::current_exe()?.with_file_name("picorun.exe");
    let mut checks = String::new();
    let result = (|| -> io::Result<()> {
        for theme in ["dark", "light"] {
            for icons in ["off", "on"] {
                let data = root.join(format!("data-{theme}-{icons}"));
                fs::create_dir_all(&data)?;
                fs::write(data.join("english-input.txt"), "off\n")?;
                let args = vec![
                    "--hidden".into(),
                    "--hotkey".into(),
                    "Ctrl+Alt+F11".into(),
                    "--data-dir".into(),
                    data.as_os_str().to_owned(),
                    "--source".into(),
                    source.as_os_str().to_owned(),
                    "--theme".into(),
                    theme.into(),
                    "--icons".into(),
                    icons.into(),
                    "--measure-icons".into(),
                    "--hold-measurement-window".into(),
                ];
                let mut child = spawn(&exe, &args)?;
                let (hwnd, edit) = wait_window(&mut child)?;
                barrier(hwnd);
                if theme == "dark" && icons == "off" {
                    clear(&markers)?;
                    query(hwnd, edit, "Mouse App");
                    let dpi = unsafe { SendMessageW(hwnd, 0x800c, 7, 0) };
                    let top = unsafe { SendMessageW(hwnd, 0x800c, 9, 0) } as i32;
                    let height = unsafe { SendMessageW(hwnd, 0x800c, 8, 0) } as i32;
                    let x = 36 * dpi as i32 / 96;
                    let y = top + height + height / 2;
                    checks.push_str(&format!("physical mouse, actual dpi={dpi}\n"));
                    physical_click(hwnd, x, y)?;
                    expect(
                        selected(hwnd) == 1,
                        "actual mouse selects second row",
                        &mut checks,
                    )?;
                    unopened(&markers, "actual first click does not open", &mut checks)?;
                    physical_click(hwnd, x, y)?;
                    opened(
                        hwnd,
                        &markers,
                        1,
                        &work,
                        "actual second click opens controlled app",
                        &mut checks,
                    )?;
                }
                for dpi in [96, 120, 144, 192] {
                    unsafe { SendMessageW(hwnd, 0x800d, dpi, 0) };
                    let top = unsafe { SendMessageW(hwnd, 0x800c, 9, 0) } as i32;
                    let height = unsafe { SendMessageW(hwnd, 0x800c, 8, 0) } as i32;
                    let width = unsafe { SendMessageW(hwnd, 0x800c, 10, 0) } as i32;
                    let pad = 12 * dpi as i32 / 96;
                    let x = pad * 3;
                    let y = |index: i32| top + height * index + height / 2;
                    checks.push_str(&format!(
                        "theme={theme},icons={icons},dpi={dpi},top={top},row={height}\n"
                    ));
                    clear(&markers)?;
                    query(hwnd, edit, "Mouse App");
                    click(hwnd, x, y(1));
                    expect(
                        selected(hwnd) == 1,
                        "first click selects a different row",
                        &mut checks,
                    )?;
                    unopened(&markers, "selection click does not open", &mut checks)?;
                    click(hwnd, x, y(1));
                    opened(
                        hwnd,
                        &markers,
                        1,
                        &work,
                        "second click opens original shortcut with args/cwd",
                        &mut checks,
                    )?;

                    clear(&markers)?;
                    for _ in 0..3 {
                        click(hwnd, x, y(1));
                    }
                    unopened(
                        &markers,
                        "queued/duplicate clicks on hidden panel do not reopen",
                        &mut checks,
                    )?;

                    query(hwnd, edit, "Mouse App");
                    unsafe {
                        SendMessageW(edit, 0x100, 0x28, 1);
                        SendMessageW(edit, 0x100, 0x28, 1);
                    }
                    expect(
                        selected(hwnd) == 2,
                        "keyboard selects third row",
                        &mut checks,
                    )?;
                    click(hwnd, x, y(2));
                    opened(
                        hwnd,
                        &markers,
                        2,
                        &work,
                        "click opens keyboard-selected row",
                        &mut checks,
                    )?;

                    clear(&markers)?;
                    query(hwnd, edit, "Mouse App");
                    click(hwnd, pad, top);
                    opened(
                        hwnd,
                        &markers,
                        0,
                        &work,
                        "default-selected first row opens at its top/left edge",
                        &mut checks,
                    )?;

                    clear(&markers)?;
                    query(hwnd, edit, "Mouse App");
                    click(hwnd, x, y(1));
                    for (cx, cy) in [
                        (pad - 1, y(1)),
                        (width - pad, y(1)),
                        (-1, y(1)),
                        (width + 1, y(1)),
                        (x, top - 1),
                        (x, -1),
                        (x, top + height * 3),
                        (x, y(4)),
                    ] {
                        click(hwnd, cx, cy);
                    }
                    unopened(
                        &markers,
                        "padding/footer/negative/outside clicks do not open",
                        &mut checks,
                    )?;
                    expect(
                        selected(hwnd) == 1,
                        "invalid clicks preserve selection",
                        &mut checks,
                    )?;

                    query(hwnd, edit, "no-match-qxzv");
                    click(hwnd, x, y(0));
                    unopened(&markers, "empty result area does not open", &mut checks)?;
                    expect(
                        selected(hwnd) == -1,
                        "no results has no selection",
                        &mut checks,
                    )?;

                    query(hwnd, edit, "Mouse App 0000");
                    unsafe { SendMessageW(edit, 0x10d, 0, 0) };
                    click(hwnd, x, y(0));
                    unopened(&markers, "IME composition click does not open", &mut checks)?;
                    unsafe {
                        SendMessageW(edit, 0x10e, 0, 0);
                        SendMessageW(edit, 0x282, 5, 1);
                    }
                    click(hwnd, x, y(0));
                    unopened(
                        &markers,
                        "open IME candidate list click does not open",
                        &mut checks,
                    )?;
                    unsafe {
                        SendMessageW(edit, 0x282, 4, 1);
                        SendMessageW(hwnd, 0x8003, 0, 0);
                    }
                    barrier(hwnd);
                    expect(
                        ime_probe::text(edit) == "Mouse App 0000" && selected(hwnd) == 0,
                        &format!(
                            "IME close retains committed query, actual={:?}, selected={}",
                            ime_probe::text(edit),
                            selected(hwnd)
                        ),
                        &mut checks,
                    )?;
                    click(hwnd, x, y(0));
                    opened(
                        hwnd,
                        &markers,
                        0,
                        &work,
                        "fresh click after IME closes can open",
                        &mut checks,
                    )?;

                    clear(&markers)?;
                    query(hwnd, edit, "Mouse App 000");
                    unsafe {
                        SendMessageW(edit, 0xb1, 13, 13);
                        // WM_CHAR posts EN_CHANGE's coalesced refresh behind this click
                        // when the batch arrives before the launcher processes it.
                        PostMessageW(edit, 0x102, '2' as usize, 1);
                        PostMessageW(hwnd, 0x201, 1, packed(x, y(0)));
                        PostMessageW(hwnd, 0x202, 0, packed(x, y(0)));
                    }
                    barrier(hwnd);
                    thread::sleep(Duration::from_millis(120));
                    expect(
                        ime_probe::text(edit) == "Mouse App 0002"
                            && !markers[0].exists()
                            && !markers[1].exists(),
                        "immediate text change/click never opens stale displayed application",
                        &mut checks,
                    )?;
                    if !markers[2].exists() {
                        click(hwnd, x, y(0));
                    }
                    opened(
                        hwnd,
                        &markers,
                        2,
                        &work,
                        "click after text sync opens current result",
                        &mut checks,
                    )?;
                }
                unsafe { SendMessageW(hwnd, 0x10, 0, 0) };
                let status = child.0.wait()?;
                expect(
                    status.success(),
                    "mouse test process exits normally",
                    &mut checks,
                )?;
            }
        }
        Ok(())
    })();
    fs::write(root.join("checks.txt"), &checks)?;
    result
}
