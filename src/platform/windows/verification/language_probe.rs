//! Real native menus, live translation and persisted choice; only controlled fixtures are used.
use super::*;
use crate::i18n::{self, Language, Notice, Text};

#[link(name = "user32")]
unsafe extern "system" {
    fn IsWindow(hwnd: Hwnd) -> i32;
    fn GetMenuState(menu: Handle, item: u32, flags: u32) -> u32;
    fn GetMenuStringW(menu: Handle, item: u32, text: *mut u16, count: i32, flags: u32) -> i32;
}
fn hash(text: &str) -> isize {
    text.encode_utf16().fold(2166136261u32, |value, unit| {
        (value ^ u32::from(unit)).wrapping_mul(16777619)
    }) as isize
}
fn metric(hwnd: Hwnd, key: usize) -> isize {
    unsafe { SendMessageW(hwnd, 0x800e, key, 0) }
}
fn bounds(hwnd: Hwnd) -> [i32; 4] {
    let mut rect = Rect::default();
    unsafe { GetWindowRect(hwnd, &mut rect) };
    [rect.left, rect.top, rect.right, rect.bottom]
}
fn layout(hwnd: Hwnd) -> usize {
    let mut pid = 0;
    unsafe { GetKeyboardLayout(GetWindowThreadProcessId(hwnd, &mut pid)) as usize }
}
fn translated(hwnd: Hwnd, language: Language, checks: &mut String) -> io::Result<()> {
    i18n::set(language);
    expect(
        metric(hwnd, 0) == isize::from(language == Language::English),
        "active UI language matches choice",
        checks,
    )?;
    for (key, text) in [(2, Text::Help), (3, Text::Empty), (5, Text::Cue)] {
        expect(
            metric(hwnd, key) == hash(text.get(language)),
            "native footer / empty message / GDI input hint translated",
            checks,
        )?;
    }
    Ok(())
}
fn menu(
    hwnd: Hwnd,
    pid: u32,
    language: Language,
    choose: Option<char>,
    path: &Path,
    checks: &mut String,
) -> io::Result<()> {
    let mut retries = 0;
    let (popup, handle) = loop {
        let popup = open_tray_menu(hwnd, pid)?;
        let handle = unsafe { SendMessageW(popup, 0x1e1, 0, 0) } as Handle;
        let mut text = [0u16; 128];
        if unsafe { GetMenuStringW(handle, tray::SHOW, text.as_mut_ptr(), text.len() as i32, 0) }
            > 0
        {
            break (popup, handle);
        }
        unsafe {
            PostMessageW(popup, 0x100, 0x1b, 1);
        }
        retries += 1;
        if retries == 3 {
            return Err(io::Error::other(
                "native language menu was dismissed before inspection",
            ));
        }
        thread::sleep(Duration::from_millis(100));
    };
    for (id, key) in [
        (tray::SHOW, Text::Open),
        (tray::REFRESH, Text::Refresh),
        (tray::LIGHT, Text::Light),
        (tray::DARK, Text::Dark),
        (tray::ENGLISH, Text::EnglishInput),
        (tray::ICONS, Text::Icons),
        (tray::STARTUP, Text::Startup),
        (tray::EXIT, Text::Quit),
    ] {
        let mut text = [0u16; 128];
        let len = unsafe { GetMenuStringW(handle, id, text.as_mut_ptr(), text.len() as i32, 0) }
            .max(0) as usize;
        if String::from_utf16_lossy(&text[..len]) != key.get(language) {
            checks.push_str(&format!(
                "Menu {id}: expected {:?}; actual {:?}\n",
                key.get(language),
                String::from_utf16_lossy(&text[..len])
            ));
        }
        expect(
            String::from_utf16_lossy(&text[..len]) == key.get(language),
            "actual native menu label translated",
            checks,
        )?;
    }
    for (id, checked) in [
        (tray::CHINESE_UI, language == Language::Chinese),
        (tray::ENGLISH_UI, language == Language::English),
    ] {
        let state = unsafe { GetMenuState(handle, id, 0) };
        expect(
            state != u32::MAX && (state & 8 != 0) == checked,
            "language menu radio state is mutually exclusive",
            checks,
        )?;
    }
    screenshot(popup, path)?;
    unsafe {
        if let Some(key) = choose {
            PostMessageW(popup, 0x102, key as usize, 1);
        } else {
            PostMessageW(popup, 0x100, 0x1b, 1);
        }
    }
    let start = Instant::now();
    while unsafe { IsWindow(popup) } != 0 {
        if start.elapsed() > Duration::from_secs(3) {
            return Err(io::Error::other("language menu did not close"));
        }
        thread::sleep(Duration::from_millis(10));
    }
    barrier(hwnd);
    Ok(())
}
fn close(child: &mut Running, hwnd: Hwnd) -> io::Result<()> {
    unsafe { SendMessageW(hwnd, 0x10, 0, 0) };
    let start = Instant::now();
    while child.0.try_wait()?.is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    if !child.0.try_wait()?.is_some_and(|status| status.success()) {
        return Err(io::Error::other(
            "language probe child did not exit normally",
        ));
    }
    Ok(())
}
fn dialog(
    exe: &Path,
    data: &Path,
    language: Language,
    help: bool,
    checks: &mut String,
) -> io::Result<()> {
    let mut args = vec!["--data-dir".into(), data.as_os_str().to_owned()];
    if help {
        args.push("--help".into());
    } else {
        args.extend(["--icons".into(), "invalid".into()]);
    }
    let mut child = spawn(exe, &args)?;
    let hwnd = conflict_dialog(&mut child)?;
    let expected = if help { Text::Usage } else { Text::ArgIcons }.get(language);
    let mut found = false;
    let mut actual = Vec::new();
    let start = Instant::now();
    loop {
        // Finding a dialog precedes creation of its text/button controls. Wait for content.
        let mut control = null_mut();
        actual.clear();
        loop {
            control = unsafe { FindWindowExW(hwnd, control, null(), null()) };
            if control.is_null() {
                break;
            }
            let mut buffer = [0u16; 2048];
            let len = unsafe { get_control_text(control, buffer.as_mut_ptr(), buffer.len() as i32) }
                .max(0) as usize;
            let text = String::from_utf16_lossy(&buffer[..len]);
            found |= text.replace("\r\n", "\n") == expected;
            actual.push(text);
        }
        if found && unsafe { IsWindowVisible(hwnd) } != 0
            || start.elapsed() > Duration::from_secs(5)
        {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    if !found {
        checks.push_str(&format!(
            "Dialog expected {expected:?}; actual {actual:?}\n"
        ));
    }
    expect(
        found,
        "actual --help / invalid-argument dialog uses saved language",
        checks,
    )?;
    // Queue dismissal after initialization; synchronous WM_COMMAND can arrive too early.
    unsafe {
        PostMessageW(hwnd, 0x10, 0, 0);
    }
    let start = Instant::now();
    while child.0.try_wait()?.is_none() && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(10));
    }
    expect(
        child
            .0
            .try_wait()?
            .is_some_and(|status| status.success() == help),
        "help / argument-error exit code is correct",
        checks,
    )
}
pub(super) fn run() -> io::Result<()> {
    super::super::window::enable_dpi();
    let token = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let root = std::env::current_dir()?.join(format!("runtime/probe-language/{token}"));
    let source = root.join("应用 入口");
    let data = root.join("data");
    fs::create_dir_all(&source)?;
    fs::create_dir_all(&data)?;
    // Synthetic exe files are indexed but never launched by this probe.
    for i in 0..10 {
        fs::write(
            source.join(format!("微信 App {i:04}.exe")),
            b"synthetic; never executed",
        )?;
    }
    fs::write(data.join("theme.txt"), "dark\n")?;
    fs::write(data.join("english-input.txt"), "off\n")?;
    fs::write(data.join("icons.txt"), "off\n")?;
    let protected: Vec<_> = ["theme.txt", "english-input.txt", "icons.txt"]
        .into_iter()
        .map(|name| fs::read(data.join(name)))
        .collect::<io::Result<_>>()?;
    let exe = std::env::current_exe()?.with_file_name("picorun.exe");
    let args = [
        "--hidden".into(),
        "--hotkey".into(),
        "Ctrl+Alt+F11".into(),
        "--measure-icons".into(),
        "--hold-measurement-window".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
        "--source".into(),
        source.as_os_str().to_owned(),
    ];
    let mut checks = String::new();
    let result = (|| -> io::Result<()> {
        let mut child = spawn(&exe, &args)?;
        let (hwnd, edit) = wait_window(&mut child)?;
        barrier(hwnd);
        translated(hwnd, Language::Chinese, &mut checks)?;
        menu(
            hwnd,
            child.0.id(),
            Language::Chinese,
            Some('g'),
            &root.join("chinese-menu.bmp"),
            &mut checks,
        )?;
        translated(hwnd, Language::English, &mut checks)?;
        expect(
            fs::read_to_string(data.join("language.txt"))? == "en\n",
            "native menu saves English choice",
            &mut checks,
        )?;
        unsafe {
            SendMessageW(hwnd, 0x8001, 0, 0);
            set_control_text(edit, wide("wx").as_ptr());
            SendMessageW(hwnd, 0x8003, 0, 0);
            SendMessageW(hwnd, 0x800b, 0x28, 0);
            SendMessageW(edit, 0xb1, 1, 2);
        }
        let original = (bounds(hwnd), metric(hwnd, 4), layout(hwnd), unsafe {
            SendMessageW(edit, 0xb0, 0, 0)
        });
        let index = fs::read(data.join("apps-v1.bin"))?;
        for i in 0..20 {
            let language = if i % 2 == 0 {
                Language::Chinese
            } else {
                Language::English
            };
            unsafe {
                SendMessageW(
                    hwnd,
                    0x111,
                    if language == Language::Chinese {
                        tray::CHINESE_UI
                    } else {
                        tray::ENGLISH_UI
                    } as usize,
                    0,
                );
            }
            translated(hwnd, language, &mut checks)?;
            expect(
                ime_probe::text(edit) == "wx"
                    && unsafe { SendMessageW(edit, 0xb0, 0, 0) } == original.3,
                "live switch retains query and text selection",
                &mut checks,
            )?;
            expect(
                bounds(hwnd) == original.0
                    && metric(hwnd, 4) == original.1
                    && unsafe { SendMessageW(hwnd, 0x800c, 6, 0) } == 1,
                "live switch retains position, results, app names and selected row",
                &mut checks,
            )?;
            expect(
                layout(hwnd) == original.2,
                "UI language does not change keyboard layout",
                &mut checks,
            )?;
        }
        expect(
            fs::read(data.join("apps-v1.bin"))? == index,
            "language switching does not rebuild index",
            &mut checks,
        )?;
        for (i, name) in ["theme.txt", "english-input.txt", "icons.txt"]
            .into_iter()
            .enumerate()
        {
            expect(
                fs::read(data.join(name))? == protected[i],
                "other saved settings are unchanged",
                &mut checks,
            )?;
        }
        i18n::set(Language::English);
        let status = Notice::Catalog {
            count: 10,
            failed: 0,
            refreshed: false,
            cache_failed: false,
            source: None,
        };
        expect(
            metric(hwnd, 1) == hash(&status.to_string()),
            "existing catalog status is retranslated",
            &mut checks,
        )?;
        unsafe {
            set_control_text(edit, wide("unmatchedxyz").as_ptr());
            SendMessageW(hwnd, 0x8003, 0, 0);
            UpdateWindow(hwnd);
        }
        screenshot(hwnd, &root.join("english-empty.bmp"))?;
        unsafe {
            SendMessageW(hwnd, 0x111, tray::LIGHT as usize, 0);
            UpdateWindow(hwnd);
        }
        screenshot(hwnd, &root.join("english-light-empty.bmp"))?;
        unsafe {
            set_control_text(edit, wide("").as_ptr());
            SendMessageW(hwnd, 0x8003, 0, 0);
            UpdateWindow(edit);
            UpdateWindow(hwnd);
        }
        screenshot(hwnd, &root.join("english-light-hint.bmp"))?;
        unsafe {
            SendMessageW(hwnd, 0x111, tray::CHINESE_UI as usize, 0);
            UpdateWindow(edit);
            UpdateWindow(hwnd);
        }
        screenshot(hwnd, &root.join("chinese-light-hint.bmp"))?;
        unsafe {
            SendMessageW(hwnd, 0x111, tray::ENGLISH_UI as usize, 0);
        }

        // Missing fixture reports an error without attempting to open any executable.
        fs::remove_file(source.join("微信 App 0000.exe"))?;
        unsafe {
            set_control_text(edit, wide("微信 App 0000").as_ptr());
            SendMessageW(hwnd, 0x8003, 0, 0);
            SendMessageW(hwnd, 0x800b, 0x0d, 0);
        }
        expect(
            metric(hwnd, 1) == hash(Text::EntryMissing.get(Language::English)),
            "missing entry error is translated without launching an app",
            &mut checks,
        )?;
        unsafe {
            SendMessageW(hwnd, 0x111, tray::CHINESE_UI as usize, 0);
        }
        expect(
            metric(hwnd, 1) == hash(Text::EntryMissing.get(Language::Chinese)),
            "existing launch error retranslates after a live switch",
            &mut checks,
        )?;
        unsafe {
            SendMessageW(hwnd, 0x111, tray::ENGLISH_UI as usize, 0);
        }
        close(&mut child, hwnd)?;
        dialog(&exe, &data, Language::English, true, &mut checks)?;
        dialog(&exe, &data, Language::English, false, &mut checks)?;
        let mut child = spawn(&exe, &args)?;
        let (hwnd, _) = wait_window(&mut child)?;
        barrier(hwnd);
        translated(hwnd, Language::English, &mut checks)?;
        menu(
            hwnd,
            child.0.id(),
            Language::English,
            Some('c'),
            &root.join("english-menu.bmp"),
            &mut checks,
        )?;
        translated(hwnd, Language::Chinese, &mut checks)?;
        expect(
            fs::read_to_string(data.join("language.txt"))? == "zh-CN\n",
            "native menu saves Chinese choice",
            &mut checks,
        )?;
        close(&mut child, hwnd)?;
        dialog(&exe, &data, Language::Chinese, true, &mut checks)?;
        dialog(&exe, &data, Language::Chinese, false, &mut checks)?;
        // A blocked setting path must report a translated save failure without losing live choice.
        fs::remove_file(data.join("language.txt"))?;
        fs::create_dir(data.join("language.txt"))?;
        let mut child = spawn(&exe, &args)?;
        let (hwnd, _) = wait_window(&mut child)?;
        translated(hwnd, Language::Chinese, &mut checks)?;
        unsafe {
            SendMessageW(hwnd, 0x111, tray::ENGLISH_UI as usize, 0);
        }
        translated(hwnd, Language::English, &mut checks)?;
        expect(
            metric(hwnd, 1) == hash(Text::LanguageSave.get(Language::English)),
            "save failure reports in the chosen language",
            &mut checks,
        )?;
        close(&mut child, hwnd)?;
        Ok(())
    })();
    fs::write(root.join("checks.txt"), &checks)?;
    println!(
        "Language checks: {}\nEvidence: {}",
        checks
            .lines()
            .filter(|line| line.starts_with("PASS"))
            .count(),
        root.display()
    );
    result
}
