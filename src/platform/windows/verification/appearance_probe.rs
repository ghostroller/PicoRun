//! Stable input anchor, actual selection pixels and paired complete-process costs.
use super::*;

fn metric(hwnd: Hwnd, key: usize) -> isize {
    unsafe { SendMessageW(hwnd, 0x800c, key, 0) }
}
fn bounds(hwnd: Hwnd) -> Rect {
    let mut rect = Rect::default();
    unsafe { GetWindowRect(hwnd, &mut rect) };
    rect
}
fn query(hwnd: Hwnd, edit: Hwnd, text: &str) {
    unsafe {
        set_control_text(edit, wide(text).as_ptr());
        SendMessageW(hwnd, 0x8003, 0, 0);
    }
}
fn show(hwnd: Hwnd) {
    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
        UpdateWindow(hwnd);
    }
}
pub(super) fn foreground(hwnd: Hwnd) -> io::Result<()> {
    if unsafe { GetForegroundWindow() } != hwnd {
        // A hidden shell cannot always grant a child foreground activation. The already
        // registered test hotkey gives our launcher the normal user invocation permission.
        if unsafe { IsWindowVisible(hwnd) } != 0 {
            actual_hotkey();
        }
        actual_hotkey();
    }
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err(io::Error::other(
            "appearance window cannot obtain foreground through its registered hotkey",
        ));
    }
    Ok(())
}
fn hide(hwnd: Hwnd) {
    unsafe {
        SendMessageW(hwnd, 0x800a, 0, 0);
    }
}
fn select(hwnd: Hwnd, edit: Hwnd, start: usize, end: isize) {
    unsafe {
        SendMessageW(edit, 0xb1, start, end);
        UpdateWindow(edit);
        UpdateWindow(hwnd);
    }
}
fn colors(path: &Path, edit: Rect, panel: Rect) -> io::Result<(usize, usize)> {
    let bytes = fs::read(path)?;
    let width = i32::from_le_bytes(bytes[18..22].try_into().unwrap()) as usize;
    let theme_bg = if path.to_string_lossy().contains("light") {
        0x00dc_dfe4
    } else {
        0x003f_4249
    };
    let native = unsafe { GetSysColor(13) };
    let native = (native & 255) << 16 | native & 0xff00 | native >> 16 & 255;
    let mut counts = (0, 0);
    for y in (edit.top - panel.top).max(0)..(edit.bottom - panel.top).max(0) {
        for x in (edit.left - panel.left).max(0)..(edit.right - panel.left).max(0) {
            let offset = 54 + (y as usize * width + x as usize) * 4;
            if let Some(pixel) = bytes.get(offset..offset + 4) {
                let rgb = u32::from_le_bytes(pixel.try_into().unwrap()) & 0xffffff;
                counts.0 += usize::from(rgb == theme_bg);
                counts.1 += usize::from(rgb == native);
            }
        }
    }
    Ok(counts)
}
fn close(child: &mut Running, hwnd: Hwnd) -> io::Result<()> {
    unsafe {
        SendMessageW(hwnd, 0x10, 0, 0);
    }
    let started = Instant::now();
    while child.0.try_wait()?.is_none() && started.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    if !child.0.try_wait()?.is_some_and(|s| s.success()) {
        return Err(io::Error::other("appearance child did not exit normally"));
    }
    Ok(())
}
pub(super) fn run(bench: bool) -> io::Result<()> {
    super::super::window::enable_dpi();
    let root = std::env::current_dir()?.join("runtime/probe-appearance");
    let source = root.join("source");
    fs::create_dir_all(&source)?;
    if unsafe { CoInitializeEx(null_mut(), 2) } < 0 {
        return Err(io::Error::other("appearance COM init failed"));
    }
    let fixture = (0..500).try_for_each(|i| {
        let name = if i == 0 {
            "微信 QQ".into()
        } else {
            format!("Synthetic App {i:04}")
        };
        create_shortcut(
            &source.join(format!("{name}.lnk")),
            &std::env::current_exe()?,
            &format!(
                "--controlled-child \"{}\" appearance {i}",
                root.join("child.txt").display()
            ),
            &root,
        )
    });
    unsafe {
        CoUninitialize();
    }
    fixture?;
    let exe = std::env::current_exe()?.with_file_name("picorun.exe");
    let mut checks = String::new();
    let mut geometry =
        String::from("theme,dpi,query,left,top,width,height,edit_top,buffer_bytes\n");
    let sys_before: Vec<_> = (0..31).map(|c| unsafe { GetSysColor(c) }).collect();
    let result = (|| -> io::Result<()> {
        for theme in ["dark", "light"] {
            let data = root.join(format!("data-{theme}"));
            fs::create_dir_all(&data)?;
            fs::write(data.join("english-input.txt"), "on\n")?;
            let args = [
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
                "off".into(),
                "--measure-icons".into(),
                "--hold-measurement-window".into(),
            ];
            let mut child = spawn(&exe, &args)?;
            let (hwnd, edit) = wait_window(&mut child)?;
            barrier(hwnd);
            show(hwnd);
            foreground(hwnd)?;
            for dpi in [96, 120, 168, 192] {
                unsafe {
                    SendMessageW(hwnd, 0x800d, dpi, 0);
                }
                let mut anchor = None;
                for text in ["", "Synthetic", "微信 QQ", "no-hit-qzxv"] {
                    hide(hwnd);
                    query(hwnd, edit, text);
                    show(hwnd);
                    let panel = bounds(hwnd);
                    let input = bounds(edit);
                    let saved = anchor.get_or_insert((panel.left, panel.top, input.top));
                    expect(
                        *saved == (panel.left, panel.top, input.top),
                        "reopening different result counts keeps input anchor",
                        &mut checks,
                    )?;
                    let height = panel.bottom - panel.top;
                    expect(
                        height
                            == metric(hwnd, 9) as i32
                                + metric(hwnd, 8) as i32
                                    * if text.is_empty() || text == "Synthetic" {
                                        12
                                    } else {
                                        1
                                    }
                                + (60 * dpi / 96) as i32,
                        "height still adapts to 12/1/0 results",
                        &mut checks,
                    )?;
                    recalled_query(edit, text, "stable-position reopen", &mut checks)?;
                    geometry.push_str(&format!(
                        "{theme},{dpi},{text:?},{},{},{},{height},{},{}\n",
                        panel.left,
                        panel.top,
                        panel.right - panel.left,
                        input.top,
                        metric(hwnd, 13)
                    ));
                }
                query(hwnd, edit, "微信 QQ abcdef");
                select(hwnd, edit, 0, -1);
                let path = root.join(format!("{theme}-{dpi}-selection.bmp"));
                foreground(hwnd)?;
                expect(
                    unsafe { GetForegroundWindow() } == hwnd,
                    "selection capture owns foreground",
                    &mut checks,
                )?;
                capture(hwnd, &path, true)?;
                let (themed, native) = colors(&path, bounds(edit), bounds(hwnd))?;
                expect(themed > 20 && native == 0, &format!("actual selection uses neutral theme, themed={themed}, native_blue={native}"), &mut checks)?;
                expect(
                    metric(hwnd, 13) > 0 && metric(hwnd, 13) <= 512 * 1024,
                    "visible selection surface is bounded",
                    &mut checks,
                )?;
                select(hwnd, edit, 3, 5);
                let path = root.join(format!("{theme}-{dpi}-partial.bmp"));
                capture(hwnd, &path, true)?;
                let (themed, native) = colors(&path, bounds(edit), bounds(hwnd))?;
                expect(
                    themed > 20 && native == 0,
                    "partial selection uses same palette",
                    &mut checks,
                )?;
                select(hwnd, edit, 0, -1);
                unsafe {
                    SendMessageW(edit, 0x102, 'x' as usize, 1);
                }
                expect(
                    ime_probe::text(edit) == "x" && metric(hwnd, 13) == 0,
                    "typing replaces selection and releases its surface",
                    &mut checks,
                )?;
                query(hwnd, edit, &"a".repeat(1024));
                select(hwnd, edit, 0, -1);
                expect(
                    metric(hwnd, 13) <= 512 * 1024,
                    "long scrolled text does not enlarge surface",
                    &mut checks,
                )?;
                hide(hwnd);
                expect(
                    metric(hwnd, 13) == 0,
                    "hidden window releases selection surface",
                    &mut checks,
                )?;
            }
            close(&mut child, hwnd)?;
        }
        expect(
            (0..31).all(|c| unsafe { GetSysColor(c) } == sys_before[c as usize]),
            "system colors remain unchanged",
            &mut checks,
        )?;
        if bench {
            measure(&root, &source, &exe)?;
        }
        Ok(())
    })();
    fs::write(root.join("checks.txt"), &checks)?;
    fs::write(root.join("geometry.csv"), geometry)?;
    result?;
    println!(
        "Appearance verification: {} checks; evidence {}",
        checks.lines().filter(|l| l.starts_with("PASS")).count(),
        root.display()
    );
    Ok(())
}
fn measure(root: &Path, source: &Path, exe: &Path) -> io::Result<()> {
    let data = root.join("bench-data");
    fs::create_dir_all(&data)?;
    fs::write(data.join("english-input.txt"), "on\n")?;
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
        "--measure-icons".into(),
        "--hold-measurement-window".into(),
    ];
    let mut memory = String::from("stage,private_bytes,working_set_bytes,peak_commit_bytes,peak_working_set_bytes,gdi_handles,user_handles,cpu_100ns\n");
    let mut timings = String::from("version,run,dpi,index,reopen_ms\n");
    for run in 0..5 {
        for version in if run % 2 == 0 {
            ["baseline", "current"]
        } else {
            ["current", "baseline"]
        } {
            let baseline = root.join("baseline.exe");
            let mut child = spawn(
                if version == "baseline" {
                    &baseline
                } else {
                    exe
                },
                &args,
            )?;
            let (hwnd, edit) = wait_window(&mut child)?;
            barrier(hwnd);
            sample(
                child.0.id(),
                &format!("{version}-{run}-initial-hidden"),
                &mut memory,
            )?;
            show(hwnd);
            foreground(hwnd)?;
            query(hwnd, edit, "synthetic");
            for _ in 0..20 {
                hide(hwnd);
                show(hwnd);
            }
            if unsafe { GetForegroundWindow() } != hwnd || ime_probe::text(edit) != "synthetic" {
                return Err(io::Error::other(
                    "appearance benchmark lost foreground or received extra input during warmup",
                ));
            }
            let dpi = metric(hwnd, 7);
            sample(
                child.0.id(),
                &format!("{version}-{run}-selected"),
                &mut memory,
            )?;
            for i in 0..120 {
                hide(hwnd);
                let started = Instant::now();
                show(hwnd);
                let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                if unsafe { GetForegroundWindow() } != hwnd || ime_probe::text(edit) != "synthetic"
                {
                    return Err(io::Error::other(
                        "appearance benchmark interrupted; no performance conclusion",
                    ));
                }
                timings.push_str(&format!("{version},{run},{dpi},{i},{:.6}\n", elapsed));
            }
            sample(
                child.0.id(),
                &format!("{version}-{run}-after-reopen"),
                &mut memory,
            )?;
            hide(hwnd);
            sample(
                child.0.id(),
                &format!("{version}-{run}-final-hidden"),
                &mut memory,
            )?;
            let before = sample(
                child.0.id(),
                &format!("{version}-{run}-idle-before"),
                &mut memory,
            )?;
            thread::sleep(Duration::from_secs(1));
            let after = sample(
                child.0.id(),
                &format!("{version}-{run}-idle-after"),
                &mut memory,
            )?;
            println!(
                "{version} run={run} dpi={dpi} idle_1s_cpu_ms={:.3}",
                after.saturating_sub(before) as f64 / 10000.0
            );
            close(&mut child, hwnd)?;
        }
    }
    fs::write(root.join("memory.csv"), memory)?;
    fs::write(root.join("timings.csv"), timings)?;
    Ok(())
}
