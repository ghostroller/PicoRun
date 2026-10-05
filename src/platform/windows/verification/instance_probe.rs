//! Single-instance cold-start and foreground handoff, using only controlled processes.
use super::*;

const CLASS: &str = "PicoRun.Verification.InstanceSource.v1";
const LAUNCHER_CLASS: &str = "PicoRun.Native.v1";
const MUTEX: &str = "Local\\PicoRun.Native.v1";
const START_SECONDARY: u32 = 0x8001;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> Handle;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn PeekMessageW(message: *mut Message, hwnd: Hwnd, min: u32, max: u32, remove: u32) -> i32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn WindowFromPoint(point: Point) -> Hwnd;
    fn mouse_event(flags: u32, x: u32, y: u32, data: u32, extra: usize);
}

struct MutexHandle(Handle);
impl Drop for MutexHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}
fn existing_mutex() -> Option<MutexHandle> {
    let handle = unsafe { OpenMutexW(0x0010_0000, 0, wide(MUTEX).as_ptr()) };
    (!handle.is_null()).then_some(MutexHandle(handle))
}
fn pump() -> io::Result<()> {
    unsafe {
        let mut message = Message::default();
        while PeekMessageW(&mut message, null_mut(), 0, 0, 1) != 0 {
            if message.message == 0x12 {
                return Err(io::Error::other("controlled source window was closed"));
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}
fn wait_for(mut condition: impl FnMut() -> io::Result<bool>, seconds: u64) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        if condition()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::other("instance probe wait timed out"));
        }
        pump()?;
        thread::sleep(Duration::from_millis(2));
    }
}
fn successful_exit(child: &mut Running, seconds: u64) -> io::Result<()> {
    wait_for(
        || match child.0.try_wait()? {
            Some(status) if status.success() => Ok(true),
            Some(status) => Err(io::Error::other(format!(
                "controlled process exited {status}"
            ))),
            None => Ok(false),
        },
        seconds,
    )
}
fn args(data: &Path, source: &Path) -> Vec<std::ffi::OsString> {
    vec![
        "--hidden".into(),
        "--hotkey".into(),
        "Ctrl+Alt+F11".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
        "--source".into(),
        source.as_os_str().to_owned(),
        "--icons".into(),
        "off".into(),
    ]
}
fn owner(hwnd: Hwnd) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}
fn window(class: &str) -> Hwnd {
    unsafe { FindWindowW(wide(class).as_ptr(), null()) }
}

unsafe extern "system" fn source_proc(hwnd: Hwnd, message: u32, wp: usize, lp: isize) -> isize {
    match message {
        0x10 => {
            DestroyWindow(hwnd);
            0
        }
        0x2 => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wp, lp),
    }
}
struct SourceWindow(Hwnd);
impl SourceWindow {
    fn new() -> io::Result<Self> {
        unsafe {
            let name = wide(CLASS);
            let instance = GetModuleHandleW(null());
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
                0x80 | 0x8,
                name.as_ptr(),
                wide("PicoRun single-instance verification").as_ptr(),
                0x00cf0000,
                60,
                60,
                460,
                180,
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            if hwnd.is_null() {
                Err(io::Error::last_os_error())
            } else {
                Ok(Self(hwnd))
            }
        }
    }
}
impl Drop for SourceWindow {
    fn drop(&mut self) {
        if owner(self.0) == std::process::id() {
            unsafe { DestroyWindow(self.0) };
        }
    }
}
fn focus_source(hwnd: Hwnd) -> io::Result<()> {
    unsafe {
        ShowWindow(hwnd, 5);
        SetForegroundWindow(hwnd);
        if GetForegroundWindow() == hwnd {
            return Ok(());
        }
        // A blocked programmatic activation is retried by clicking only this
        // process's unobstructed client. Never send keys or clicks to other apps.
        let mut point = Point { x: 40, y: 70 };
        let mut previous = Point::default();
        if owner(hwnd) != std::process::id()
            || ClientToScreen(hwnd, &mut point) == 0
            || WindowFromPoint(point) != hwnd
            || GetCursorPos(&mut previous) == 0
            || SetCursorPos(point.x, point.y) == 0
        {
            return Err(io::Error::other(
                "controlled source cannot be safely activated",
            ));
        }
        if WindowFromPoint(point) != hwnd {
            SetCursorPos(previous.x, previous.y);
            return Err(io::Error::other("source click target changed"));
        }
        mouse_event(2, 0, 0, 0, 0);
        mouse_event(4, 0, 0, 0, 0);
        SetCursorPos(previous.x, previous.y);
    }
    wait_for(|| Ok(unsafe { GetForegroundWindow() == hwnd }), 3)
}

pub(super) fn source(data: PathBuf, apps: PathBuf, marker: PathBuf) -> io::Result<()> {
    let owned = SourceWindow::new()?;
    focus_source(owned.0)?;
    fs::write(&marker, "ready\n")?;
    let exe = std::env::current_exe()?.with_file_name("picorun.exe");
    let mut message = Message::default();
    loop {
        match unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } {
            -1 => return Err(io::Error::last_os_error()),
            0 => return Ok(()),
            _ if message.hwnd == owned.0 && message.message == START_SECONDARY => {
                if unsafe { GetForegroundWindow() } != owned.0 {
                    return Err(io::Error::other(
                        "source lost foreground before spawning secondary",
                    ));
                }
                let mut secondary = spawn(&exe, &args(&data, &apps))?;
                successful_exit(&mut secondary, 12)?;
                fs::write(&marker, "complete\n")?;
            }
            _ => unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            },
        }
    }
}

pub(super) fn run() -> io::Result<()> {
    if existing_mutex().is_some() || !window(LAUNCHER_CLASS).is_null() {
        return Err(io::Error::other(
            "quit the existing PicoRun before the instance probe",
        ));
    }
    let probe = std::env::current_exe()?;
    let exe = probe.with_file_name("picorun.exe");
    let root = std::env::current_dir()?.join("runtime/probe-instance");
    let session = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos()
    );
    let apps = root.join("source");
    fs::create_dir_all(&apps)?;
    for index in 0..10_000 {
        // Discovery accepts .exe entries; these empty fixtures are never opened.
        fs::write(apps.join(format!("Instance App {index:05}.exe")), [])?;
    }
    let mut checks = String::new();
    let result = (|| -> io::Result<()> {
        for round in 0..3 {
            let data = root.join(format!("data-{session}-{round}"));
            fs::create_dir_all(&data)?;
            fs::write(data.join("english-input.txt"), "off\n")?;
            let launch_args = args(&data, &apps);
            let mut primary = spawn(&exe, &launch_args)?;
            let mut mutex = None;
            wait_for(
                || {
                    if primary.0.try_wait()?.is_some() {
                        return Err(io::Error::other("primary exited during cold startup"));
                    }
                    mutex = existing_mutex();
                    Ok(mutex.is_some())
                },
                10,
            )?;
            expect(
                window(LAUNCHER_CLASS).is_null(),
                "cold-start secondary begins after mutex creation and before window creation",
                &mut checks,
            )?;
            let mut secondary = spawn(&exe, &launch_args)?;
            successful_exit(&mut secondary, 12)?;
            let (hwnd, _) = wait_window(&mut primary)?;
            wait_for(|| Ok(unsafe { IsWindowVisible(hwnd) != 0 }), 3)?;
            expect(
                owner(hwnd) == primary.0.id() && primary.0.try_wait()?.is_none(),
                "secondary exits successfully and shows the original hidden primary",
                &mut checks,
            )?;
            expect(
                cache::load(&data.join("apps-v1.bin"))?.entries().len() == 10_000,
                "cold startup indexed all 10000 controlled fixtures",
                &mut checks,
            )?;

            if round == 0 {
                unsafe { PostMessageW(hwnd, 0x6, 0, 0) };
                wait_for(|| Ok(unsafe { IsWindowVisible(hwnd) == 0 }), 3)?;
                let marker = root.join(format!("source-{}.txt", std::process::id()));
                let _ = fs::remove_file(&marker);
                let mut origin = spawn(
                    &probe,
                    &[
                        "--instance-source".into(),
                        data.as_os_str().to_owned(),
                        apps.as_os_str().to_owned(),
                        marker.as_os_str().to_owned(),
                    ],
                )?;
                wait_for(
                    || {
                        if origin.0.try_wait()?.is_some() {
                            return Err(io::Error::other("controlled foreground source exited"));
                        }
                        Ok(fs::read_to_string(&marker).is_ok_and(|text| text == "ready\n"))
                    },
                    5,
                )?;
                let source_window = window(CLASS);
                expect(
                    owner(source_window) == origin.0.id()
                        && unsafe { GetForegroundWindow() } == source_window,
                    "a different controlled process owns foreground before spawning secondary",
                    &mut checks,
                )?;
                unsafe { PostMessageW(source_window, START_SECONDARY, 0, 0) };
                wait_for(
                    || {
                        if origin.0.try_wait()?.is_some() {
                            return Err(io::Error::other(
                                "foreground source exited during handoff",
                            ));
                        }
                        Ok(fs::read_to_string(&marker).is_ok_and(|text| text == "complete\n"))
                    },
                    15,
                )?;
                wait_for(|| Ok(unsafe { GetForegroundWindow() == hwnd }), 3)?;
                expect(
                    unsafe { IsWindowVisible(hwnd) != 0 && GetForegroundWindow() == hwnd },
                    "secondary launched by foreground process transfers keyboard focus to primary",
                    &mut checks,
                )?;
                unsafe { PostMessageW(source_window, 0x10, 0, 0) };
                successful_exit(&mut origin, 5)?;
            }
            unsafe { PostMessageW(hwnd, 0x10, 0, 0) };
            successful_exit(&mut primary, 5)?;
            drop(mutex);
            expect(
                existing_mutex().is_none() && window(LAUNCHER_CLASS).is_null(),
                "normal exit releases the single-instance mutex and native window",
                &mut checks,
            )?;
        }
        Ok(())
    })();
    fs::write(root.join("checks.txt"), &checks)?;
    result?;
    println!(
        "Instance verification: {} checks; evidence {}",
        checks.lines().count(),
        root.display()
    );
    Ok(())
}
