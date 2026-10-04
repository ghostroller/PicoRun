//! Sample screen pixels concurrently with physical native Edit mouse selection.
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[link(name = "user32")]
unsafe extern "system" {
    fn GetCursorPos(point: *mut Point) -> i32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn WindowFromPoint(point: Point) -> Hwnd;
    fn mouse_event(flags: u32, x: u32, y: u32, data: u32, extra: usize);
}
struct Mouse {
    original: Point,
    hwnd: Hwnd,
    pressed: bool,
}
impl Drop for Mouse {
    fn drop(&mut self) {
        unsafe {
            if self.pressed {
                mouse_event(4, 0, 0, 0, 0);
                barrier(self.hwnd);
            }
            if GetForegroundWindow() == self.hwnd {
                SetCursorPos(self.original.x, self.original.y);
            }
        }
    }
}
struct Screen {
    source: Handle,
    dc: Handle,
    bitmap: Handle,
    old: Handle,
    pixels: *mut u32,
    origin: Point,
    width: i32,
    height: i32,
}
impl Screen {
    fn new(edit: Hwnd) -> io::Result<Self> {
        unsafe {
            let mut rect = Rect::default();
            GetClientRect(edit, &mut rect);
            let mut result = Self {
                source: GetDC(null_mut()),
                dc: null_mut(),
                bitmap: null_mut(),
                old: null_mut(),
                pixels: null_mut(),
                origin: Point::default(),
                width: rect.right,
                height: rect.bottom,
            };
            if result.source.is_null()
                || result.width <= 0
                || result.height <= 0
                || ClientToScreen(edit, &mut result.origin) == 0
            {
                return Err(io::Error::last_os_error());
            }
            result.dc = CreateCompatibleDC(result.source);
            let info = BitmapInfo {
                size: 40,
                width: result.width,
                height: -result.height,
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
            let mut pixels = null_mut();
            result.bitmap = CreateDIBSection(
                result.source,
                (&info as *const BitmapInfo).cast(),
                0,
                &mut pixels,
                null_mut(),
                0,
            );
            if result.dc.is_null() || result.bitmap.is_null() || pixels.is_null() {
                return Err(io::Error::last_os_error());
            }
            result.old = SelectObject(result.dc, result.bitmap);
            if result.old.is_null() || result.old as isize == -1 {
                return Err(io::Error::last_os_error());
            }
            result.pixels = pixels.cast();
            Ok(result)
        }
    }
    fn colors(&self, neutral: u32, blue: u32) -> io::Result<(usize, usize)> {
        unsafe {
            if BitBlt(
                self.dc,
                0,
                0,
                self.width,
                self.height,
                self.source,
                self.origin.x,
                self.origin.y,
                0x00cc0020,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            GdiFlush();
            let pixels =
                std::slice::from_raw_parts(self.pixels, (self.width * self.height) as usize);
            let mut counts = (0, 0);
            for &pixel in pixels {
                counts.0 += usize::from(pixel & 0xffffff == neutral);
                counts.1 += usize::from(pixel & 0xffffff == blue);
            }
            Ok(counts)
        }
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        unsafe {
            GdiFlush();
            if !self.old.is_null() && self.old as isize != -1 {
                SelectObject(self.dc, self.old);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
            if !self.source.is_null() {
                ReleaseDC(null_mut(), self.source);
            }
        }
    }
}
fn owned(hwnd: Hwnd, edit: Hwnd, point: Point) -> io::Result<()> {
    let hit = unsafe { WindowFromPoint(point) };
    // Edge scrolling moves beyond the Edit into our own panel padding. Native
    // capture still owns those moves; reject foreign windows or a lost foreground.
    let mut info = GuiThreadInfo {
        size: std::mem::size_of::<GuiThreadInfo>() as u32,
        ..Default::default()
    };
    let captured = hit == hwnd
        && unsafe {
            let tid = GetWindowThreadProcessId(hwnd, null_mut());
            GetGUIThreadInfo(tid, &mut info) != 0 && info.capture == edit
        };
    if unsafe { GetForegroundWindow() != hwnd } || hit != edit && !captured {
        return Err(io::Error::other(format!(
            "drag requires own unobstructed foreground Edit; point=({},{}), foreground={:?}, parent={hwnd:?}, hit={hit:?}, edit={edit:?}",
            point.x, point.y, unsafe { GetForegroundWindow() }
        )));
    }
    Ok(())
}
fn drag(
    hwnd: Hwnd,
    edit: Hwnd,
    theme: &str,
    mode: &str,
    frames: &mut String,
) -> io::Result<(usize, usize)> {
    let mut points = Vec::new();
    for i in 0..=28 {
        let index = if mode == "reverse" { 28 - i } else { i };
        let position = unsafe { SendMessageW(edit, 0xd6, index, 0) } as u32; // EM_POSFROMCHAR
        let mut point = Point {
            x: i32::from(position as u16 as i16) + 1,
            y: 12,
        };
        unsafe { ClientToScreen(edit, &mut point) };
        owned(hwnd, edit, point)?;
        points.push(point);
    }
    if mode == "scroll" {
        let mut rect = Rect::default();
        unsafe {
            GetClientRect(edit, &mut rect);
        }
        points.clear();
        for i in 0..=36 {
            let mut point = Point {
                x: if i <= 28 {
                    1 + (rect.right - 2) * i / 28
                } else {
                    rect.right + 1 + i % 2
                },
                y: 12,
            };
            unsafe {
                ClientToScreen(edit, &mut point);
            }
            points.push(point);
        }
    }
    let mut mouse = Mouse {
        original: Point::default(),
        hwnd,
        pressed: false,
    };
    if unsafe { GetCursorPos(&mut mouse.original) } == 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe { SetCursorPos(points[0].x, points[0].y) };
    owned(hwnd, edit, points[0])?;
    unsafe { mouse_event(2, 0, 0, 0, 0) };
    mouse.pressed = true;
    barrier(hwnd);
    let stop = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&stop);
    let (parent, child) = (hwnd as usize, edit as usize);
    let neutral = if theme == "light" { 0xdcdfe4 } else { 0x3f4249 };
    let sampler = thread::spawn(move || -> io::Result<(String, usize, usize)> {
        let screen = Screen::new(child as Hwnd)?;
        let blue = unsafe { GetSysColor(13) };
        let blue = (blue & 255) << 16 | blue & 0xff00 | blue >> 16 & 255;
        let start = Instant::now();
        let (mut samples, mut blue_frames, mut themed_frames) = (String::new(), 0, 0);
        while !signal.load(Ordering::Relaxed) {
            if unsafe { GetForegroundWindow() } as usize != parent {
                return Err(io::Error::other("drag sampler lost foreground"));
            }
            let (themed, native) = screen.colors(neutral, blue)?;
            samples.push_str(&format!(
                "{},{themed},{native}\n",
                start.elapsed().as_micros()
            ));
            blue_frames += usize::from(native > 0);
            themed_frames += usize::from(themed > 20);
        }
        Ok((samples, blue_frames, themed_frames))
    });
    let movement = (|| -> io::Result<()> {
        for point in points.into_iter().skip(1) {
            owned(hwnd, edit, point)?;
            if unsafe { SetCursorPos(point.x, point.y) } == 0 {
                return Err(io::Error::last_os_error());
            }
            thread::sleep(Duration::from_millis(12));
        }
        barrier(hwnd);
        Ok(())
    })();
    stop.store(true, Ordering::Relaxed);
    let sampled = sampler
        .join()
        .map_err(|_| io::Error::other("drag sampler panicked"))?;
    movement?;
    let (samples, blue, themed) = sampled?;
    frames.push_str(&samples);
    // RAII releases the physical button before checking native selection/capture below.
    drop(mouse);
    barrier(hwnd);
    Ok((blue, themed))
}
pub(super) fn run(reference: bool) -> io::Result<()> {
    super::super::window::enable_dpi();
    let root = std::env::current_dir()?.join("runtime/probe-edit-drag");
    let source = root.join("empty-source");
    fs::create_dir_all(&source)?;
    let exe = if reference {
        root.join("baseline.exe")
    } else {
        std::env::current_exe()?.with_file_name("picorun.exe")
    };
    let variant = if reference { "reference" } else { "fixed" };
    let mut checks = String::new();
    let mut memory = String::new();
    let mut summary = String::from(
        "theme,dpi,direction,frames,blue_frames,themed_frames,selection,buffer_bytes,first_character_x\n",
    );
    let result = (|| -> io::Result<()> {
        for theme in ["dark", "light"] {
            let data = root.join(format!("data-{variant}-{theme}"));
            fs::create_dir_all(&data)?;
            fs::write(data.join("english-input.txt"), "off\n")?;
            let args = [
                "--hidden".into(),
                "--hotkey".into(),
                "Ctrl+Alt+F11".into(),
                "--source".into(),
                source.as_os_str().to_owned(),
                "--data-dir".into(),
                data.as_os_str().to_owned(),
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
            sample(
                child.0.id(),
                &format!("{theme}-initial-hidden"),
                &mut memory,
            )?;
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
            }
            appearance_probe::foreground(hwnd)?;
            expect(
                unsafe { GetForegroundWindow() } == hwnd,
                "own drag window foreground",
                &mut checks,
            )?;
            for dpi in [96, 120, 168, 192] {
                unsafe {
                    SendMessageW(hwnd, 0x800d, dpi, 0);
                }
                for mode in ["forward", "reverse", "scroll"] {
                    let text = if mode == "scroll" {
                        "a".repeat(400)
                    } else {
                        "微信 QQ abcdefghijklmnopqrstuvwxyz 1234567890".into()
                    };
                    unsafe {
                        set_control_text(edit, wide(&text).as_ptr());
                        SendMessageW(hwnd, 0x8003, 0, 0);
                        SendMessageW(edit, 0xb1, 0, 0);
                        SendMessageW(edit, 0xb7, 0, 0); // EM_SCROLLCARET
                    }
                    barrier(hwnd);
                    let mut frames =
                        String::from("elapsed_us,neutral_pixels,system_highlight_pixels\n");
                    let (blue, themed) = drag(hwnd, edit, theme, mode, &mut frames)?;
                    let path = root.join(format!("{variant}-{theme}-{dpi}-{mode}.csv"));
                    fs::write(path, &frames)?;
                    let selection = unsafe { SendMessageW(edit, 0xb0, 0, 0) } as u32;
                    let bytes = unsafe { SendMessageW(hwnd, 0x800c, 13, 0) };
                    let first_x = unsafe { SendMessageW(edit, 0xd6, 0, 0) } as u16 as i16;
                    summary.push_str(&format!(
                        "{theme},{dpi},{mode},{},{blue},{themed},{selection},{bytes},{first_x}\n",
                        frames.lines().count() - 1
                    ));
                    expect(
                        themed > 5,
                        "continuous drag displays themed selection",
                        &mut checks,
                    )?;
                    expect(
                        reference || blue == 0,
                        &format!("no system highlight during drag; blue_frames={blue}"),
                        &mut checks,
                    )?;
                    expect(
                        selection & 0xffff == 0
                            && if mode == "scroll" {
                                first_x < 0 && selection >> 16 > 28 && selection >> 16 <= 400
                            } else {
                                selection >> 16 == 28
                            },
                        &format!(
                            "native drag selection correct: {selection:#x}, first_character_x={first_x}"
                        ),
                        &mut checks,
                    )?;
                    expect(
                        ime_probe::text(edit) == text && bytes > 0 && bytes <= 512 * 1024,
                        "native text unchanged; selected surface bounded",
                        &mut checks,
                    )?;
                    let mut info = GuiThreadInfo {
                        size: std::mem::size_of::<GuiThreadInfo>() as u32,
                        ..Default::default()
                    };
                    let tid = unsafe { GetWindowThreadProcessId(hwnd, null_mut()) };
                    expect(
                        unsafe { GetGUIThreadInfo(tid, &mut info) } != 0 && info.capture.is_null(),
                        "mouse capture released",
                        &mut checks,
                    )?;
                    expect(
                        unsafe { GetWindowLongPtrW(edit, -16) } & 0x10000000 != 0,
                        "Edit visible after drag",
                        &mut checks,
                    )?;
                    unsafe {
                        SendMessageW(edit, 0x102, 'x' as usize, 1);
                    }
                    expect(
                        ime_probe::text(edit)
                            == format!(
                                "x{}",
                                &text[text
                                    .char_indices()
                                    .nth((selection >> 16) as usize)
                                    .map_or(text.len(), |(offset, _)| offset)..]
                            )
                            && unsafe { SendMessageW(hwnd, 0x800c, 13, 0) } == 0,
                        "typing replaces native drag selection and releases surface",
                        &mut checks,
                    )?;
                }
            }
            sample(child.0.id(), &format!("{theme}-after-drags"), &mut memory)?;
            unsafe {
                SendMessageW(hwnd, 0x800a, 0, 0);
            }
            expect(
                unsafe { SendMessageW(hwnd, 0x800c, 13, 0) } == 0,
                "hide releases selection surface",
                &mut checks,
            )?;
            sample(child.0.id(), &format!("{theme}-final-hidden"), &mut memory)?;
            unsafe {
                SendMessageW(hwnd, 0x10, 0, 0);
            }
            child.0.wait()?;
        }
        Ok(())
    })();
    fs::write(root.join(format!("{variant}-checks.txt")), &checks)?;
    fs::write(root.join(format!("{variant}-summary.csv")), summary)?;
    fs::write(root.join(format!("{variant}-memory.csv")), memory)?;
    println!("{} drag checks passed ({variant})", checks.lines().count());
    result
}
