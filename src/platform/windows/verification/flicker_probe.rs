//! Selection paint regions, visible keyboard input, bounded buffering and redraw A/B.
use super::*;

fn metric(hwnd: Hwnd, key: usize) -> isize {
    unsafe { SendMessageW(hwnd, 0x800c, key, 0) }
}
fn icons_ready(hwnd: Hwnd, enabled: bool) -> io::Result<()> {
    let started = Instant::now();
    loop {
        let ready = unsafe { SendMessageW(hwnd, 0x8006, 0, 0) } == 1;
        let visible = unsafe { SendMessageW(hwnd, 0x8006, 6, 0) };
        // This fixture has either 12 results or none. Reopening now retains a no-hit query.
        let expected = if metric(hwnd, 6) < 0 { 0 } else { 12 };
        if ready && (!enabled || visible == expected) {
            break;
        }
        if started.elapsed() > Duration::from_secs(10) {
            return Err(io::Error::other("flicker probe icon readiness timed out"));
        }
        thread::sleep(Duration::from_millis(10));
    }
    barrier(hwnd);
    Ok(())
}
fn arrow(hwnd: Hwnd, key: usize) -> (isize, [isize; 6], f64) {
    let started = Instant::now();
    let selected = unsafe { SendMessageW(hwnd, 0x800b, key, 0) };
    let milliseconds = started.elapsed().as_secs_f64() * 1000.0;
    (
        selected,
        std::array::from_fn(|i| metric(hwnd, i)),
        milliseconds,
    )
}
fn area(region: &[isize; 6]) -> isize {
    (region[2] - region[0]).max(0) * (region[3] - region[1]).max(0)
}
fn validate_region(
    hwnd: Hwnd,
    before: isize,
    selected: isize,
    region: &[isize; 6],
    checks: &mut String,
) -> io::Result<()> {
    let top = metric(hwnd, 9);
    let height = metric(hwnd, 8);
    if before == selected {
        expect(
            area(region) == 0,
            "unchanged/boundary/empty selection causes no repaint",
            checks,
        )?;
    } else {
        if region[1] != top + before.min(selected) * height
            || region[3] != top + (before.max(selected) + 1) * height
        {
            return Err(io::Error::other(format!("selection redraw mismatch: before={before}, selected={selected}, region={region:?}, top={top}, height={height}, dpi={}", metric(hwnd,7))));
        }
        expect(
            region[1] == top + before.min(selected) * height
                && region[3] == top + (before.max(selected) + 1) * height,
            "selection redraw is confined to the previous and new rows",
            checks,
        )?;
    }
    expect(
        region[4] == 0 && region[5] == 0,
        "arrow does not invalidate or repaint native Edit",
        checks,
    )
}
fn close(child: &mut Running, hwnd: Hwnd) -> io::Result<()> {
    unsafe {
        PostMessageW(hwnd, 0x10, 0, 0);
    }
    let started = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other("flicker child exit failed"))
            };
        }
        if started.elapsed() > Duration::from_secs(3) {
            return Err(io::Error::other("flicker child exit timed out"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
pub(super) fn run(reference: bool) -> io::Result<()> {
    super::super::window::enable_dpi();
    let common = std::env::current_dir()?.join("runtime/flicker");
    let root = common.join(if reference {
        "probe-reference"
    } else {
        "probe-fixed"
    });
    let source = common.join("应用 来源");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&source)?;
    let exe = if reference {
        common.join("reference-instrumented.exe")
    } else {
        std::env::current_exe()?.with_file_name("picorun.exe")
    };
    unsafe {
        if CoInitializeEx(null_mut(), 2) < 0 {
            return Err(io::Error::other("flicker COM init failed"));
        }
    }
    let shortcuts = (0..500).try_for_each(|i| {
        let name = if i == 0 {
            "重庆银行".into()
        } else {
            format!("Synthetic App {i:04}")
        };
        create_shortcut(
            &source.join(format!("{name}.lnk")),
            &std::env::current_exe()?,
            "",
            &common,
        )
    });
    unsafe {
        CoUninitialize();
    }
    shortcuts?;
    let mut checks = String::new();
    let mut memory = String::from("stage,private_bytes,working_set_bytes,peak_commit_bytes,peak_working_set_bytes,gdi_handles,user_handles,cpu_100ns\n");
    let mut responses = String::from("theme,icons,run,phase,index,selection,left,top,right,bottom,edit_dirty,edit_paints,elapsed_ms,buffer_pixels\n");
    let result = (|| -> io::Result<()> {
        for theme in ["dark", "light"] {
            for enabled in [false, true] {
                let data = root.join(format!("data-{theme}-{enabled}"));
                fs::create_dir_all(&data)?;
                fs::write(data.join("english-input.txt"), "on\n")?;
                fs::write(data.join("theme.txt"), format!("{theme}\n"))?;
                let _ = fs::remove_file(data.join("apps-v1.bin"));
                for run in 0..3 {
                    let args = vec![
                        "--hotkey".into(),
                        "Ctrl+Alt+F11".into(),
                        "--data-dir".into(),
                        data.as_os_str().to_owned(),
                        "--source".into(),
                        source.as_os_str().to_owned(),
                        "--theme".into(),
                        theme.into(),
                        "--icons".into(),
                        if enabled { "on".into() } else { "off".into() },
                        "--measure-icons".into(),
                        "--hold-measurement-window".into(),
                    ];
                    let mut child = spawn(&exe, &args)?;
                    let (hwnd, edit) = wait_window(&mut child)?;
                    // Edit creation can precede tray/hotkey setup and the first ShowWindow.
                    let started = Instant::now();
                    while unsafe { IsWindowVisible(hwnd) } == 0 {
                        if started.elapsed() > Duration::from_secs(5)
                            || child.0.try_wait()?.is_some()
                        {
                            return Err(io::Error::other("first show did not finish"));
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    // All A/B processes use the same primary monitor, avoiding mixed DPI.
                    unsafe {
                        SetWindowPos(hwnd, null_mut(), 80, 80, 0, 0, 0x15);
                    }
                    icons_ready(hwnd, enabled)?;
                    expect(
                        unsafe { IsWindowVisible(hwnd) } != 0 && ime_probe::text(edit).is_empty(),
                        "first visible opening has empty native Edit",
                        &mut checks,
                    )?;
                    let stage = format!("{theme}-{enabled}-{run}");
                    sample(child.0.id(), &format!("{stage}-visible"), &mut memory)?;
                    let mut log = |phase: &str,
                                   index: usize,
                                   key: usize,
                                   checks: &mut String|
                     -> io::Result<()> {
                        let before = metric(hwnd, 6);
                        let (selected, region, ms) = arrow(hwnd, key);
                        responses.push_str(&format!("{theme},{enabled},{run},{phase},{index},{selected},{},{},{},{},{},{},{ms:.6},{}\n",
                            region[0],region[1],region[2],region[3],region[4],region[5],metric(hwnd,12)));
                        if !reference {
                            validate_region(hwnd, before, selected, &region, checks)?;
                        }
                        Ok(())
                    };
                    log("first_up", 0, 0x26, &mut checks)?;
                    log("first_down", 0, 0x28, &mut checks)?;
                    expect(
                        metric(hwnd, 6) == 1,
                        "first Down changes selection to row 1",
                        &mut checks,
                    )?;
                    for i in 0..200 {
                        log(
                            "empty_query",
                            i,
                            if i % 2 == 0 { 0x28 } else { 0x26 },
                            &mut checks,
                        )?;
                    }
                    for _ in 0..20 {
                        arrow(hwnd, 0x28);
                    }
                    log("last_boundary", 0, 0x28, &mut checks)?;
                    unsafe {
                        set_control_text(edit, wide("synthetic").as_ptr());
                        SendMessageW(hwnd, 0x8003, 0, 0);
                    }
                    icons_ready(hwnd, enabled)?;
                    for i in 0..200 {
                        log(
                            "typed_query",
                            i,
                            if i % 2 == 0 { 0x28 } else { 0x26 },
                            &mut checks,
                        )?;
                    }
                    expect(
                        ime_probe::text(edit) == "synthetic",
                        "arrows preserve committed search text",
                        &mut checks,
                    )?;
                    sample(child.0.id(), &format!("{stage}-after-arrows"), &mut memory)?;
                    if !reference {
                        expect(
                            metric(hwnd, 12) == metric(hwnd, 10) * metric(hwnd, 8),
                            "buffer is exactly one scaled row",
                            &mut checks,
                        )?;
                    }
                    // Take pictures after the measured workload: WM_PRINTCLIENT can
                    // load a different GDI/font path and must not inflate redraw samples.
                    if run == 0 {
                        unsafe {
                            SendMessageW(hwnd, 0x8001, 0, 0);
                        }
                        icons_ready(hwnd, enabled)?;
                        arrow(hwnd, 0x28);
                        screenshot(
                            hwnd,
                            &root.join(format!("{theme}-{enabled}-first-down.bmp")),
                        )?;
                    }
                    if run == 0 && !reference {
                        let original_dpi = metric(hwnd, 7) as usize;
                        for dpi in [96, 144, 192] {
                            unsafe {
                                SendMessageW(hwnd, 0x800d, dpi, 0);
                            }
                            icons_ready(hwnd, enabled)?;
                            expect(
                                metric(hwnd, 7) == dpi as isize
                                    && metric(hwnd, 12) == metric(hwnd, 10) * metric(hwnd, 8),
                                "DPI change recreates exactly one row buffer",
                                &mut checks,
                            )?;
                            let before = metric(hwnd, 6);
                            let (selected, region, _) = arrow(hwnd, 0x28);
                            validate_region(hwnd, before, selected, &region, &mut checks)?;
                        }
                        unsafe {
                            SendMessageW(hwnd, 0x800d, original_dpi, 0);
                        }
                        icons_ready(hwnd, enabled)?;
                        // Visible keyboard verification goes through Windows' normal key delivery.
                        unsafe {
                            // Start this first-row scenario explicitly empty; show retains text.
                            set_control_text(edit, wide("").as_ptr());
                            SendMessageW(hwnd, 0x8003, 0, 0);
                            SendMessageW(hwnd, 0x8001, 0, 0);
                            SetForegroundWindow(hwnd);
                        }
                        barrier(hwnd);
                        if unsafe { GetForegroundWindow() } != hwnd {
                            actual_hotkey();
                            actual_hotkey();
                        }
                        expect(
                            unsafe { GetForegroundWindow() } == hwnd,
                            "controlled launcher owns keyboard focus",
                            &mut checks,
                        )?;
                        unsafe {
                            keybd_event(0x28, 0, 0, 0);
                            keybd_event(0x28, 0, 2, 0);
                        }
                        barrier(hwnd);
                        expect(
                            metric(hwnd, 6) == 1 && ime_probe::text(edit).is_empty(),
                            "actual first Down preserves Edit and selects second row",
                            &mut checks,
                        )?;
                        unsafe {
                            keybd_event(0x26, 0, 0, 0);
                            keybd_event(0x26, 0, 2, 0);
                        }
                        barrier(hwnd);
                        expect(
                            metric(hwnd, 6) == 0,
                            "actual Up restores first row",
                            &mut checks,
                        )?;
                    }
                    unsafe {
                        set_control_text(edit, wide("不存在_Probe_NoHit").as_ptr());
                        SendMessageW(hwnd, 0x8003, 0, 0);
                    }
                    barrier(hwnd);
                    let (selected, region, _) = arrow(hwnd, 0x28);
                    expect(selected == -1, "empty results ignore Down", &mut checks)?;
                    if !reference {
                        validate_region(hwnd, -1, selected, &region, &mut checks)?;
                    }
                    unsafe {
                        SendMessageW(hwnd, 0x800a, 0, 0);
                    }
                    if !reference {
                        expect(
                            metric(hwnd, 12) == 0,
                            "hide releases row bitmap and DC",
                            &mut checks,
                        )?;
                    }
                    for _ in 0..3 {
                        unsafe {
                            SendMessageW(hwnd, 0x8001, 0, 0);
                        }
                        icons_ready(hwnd, enabled)?;
                        let previous = metric(hwnd, 6);
                        let (selected, region, _) = arrow(hwnd, 0x28);
                        if !reference {
                            validate_region(hwnd, previous, selected, &region, &mut checks)?;
                        }
                        unsafe {
                            SendMessageW(hwnd, 0x800a, 0, 0);
                        }
                    }
                    let before =
                        sample(child.0.id(), &format!("{stage}-idle-before"), &mut memory)?;
                    thread::sleep(Duration::from_secs(1));
                    let after = sample(child.0.id(), &format!("{stage}-idle-after"), &mut memory)?;
                    checks.push_str(&format!(
                        "OBSERVE {stage} hidden idle 1s CPU delta {} ms\n",
                        after.saturating_sub(before) as f64 / 10_000.0
                    ));
                    close(&mut child, hwnd)?;
                    println!("completed {stage}");
                }
            }
        }
        Ok(())
    })();
    fs::write(root.join("checks.txt"), &checks)?;
    fs::write(root.join("memory.csv"), memory)?;
    fs::write(root.join("responses.csv"), responses)?;
    result?;
    println!(
        "{} checks passed; evidence: {}",
        checks
            .lines()
            .filter(|line| line.starts_with("PASS "))
            .count(),
        root.display()
    );
    Ok(())
}
