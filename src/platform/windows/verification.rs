//! Native verification helpers live here to keep all Win32 unsafe at the platform boundary.
mod english_probe;
mod icons_probe;
mod ime_probe;
use super::{discovery, ffi::*, tray, wide};
use crate::cache;
use std::{
    ffi::c_void,
    fs, io,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command},
    ptr::{null, null_mut},
    thread,
    time::{Duration, Instant},
};

#[repr(C)]
#[derive(Default)]
struct Memory {
    size: u32,
    faults: u32,
    peak_ws: usize,
    ws: usize,
    peak_paged: usize,
    paged: usize,
    peak_nonpaged: usize,
    nonpaged: usize,
    pagefile: usize,
    peak_pagefile: usize,
    private: usize,
}
#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}
impl FileTime {
    fn ticks(&self) -> u64 {
        u64::from(self.low) | u64::from(self.high) << 32
    }
}
#[link(name = "psapi")]
unsafe extern "system" {
    fn keybd_event(key: u8, scan: u8, flags: u32, extra: usize);
    fn GetCurrentProcess() -> Handle;
    fn GetProcessAffinityMask(
        process: Handle,
        process_mask: *mut usize,
        system_mask: *mut usize,
    ) -> i32;
    fn SetProcessAffinityMask(process: Handle, mask: usize) -> i32;
    fn GetProcessMemoryInfo(process: Handle, counters: *mut Memory, size: u32) -> i32;
}
fn actual_hotkey() {
    unsafe {
        keybd_event(0x11, 0, 0, 0);
        keybd_event(0x12, 0, 0, 0);
        keybd_event(0x7a, 0, 0, 0);
        keybd_event(0x7a, 0, 2, 0);
        keybd_event(0x12, 0, 2, 0);
        keybd_event(0x11, 0, 2, 0);
    }
    thread::sleep(Duration::from_millis(250));
}
fn conflict_dialog(child: &mut Running) -> io::Result<Hwnd> {
    let start = Instant::now();
    loop {
        let hwnd = unsafe { FindWindowW(wide("#32770").as_ptr(), wide("PicoRun").as_ptr()) };
        let mut pid = 0;
        if !hwnd.is_null() {
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut pid);
            }
        }
        if pid == child.0.id() {
            return Ok(hwnd);
        }
        if child.0.try_wait()?.is_some() || start.elapsed() > Duration::from_secs(5) {
            return Err(io::Error::other("conflict dialog was not displayed"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
pub fn pin_one_cpu() -> bool {
    unsafe {
        let process = GetCurrentProcess();
        let mut available = 0usize;
        let mut system = 0usize;
        GetProcessAffinityMask(process, &mut available, &mut system) != 0
            && SetProcessAffinityMask(process, available & available.wrapping_neg()) != 0
    }
}
pub fn current_memory(stage: &str) -> io::Result<String> {
    let mut line = String::new();
    sample(std::process::id(), stage, &mut line)?;
    Ok(line)
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn GetProcessTimes(
        process: Handle,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
    fn GlobalLock(handle: Handle) -> *mut c_void;
    fn GlobalUnlock(handle: Handle) -> i32;
    fn GlobalFree(handle: Handle) -> Handle;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(hwnd: Hwnd) -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, handle: Handle) -> Handle;
    fn CloseClipboard() -> i32;
    fn GetGuiResources(process: Handle, flags: u32) -> u32;
    fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
    fn GetDC(hwnd: Hwnd) -> Handle;
    fn ReleaseDC(hwnd: Hwnd, dc: Handle) -> i32;
    fn PrintWindow(hwnd: Hwnd, dc: Handle, flags: u32) -> i32;
    fn ClientToScreen(hwnd: Hwnd, point: *mut Point) -> i32;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(dc: Handle) -> Handle;
    fn DeleteDC(dc: Handle) -> i32;
    fn BitBlt(
        destination: Handle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        source: Handle,
        source_x: i32,
        source_y: i32,
        operation: u32,
    ) -> i32;
    fn CreateDIBSection(
        dc: Handle,
        info: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut c_void,
        section: Handle,
        offset: u32,
    ) -> Handle;
}
#[repr(C)]
struct BitmapInfo {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    xppm: i32,
    yppm: i32,
    used: u32,
    important: u32,
    colors: [u32; 1],
}
struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn spawn(exe: &Path, args: &[std::ffi::OsString]) -> io::Result<Running> {
    let mut command = Command::new(exe);
    command.args(args).creation_flags(0x08000000);
    Ok(Running(command.spawn()?))
}
fn wait_window(child: &mut Running) -> io::Result<(Hwnd, Hwnd)> {
    let start = Instant::now();
    loop {
        let hwnd = unsafe { FindWindowW(wide("PicoRun.Native.v1").as_ptr(), null()) };
        let mut pid = 0;
        if !hwnd.is_null() {
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut pid);
            }
        }
        if pid == child.0.id() {
            let edit = unsafe { FindWindowExW(hwnd, null_mut(), wide("EDIT").as_ptr(), null()) };
            if !edit.is_null() {
                return Ok((hwnd, edit));
            }
        }
        if child.0.try_wait()?.is_some() {
            return Err(io::Error::other("launcher exited before creating window"));
        }
        if start.elapsed() > Duration::from_secs(15) {
            return Err(io::Error::other(
                "window creation timed out (check hotkey conflict or existing launcher)",
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
}
fn sample(pid: u32, stage: &str, report: &mut String) -> io::Result<u64> {
    unsafe {
        let process = OpenProcess(0x410, 0, pid);
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut memory = Memory {
            size: std::mem::size_of::<Memory>() as u32,
            ..Memory::default()
        };
        let mut creation = FileTime::default();
        let mut exit = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        let okay = GetProcessMemoryInfo(process, &mut memory, std::mem::size_of::<Memory>() as u32)
            != 0
            && GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) != 0;
        let gdi = GetGuiResources(process, 0);
        let user_handles = GetGuiResources(process, 1);
        CloseHandle(process);
        if !okay {
            return Err(io::Error::last_os_error());
        }
        report.push_str(&format!(
            "{stage},{},{},{},{},{gdi},{user_handles},{}\n",
            memory.private,
            memory.ws,
            memory.peak_pagefile,
            memory.peak_ws,
            kernel.ticks() + user.ticks()
        ));
        Ok(kernel.ticks() + user.ticks())
    }
}
fn clipboard(hwnd: Hwnd, text: &str) -> io::Result<()> {
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return Err(io::Error::last_os_error());
        }
        let text = wide(text);
        let allocation = GlobalAlloc(2, text.len() * 2);
        if allocation.is_null() {
            CloseClipboard();
            return Err(io::Error::last_os_error());
        }
        let data = GlobalLock(allocation).cast::<u16>();
        if data.is_null() {
            GlobalFree(allocation);
            CloseClipboard();
            return Err(io::Error::last_os_error());
        }
        std::ptr::copy_nonoverlapping(text.as_ptr(), data, text.len());
        GlobalUnlock(allocation);
        EmptyClipboard();
        if SetClipboardData(13, allocation).is_null() {
            GlobalFree(allocation);
            CloseClipboard();
            return Err(io::Error::last_os_error());
        }
        CloseClipboard(); // System now owns allocation.
    }
    Ok(())
}
fn create_shortcut(
    path: &Path,
    target: &Path,
    arguments: &str,
    directory: &Path,
) -> io::Result<()> {
    let (link, persist) = discovery::shell_link()?;
    unsafe {
        for (index, value) in [
            (20, wide(target)),
            (11, wide(arguments)),
            (9, wide(directory)),
        ] {
            let set: unsafe extern "system" fn(*mut c_void, *const u16) -> i32 =
                std::mem::transmute(link.method(index));
            if set(link.0, value.as_ptr()) < 0 {
                return Err(io::Error::other("shortcut set failed"));
            }
        }
        let save: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32 =
            std::mem::transmute(persist.method(6));
        if save(persist.0, wide(path).as_ptr(), 1) < 0 {
            return Err(io::Error::other("shortcut save failed"));
        }
    }
    Ok(())
}
fn screenshot(hwnd: Hwnd, path: &Path) -> io::Result<()> {
    capture(hwnd, path, false)
}
fn capture(hwnd: Hwnd, path: &Path, onscreen: bool) -> io::Result<()> {
    unsafe {
        if onscreen && GetForegroundWindow() != hwnd {
            return Err(io::Error::other(
                "screen capture requires own launcher foreground",
            ));
        }
        let mut rect = Rect::default();
        GetClientRect(hwnd, &mut rect);
        let info = BitmapInfo {
            size: 40,
            width: rect.right,
            height: -rect.bottom,
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: 0,
            xppm: 0,
            yppm: 0,
            used: 0,
            important: 0,
            colors: [0],
        };
        let source = if onscreen { null_mut() } else { hwnd };
        let screen = GetDC(source);
        let dc = CreateCompatibleDC(screen);
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(screen, &info, 0, &mut bits, null_mut(), 0);
        if bitmap.is_null() || dc.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            if !dc.is_null() {
                DeleteDC(dc);
            }
            ReleaseDC(source, screen);
            return Err(io::Error::last_os_error());
        }
        let old = SelectObject(dc, bitmap);
        let captured = if onscreen {
            let mut origin = Point::default();
            ClientToScreen(hwnd, &mut origin) != 0
                && BitBlt(
                    dc,
                    0,
                    0,
                    rect.right,
                    rect.bottom,
                    screen,
                    origin.x,
                    origin.y,
                    0x00cc0020,
                ) != 0
        } else {
            PrintWindow(hwnd, dc, 1) != 0
        };
        let len = rect.right as usize * rect.bottom as usize * 4;
        let mut bytes = Vec::with_capacity(54 + len);
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&((54 + len) as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&54u32.to_le_bytes());
        bytes.extend_from_slice(std::slice::from_raw_parts(
            (&info as *const BitmapInfo).cast::<u8>(),
            40,
        ));
        bytes.extend_from_slice(std::slice::from_raw_parts(bits.cast::<u8>(), len));
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(source, screen);
        if !captured {
            return Err(io::Error::other("window capture failed"));
        }
        fs::write(path, bytes)
    }
}
fn expect(condition: bool, description: &str, checks: &mut String) -> io::Result<()> {
    if !condition {
        return Err(io::Error::other(format!("check failed: {description}")));
    }
    checks.push_str(&format!("PASS {description}\n"));
    Ok(())
}
fn barrier(hwnd: Hwnd) {
    unsafe {
        SendMessageW(hwnd, 0, 0, 0);
    }
    thread::sleep(Duration::from_millis(20));
}
// Cross-process Edit text requires marshalled WM_SETTEXT/WM_GETTEXT. The similarly
// named convenience APIs only access another process's window caption.
unsafe fn set_control_text(hwnd: Hwnd, text: *const u16) -> isize {
    SendMessageW(hwnd, 0xc, 0, text as isize)
}
unsafe fn get_control_text(hwnd: Hwnd, text: *mut u16, length: i32) -> isize {
    SendMessageW(hwnd, 0xd, length as usize, text as isize)
}
fn wait_marker(path: &Path) -> io::Result<String> {
    let start = Instant::now();
    loop {
        if let Ok(text) = fs::read_to_string(path) {
            if text.ends_with('\n') && text.contains("args=") {
                return Ok(text);
            }
        }
        if start.elapsed() > Duration::from_secs(5) {
            return Err(io::Error::other("controlled child marker timed out"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
fn tray_rect(hwnd: Hwnd) -> Option<Rect> {
    let id = NotifyIconIdentifier {
        size: std::mem::size_of::<NotifyIconIdentifier>() as u32,
        hwnd,
        id: tray::ICON_ID,
        guid: Guid {
            a: 0,
            b: 0,
            c: 0,
            d: [0; 8],
        },
    };
    let mut rect = Rect::default();
    (unsafe { Shell_NotifyIconGetRect(&id, &mut rect) } >= 0).then_some(rect)
}
fn open_tray_menu(hwnd: Hwnd, pid: u32) -> io::Result<Hwnd> {
    let event = (tray::ICON_ID << 16 | 0x7b) as isize;
    unsafe {
        PostMessageW(hwnd, tray::CALLBACK, 600 | 400 << 16, event);
    }
    let start = Instant::now();
    loop {
        let menu = unsafe { FindWindowW(wide("#32768").as_ptr(), null()) };
        let mut owner = 0;
        unsafe {
            GetWindowThreadProcessId(menu, &mut owner);
        }
        if !menu.is_null() && owner == pid {
            thread::sleep(Duration::from_millis(150)); // Allow the native popup's nested loop to become ready.
            return Ok(menu);
        }
        if start.elapsed() > Duration::from_secs(2) {
            return Err(io::Error::other("tray popup menu did not open"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
pub fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--controlled-child") {
        let marker = PathBuf::from(
            args.get(1)
                .ok_or_else(|| io::Error::other("missing marker"))?,
        );
        let text = format!(
            "cwd={}\nargs={}\n",
            std::env::current_dir()?.display(),
            args[2..]
                .iter()
                .map(|a| a.to_string_lossy())
                .collect::<Vec<_>>()
                .join("|")
        );
        fs::write(marker, text)?;
        return Ok(());
    }
    let real = args.iter().any(|a| a == "--real");
    let baseline = args.iter().any(|a| a == "--baseline");
    let installed_ime = args.iter().any(|a| a == "--ime-real");
    let icons = args.iter().any(|a| a == "--icons");
    super::window::enable_dpi();
    let root = std::env::current_dir()?.join("runtime").join(if icons {
        "probe-icons-controlled"
    } else if real {
        "probe-real"
    } else {
        "probe-controlled"
    });
    fs::create_dir_all(&root)?;
    let exe = if baseline {
        std::env::current_dir()?.join("runtime/tray-theme-baseline/picorun.exe")
    } else {
        std::env::current_exe()?.with_file_name("picorun.exe")
    };
    let data = root.join("data");
    fs::create_dir_all(&data)?;
    let source = root.join("应用 入口");
    fs::create_dir_all(&source)?;
    let working = root.join("工作 目录");
    fs::create_dir_all(&working)?;
    let marker = root.join("child.txt");
    let child_exe = root.join("测试 程序.exe");
    let initialized = unsafe { CoInitializeEx(null_mut(), 2) >= 0 };
    if !initialized {
        return Err(io::Error::other("probe COM init failed"));
    }
    if !real {
        fs::copy(std::env::current_exe()?, &child_exe)?;
        for index in 0..500 {
            let title = match index {
                0 => "微信".into(),
                1 => "记事本".into(),
                2 => "重庆银行".into(),
                3 => "腾讯QQ".into(),
                _ => format!("Synthetic App {index:04}"),
            };
            let arguments = format!(
                "--controlled-child \"{}\" \"参数 with spaces\" {index}",
                marker.display()
            );
            create_shortcut(
                &source.join(format!("{title}.lnk")),
                &child_exe,
                &arguments,
                &working,
            )?;
        }
        // A document shortcut must be excluded.
        let document = root.join("document.txt");
        fs::write(&document, "not an application")?;
        create_shortcut(&source.join("文档.lnk"), &document, "", &working)?;
    }
    unsafe {
        CoUninitialize();
    }
    let mut launch_args = vec![
        "--hidden".into(),
        "--hotkey".into(),
        "Ctrl+Alt+F11".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
    ];
    if !real {
        launch_args.extend(["--source".into(), source.as_os_str().to_owned()]);
    }
    if icons {
        launch_args.push("--measure-icons".into());
    }
    let mut timings = Vec::new();
    let mut memory = String::from("stage,private_bytes,working_set_bytes,peak_commit_bytes,peak_working_set_bytes,gdi_handles,user_handles,cpu_100ns\n");
    let mut checks = String::new();
    let cache_path = data.join("apps-v1.bin");
    let theme_path = data.join("theme.txt");
    let input_path = data.join("english-input.txt");
    let _ = fs::remove_file(&input_path);
    if icons {
        let _ = fs::remove_file(data.join("icons.txt"));
    }
    fs::write(&theme_path, "dark\n")?;
    for run in 0..5 {
        if run == 0 {
            let _ = fs::remove_file(&cache_path);
        }
        let start = Instant::now();
        let mut child = spawn(&exe, &launch_args)?;
        let (hwnd, edit) = wait_window(&mut child)?;
        if icons && run == 0 && !real {
            icons_probe::features(hwnd, edit, child.0.id(), &data, &root, &mut checks)?;
        }
        let startup = start.elapsed().as_secs_f64() * 1000.0;
        thread::sleep(Duration::from_millis(100));
        if !baseline {
            expect(
                tray_rect(hwnd).is_some(),
                "notification icon registered with Windows Shell",
                &mut checks,
            )?;
        }
        sample(child.0.id(), &format!("run{run}_hidden"), &mut memory)?;
        let start = Instant::now();
        unsafe {
            SendMessageW(hwnd, 0x8001, 0, 0);
            SendMessageW(hwnd, 0xf, 0, 0);
        }
        let show_ms = start.elapsed().as_secs_f64() * 1000.0;
        sample(child.0.id(), &format!("run{run}_shown"), &mut memory)?;
        for _ in 0..20 {
            unsafe {
                set_control_text(edit, wide("wx").as_ptr());
                SendMessageW(hwnd, 0x8003, 0, 0);
                SendMessageW(hwnd, 0xf, 0, 0);
            }
        }
        let mut input_samples = Vec::new();
        for i in 0..120 {
            let start = Instant::now();
            unsafe {
                set_control_text(
                    edit,
                    wide(["wx", "jsb", "cqyh", "腾讯qq", "no-match", ""][i % 6]).as_ptr(),
                );
                SendMessageW(hwnd, 0x8003, 0, 0);
                SendMessageW(hwnd, 0xf, 0, 0);
            }
            input_samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        input_samples.sort_by(f64::total_cmp);
        timings.push(format!("run={run} cache_cold={} startup_ms={startup:.4} show_send_and_paint_ms={show_ms:.4} input_n=120 input_p50_ms={:.4} input_p95_ms={:.4}\n", run==0, input_samples[59], input_samples[113]));
        sample(child.0.id(), &format!("run{run}_input"), &mut memory)?;
        unsafe {
            SendMessageW(hwnd, 0x8002, 0, 0);
        }
        sample(child.0.id(), &format!("run{run}_refreshed"), &mut memory)?;
        unsafe {
            SendMessageW(edit, 0x100, 0x1b, 0);
        }
        barrier(hwnd);
        thread::sleep(Duration::from_secs(1)); // Settle outstanding native input/IME work before idle CPU.
        let cpu_before = sample(child.0.id(), &format!("run{run}_idle_start"), &mut memory)?;
        thread::sleep(Duration::from_secs(5));
        let cpu_after = sample(child.0.id(), &format!("run{run}_idle_end"), &mut memory)?;
        checks.push_str(&format!(
            "idle run={run} cpu_ms={:.3} interval_ms=5000\n",
            (cpu_after - cpu_before) as f64 / 10000.0
        ));
        if run == 0 && !real && !baseline {
            unsafe {
                SendMessageW(
                    hwnd,
                    tray::CALLBACK,
                    0,
                    (tray::ICON_ID << 16 | 0x400) as isize,
                );
            }
            expect(
                unsafe { IsWindowVisible(hwnd) != 0 },
                "version 4 tray selection shows launcher",
                &mut checks,
            )?;
            unsafe {
                set_control_text(edit, wide("cqyh").as_ptr());
                SendMessageW(hwnd, 0x8003, 0, 0);
            }
            let menu = open_tray_menu(hwnd, child.0.id())?;
            screenshot(menu, &root.join("tray-menu.bmp"))?;
            unsafe {
                PostMessageW(menu, 0x102, 'l' as usize, 1);
            } // Select through the native popup loop.
            barrier(hwnd);
            expect(
                fs::read_to_string(&theme_path)?.trim() == "light",
                "native popup accelerator message selects and saves light theme",
                &mut checks,
            )?;
            let mut query = [0u16; 16];
            unsafe {
                get_control_text(edit, query.as_mut_ptr(), 16);
                ShowWindow(hwnd, 5);
                SendMessageW(hwnd, 0xf, 0, 0);
            }
            expect(
                query[..5] == [99, 113, 121, 104, 0],
                "theme switch preserves query",
                &mut checks,
            )?;
            screenshot(hwnd, &root.join("light.bmp"))?;
            unsafe {
                SendMessageW(hwnd, 0x111, tray::DARK as usize, 0);
                SendMessageW(hwnd, 0xf, 0, 0);
            }
            screenshot(hwnd, &root.join("dark.bmp"))?;
            sample(child.0.id(), "theme_stress_before", &mut memory)?;
            for i in 0..100 {
                unsafe {
                    SendMessageW(
                        hwnd,
                        0x111,
                        if i % 2 == 0 { tray::LIGHT } else { tray::DARK } as usize,
                        0,
                    );
                }
            }
            for _ in 0..20 {
                open_tray_menu(hwnd, child.0.id())?;
                unsafe {
                    SendMessageW(hwnd, 0x1f, 0, 0);
                } // WM_CANCELMODE closes this test's native popup.
                barrier(hwnd);
            }
            let taskbar_created =
                unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
            for _ in 0..20 {
                unsafe {
                    SendMessageW(hwnd, taskbar_created, 0, 0);
                }
            }
            expect(
                tray_rect(hwnd).is_some(),
                "20 simulated TaskbarCreated notifications restore icon",
                &mut checks,
            )?;
            sample(child.0.id(), "theme_stress_after", &mut memory)?;
            expect(
                cache::load(&cache_path)?.entries().len() == 500,
                "500 application shortcuts, document excluded",
                &mut checks,
            )?;
            // Delete operates on actual Edit text; activation proves the updated result was selected.
            let combination = super::Hotkey::parse("Ctrl+Alt+F11")?;
            expect(
                unsafe {
                    RegisterHotKey(null_mut(), 77, combination.modifiers, combination.key) == 0
                },
                "global hotkey registration reports duplicate conflict",
                &mut checks,
            )?;
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
                set_control_text(edit, wide("wxx").as_ptr());
                SendMessageW(edit, 0xb1, 2, 2);
                SendMessageW(edit, 0x100, 0x2e, 0x01530001);
            }
            barrier(hwnd);
            let mut text = [0u16; 16];
            unsafe {
                get_control_text(edit, text.as_mut_ptr(), 16);
            }
            expect(
                text[0..3] == [119, 120, 0],
                &format!(
                    "Delete changes native Edit text to wx (actual {:?})",
                    String::from_utf16_lossy(
                        &text[..text.iter().position(|&c| c == 0).unwrap_or(16)]
                    )
                ),
                &mut checks,
            )?;
            let _ = fs::remove_file(&marker);
            unsafe {
                SendMessageW(edit, 0x100, 0x0d, 0);
            }
            let launched = wait_marker(&marker)?;
            expect(launched.contains("参数 with spaces|0") && launched.contains(&format!("cwd={}", working.display())), "original shortcut preserves Unicode/space paths, quoted arguments and working directory", &mut checks)?;
            sample(child.0.id(), "after_first_shell_launch", &mut memory)?;
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
            }
            clipboard(hwnd, "jsb")?;
            unsafe {
                set_control_text(edit, wide("").as_ptr());
                SendMessageW(edit, 0x302, 0, 0);
            }
            let _ = fs::remove_file(&marker);
            unsafe {
                SendMessageW(edit, 0x100, 0x0d, 0);
            }
            expect(
                wait_marker(&marker)?.contains("参数 with spaces|1"),
                "WM_PASTE then immediate Enter opens current candidate",
                &mut checks,
            )?;
            sample(child.0.id(), "after_paste_launch", &mut memory)?;
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
                set_control_text(edit, wide("jsb").as_ptr());
                SendMessageW(edit, 0xb1, 0, -1);
                SendMessageW(edit, 0x301, 0, 0);
                set_control_text(edit, wide("").as_ptr());
                SendMessageW(edit, 0x302, 0, 0);
            }
            let _ = fs::remove_file(&marker);
            unsafe {
                SendMessageW(edit, 0x100, 0x0d, 1);
            }
            expect(
                wait_marker(&marker)?.contains("参数 with spaces|1"),
                "native Copy/Paste selects expected app",
                &mut checks,
            )?;
            unsafe {
                set_control_text(edit, wide("Synthetic App 000").as_ptr());
                SendMessageW(edit, 0x100, 0x28, 1);
                SendMessageW(edit, 0x100, 0x28, 1);
                SendMessageW(edit, 0x100, 0x26, 1);
                SendMessageW(hwnd, 0x111, tray::LIGHT as usize, 0);
                SendMessageW(hwnd, 0x111, tray::DARK as usize, 0);
            }
            let _ = fs::remove_file(&marker);
            unsafe {
                SendMessageW(edit, 0x100, 0x0d, 1);
            }
            expect(
                wait_marker(&marker)?.contains("参数 with spaces|5"),
                "Down/Down/Up and theme switches preserve second stable result",
                &mut checks,
            )?;
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
                set_control_text(edit, wide("zqxvnomatch").as_ptr());
            }
            let _ = fs::remove_file(&marker);
            unsafe {
                SendMessageW(edit, 0x100, 0x0d, 1);
            }
            thread::sleep(Duration::from_millis(150));
            expect(
                !marker.exists(),
                "no-match Enter starts no process",
                &mut checks,
            )?;
            english_probe::features(
                hwnd,
                edit,
                child.0.id(),
                &input_path,
                &root,
                installed_ime,
                &mut checks,
            )?;
            ime_probe::injected(hwnd, edit, &marker, &mut checks)?;
            sample(child.0.id(), "after_injected_ime_regressions", &mut memory)?;
            fs::write(root.join("ime-checks-progress.txt"), &checks)?;
            if installed_ime {
                ime_probe::installed(hwnd, edit, &marker, &root, &mut checks)?;
                sample(child.0.id(), "after_installed_ime", &mut memory)?;
                fs::write(root.join("ime-checks-progress.txt"), &checks)?;
            }
            let shortcut = source.join("微信.lnk");
            let saved_shortcut = fs::read(&shortcut)?;
            fs::remove_file(&shortcut)?;
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
                set_control_text(edit, wide("wx").as_ptr());
            }
            let _ = fs::remove_file(&marker);
            unsafe {
                SendMessageW(edit, 0x100, 0x0d, 1);
            }
            expect(
                !marker.exists() && unsafe { IsWindowVisible(hwnd) != 0 },
                "stale shortcut gives visible recoverable error",
                &mut checks,
            )?;
            unsafe {
                SendMessageW(hwnd, 0x8002, 0, 0);
            }
            expect(
                cache::load(&cache_path)?.entries().len() == 499,
                "refresh drops removed entry",
                &mut checks,
            )?;
            fs::write(shortcut, saved_shortcut)?;
            unsafe {
                SendMessageW(hwnd, 0x8002, 0, 0);
            }
            expect(
                cache::load(&cache_path)?.entries().len() == 500,
                "refresh discovers restored entry",
                &mut checks,
            )?;
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
                set_control_text(edit, wide("cqyh").as_ptr());
                SendMessageW(hwnd, 0x8003, 0, 0);
                SendMessageW(hwnd, 0xf, 0, 0);
            }
            screenshot(hwnd, &root.join("window.bmp"))?;
            sample(child.0.id(), "after_screenshot_and_recovery", &mut memory)?;
            let mut second = spawn(&exe, &launch_args)?;
            let start = Instant::now();
            while second.0.try_wait()?.is_none() && start.elapsed() < Duration::from_secs(2) {
                thread::sleep(Duration::from_millis(5));
            }
            expect(
                second.0.try_wait()?.is_some_and(|s| s.success()),
                "second instance exits and signals existing window",
                &mut checks,
            )?;
            sample(child.0.id(), "after_second_instance", &mut memory)?;
            actual_hotkey();
            expect(
                unsafe { IsWindowVisible(hwnd) == 0 },
                "actual Ctrl+Alt+F11 hides visible window",
                &mut checks,
            )?;
            actual_hotkey();
            expect(
                unsafe { IsWindowVisible(hwnd) != 0 },
                "actual Ctrl+Alt+F11 shows hidden window",
                &mut checks,
            )?;
            // Save a valid cache snapshot, corrupt it after process exit, verify recovery on next run.
            sample(child.0.id(), "stress_before", &mut memory)?;
            for i in 0..1000 {
                unsafe {
                    set_control_text(
                        edit,
                        wide(["wx", "jsb", "cqyh", "", "no-match"][i % 5]).as_ptr(),
                    );
                    SendMessageW(hwnd, 0x8003, 0, 0);
                    SendMessageW(hwnd, 0xf, 0, 0);
                }
            }
            sample(child.0.id(), "stress_after_1000", &mut memory)?;
            unsafe {
                SendMessageW(hwnd, 0x6, 0, 0);
            }
            expect(
                unsafe { IsWindowVisible(hwnd) == 0 },
                "deactivation hides window",
                &mut checks,
            )?;
        }
        unsafe {
            if baseline {
                SendMessageW(hwnd, 0x10, 0, 0);
            } else {
                SendMessageW(hwnd, 0x111, tray::EXIT as usize, 0);
            }
        }
        expect(
            child.0.wait()?.success(),
            if baseline {
                "baseline WM_CLOSE exits cleanly"
            } else {
                "tray Exit command exits cleanly"
            },
            &mut checks,
        )?;
        if !baseline {
            expect(
                tray_rect(hwnd).is_none(),
                "notification icon removed on exit",
                &mut checks,
            )?;
        }
        let combination = super::Hotkey::parse("Ctrl+Alt+F11")?;
        expect(
            unsafe { RegisterHotKey(null_mut(), 77, combination.modifiers, combination.key) != 0 },
            "hotkey released on exit",
            &mut checks,
        )?;
        unsafe {
            UnregisterHotKey(null_mut(), 77);
        }
        if run == 0 {
            fs::write(&cache_path, "broken-cache")?;
        }
        if run == 1 {
            expect(
                cache::load(&cache_path).is_ok(),
                "corrupted cache recovers by discovery",
                &mut checks,
            )?;
        }
    }
    if !real && !baseline {
        fs::write(&theme_path, "light\n")?;
        super::settings::save_english(&input_path, true)?;
        let mut restarted = spawn(&exe, &launch_args)?;
        let (hwnd, _) = wait_window(&mut restarted)?;
        english_probe::restarted(hwnd, &mut checks)?;
        if icons {
            expect(
                unsafe { SendMessageW(hwnd, 0x8006, 17, 0) } == 1,
                "restart restores saved icon setting",
                &mut checks,
            )?;
        }
        unsafe {
            SendMessageW(hwnd, 0x8001, 0, 0);
            SendMessageW(hwnd, 0xf, 0, 0);
        }
        screenshot(hwnd, &root.join("light-restarted.bmp"))?;
        // Choosing the already active palette must save it as well (including --theme overrides).
        unsafe {
            SendMessageW(hwnd, 0x111, tray::LIGHT as usize, 0);
            SendMessageW(hwnd, 0x111, tray::EXIT as usize, 0);
        }
        expect(
            restarted.0.wait()?.success(),
            "restart with saved light theme exits cleanly",
            &mut checks,
        )?;
        let combination = super::Hotkey::parse("Ctrl+Alt+F11")?;
        if unsafe { RegisterHotKey(null_mut(), 77, combination.modifiers, combination.key) } == 0 {
            return Err(io::Error::other("cannot reserve conflict test hotkey"));
        }
        let mut conflicted = spawn(&exe, &launch_args)?;
        let dialog = conflict_dialog(&mut conflicted)?;
        unsafe {
            SendMessageW(dialog, 0x111, 1, 0);
            UnregisterHotKey(null_mut(), 77);
        }
        expect(
            !conflicted.0.wait()?.success(),
            "occupied hotkey shows error dialog and exits",
            &mut checks,
        )?;
    }
    let prefix = if baseline { "baseline-" } else { "" };
    fs::write(root.join(format!("{prefix}memory.csv")), memory)?;
    fs::write(root.join(format!("{prefix}timings.txt")), timings.join(""))?;
    fs::write(root.join(format!("{prefix}checks.txt")), &checks)?;
    println!("Native probe completed: {}\n{checks}", root.display());
    Ok(())
}
