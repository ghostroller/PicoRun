//! English only during an input session. No layouts are installed or unloaded.
use super::ffi::*;
use std::{cell::Cell, io, ptr::null_mut};

#[derive(Clone, Copy)]
struct Mode {
    open: bool,
    conversion: Option<(u32, u32)>,
}
#[derive(Clone, Copy)]
struct Origin {
    window: Hwnd,
    thread: u32,
    process: u32,
}
#[derive(Clone, Copy)]
struct Session {
    edit: Hwnd,
    layout: Handle,
    mode: Option<Mode>,
    origin: Option<Origin>,
}
thread_local! {
    // HWND/HKL are borrowed identifiers. Only mode values are saved; no HIMC is retained.
    static PREVIOUS: Cell<Option<Session>> = const { Cell::new(None) };
    static RESTORING: Cell<bool> = const { Cell::new(false) };
}
struct Restoring;
impl Drop for Restoring {
    fn drop(&mut self) {
        RESTORING.set(false);
    }
}

fn english(layout: Handle) -> bool {
    layout as usize & 0x3ff == 9 // PRIMARYLANGID(LANGID): LANG_ENGLISH, any region.
}

struct Context {
    hwnd: Hwnd,
    handle: Handle,
}
impl Context {
    fn get(hwnd: Hwnd) -> Option<Self> {
        let handle = unsafe { ImmGetContext(hwnd) };
        (!handle.is_null()).then_some(Self { hwnd, handle })
    }
    fn mode(&self) -> Mode {
        let mut conversion = 0;
        let mut sentence = 0;
        let valid = unsafe { ImmGetConversionStatus(self.handle, &mut conversion, &mut sentence) };
        Mode {
            open: unsafe { ImmGetOpenStatus(self.handle) != 0 },
            conversion: (valid != 0).then_some((conversion, sentence)),
        }
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        // This is a temporary own-thread context, paired with the same HWND.
        unsafe { ImmReleaseContext(self.hwnd, self.handle) };
    }
}

fn ime_value(ime: Hwnd, command: usize) -> Option<usize> {
    ime_message(ime, command, 0)
}
fn ime_message(ime: Hwnd, command: usize, data: isize) -> Option<usize> {
    let mut value = 0;
    // A foreign hung or elevated window must not stall the launcher. Only scalar
    // IMC values are sent here, never pointers or focus changes.
    (unsafe { SendMessageTimeoutW(ime, 0x283, command, data, 2, 50, &mut value) } != 0)
        .then_some(value)
}
fn foreground(window: Hwnd) -> Option<(Handle, Option<Mode>, Option<Origin>)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() || hwnd == window {
            return None;
        }
        let mut pid = 0;
        let thread = GetWindowThreadProcessId(hwnd, &mut pid);
        if thread == 0 {
            return None;
        }
        let layout = GetKeyboardLayout(thread);
        if layout.is_null() {
            return None;
        }
        let mut info = GuiThreadInfo {
            size: std::mem::size_of::<GuiThreadInfo>() as u32,
            ..Default::default()
        };
        let has_focus = GetGUIThreadInfo(thread, &mut info) != 0 && !info.focus.is_null();
        let target = if has_focus { info.focus } else { hwnd };
        let ime = ImmGetDefaultIMEWnd(target);
        let mode = if !english(layout) && has_focus {
            if ime.is_null() {
                None
            } else {
                ime_value(ime, 5).map(|open| Mode {
                    // A matched original window is the only foreign context we restore.
                    open: open != 0,
                    conversion: ime_value(ime, 1).and_then(|conversion| {
                        ime_value(ime, 3).map(|sentence| (conversion as u32, sentence as u32))
                    }),
                })
            }
        } else {
            None
        };
        let origin = Some(Origin {
            window: target,
            thread,
            process: pid,
        });
        Some((layout, mode, origin))
    }
}

/// Save before ShowWindow/SetFocus lets TSF change the active layout or mode.
/// Repeated show/enable calls cannot overwrite the first backup in this session.
pub fn remember(window: Hwnd, edit: Hwnd, from_foreground: bool) -> io::Result<()> {
    if RESTORING.get() || PREVIOUS.get().is_some() {
        return Ok(());
    }
    let own = unsafe { GetKeyboardLayout(0) };
    if own.is_null() {
        return Err(io::Error::last_os_error());
    }
    let (layout, mode, origin) = if from_foreground {
        foreground(window).unwrap_or_else(|| (own, Context::get(edit).map(|c| c.mode()), None))
    } else {
        (own, Context::get(edit).map(|c| c.mode()), None)
    };
    PREVIOUS.set(Some(Session {
        edit,
        layout,
        mode,
        origin,
    }));
    Ok(())
}

pub fn begin(window: Hwnd, edit: Hwnd) -> io::Result<()> {
    if RESTORING.get() {
        return Ok(()); // IME restoration may synchronously refocus the Edit.
    }
    remember(window, edit, false)?;
    let current = unsafe { GetKeyboardLayout(0) };
    if let Some(previous) = PREVIOUS.get() {
        if !english(previous.layout) && previous.mode.is_some() {
            // Keep the invoking IME/profile. Changing HKL can make TSF remember English
            // in the original app's Edit as well; a temporary closed context avoids it.
            if current != previous.layout
                && unsafe { ActivateKeyboardLayout(previous.layout, 0) }.is_null()
            {
                return Err(io::Error::last_os_error());
            }
            if let Some(context) = Context::get(edit) {
                if unsafe { ImmGetOpenStatus(context.handle) } == 0
                    || unsafe { ImmSetOpenStatus(context.handle, 0) } != 0
                        && unsafe { ImmGetOpenStatus(context.handle) } == 0
                {
                    return Ok(());
                }
            }
        }
    }
    let current = unsafe { GetKeyboardLayout(0) };
    let target = if english(current) {
        current
    } else {
        let count = unsafe { GetKeyboardLayoutList(0, null_mut()) };
        if !(1..=1024).contains(&count) {
            return Err(io::Error::other("无法读取可用的键盘布局"));
        }
        let mut layouts = vec![null_mut(); count as usize];
        let copied = unsafe { GetKeyboardLayoutList(count, layouts.as_mut_ptr()) };
        layouts[..copied.max(0).min(count) as usize]
            .iter()
            .copied()
            .find(|&layout| english(layout))
            .ok_or_else(|| io::Error::other("未找到可用的英文键盘布局"))?
    };
    // Publish before this reentrant call. Flags=0 affects our thread only.
    if target != current && unsafe { ActivateKeyboardLayout(target, 0) }.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn restore() -> io::Result<()> {
    if RESTORING.get() {
        return Ok(());
    }
    // Take the backup before reentrant FFI. WM_KILLFOCUS/hide/close share this session.
    let Some(previous) = PREVIOUS.take() else {
        return Ok(());
    };
    RESTORING.set(true);
    let _restoring = Restoring;
    let result = restore_session(previous);
    if result.is_err() && PREVIOUS.get().is_none() {
        PREVIOUS.set(Some(previous)); // A later hide/close can retry.
    }
    result
}
fn restore_session(previous: Session) -> io::Result<()> {
    if unsafe { GetKeyboardLayout(0) != previous.layout }
        && unsafe { ActivateKeyboardLayout(previous.layout, 0) }.is_null()
    {
        return Err(io::Error::last_os_error());
    }
    let own = restore_mode(previous);
    let origin = restore_origin(previous);
    own.and(origin)
}
fn restore_mode(previous: Session) -> io::Result<()> {
    if let Some(mode) = previous.mode {
        let context = Context::get(previous.edit)
            .ok_or_else(|| io::Error::other("原生输入框的输入法上下文已失效"))?;
        let current = context.mode();
        // Some TSF IMEs derive their open state from IME_CMODE_NATIVE. Restoring
        // a stale native flag in a closed context can reopen it asynchronously.
        // Conversion/sentence modes apply to composition; preserve English first.
        if let Some((conversion, sentence)) = mode.conversion.filter(|_| mode.open) {
            if current.conversion != mode.conversion
                && unsafe { ImmSetConversionStatus(context.handle, conversion, sentence) } == 0
            {
                return Err(io::Error::other("输入法转换模式恢复失败"));
            }
        }
        // Apply the open state last, even when the old readback already matches:
        // conversion changes may notify TSF asynchronously.
        if unsafe { ImmSetOpenStatus(context.handle, i32::from(mode.open)) } == 0 {
            return Err(io::Error::other("输入法中英文模式恢复失败"));
        }
    }
    Ok(())
}
fn restore_origin(previous: Session) -> io::Result<()> {
    let Some(origin) = previous.origin else {
        return Ok(());
    };
    let mut pid = 0;
    unsafe {
        if GetWindowThreadProcessId(origin.window, &mut pid) != origin.thread
            || pid != origin.process
        {
            return Ok(()); // Original app closed; never address a different process/thread.
        }
        // Layout requests are scalar and can be posted. WM_IME_CONTROL cannot be
        // posted across processes on Windows even for scalar IMC commands; send
        // those with a timeout so a foreign focus transition cannot block us.
        if GetKeyboardLayout(origin.thread) != previous.layout
            && PostMessageW(origin.window, 0x50, 0, previous.layout as isize) == 0
        {
            return Err(io::Error::other("原窗口的输入布局恢复请求失败"));
        }
        let Some(mode) = previous.mode else {
            return Ok(());
        };
        let ime = ImmGetDefaultIMEWnd(origin.window);
        if ime.is_null() {
            return Err(io::Error::other("原窗口的输入法上下文已失效"));
        }
        if let Some((conversion, sentence)) = mode.conversion.filter(|_| mode.open) {
            if ime_message(ime, 2, conversion as isize).is_none()
                || ime_message(ime, 4, sentence as isize).is_none()
            {
                return Err(io::Error::other("原窗口的输入法转换模式恢复请求失败"));
            }
        }
        if ime_message(ime, 6, isize::from(mode.open)).is_none() {
            return Err(io::Error::other("原窗口的输入法中英文模式恢复请求失败"));
        }
    }
    Ok(())
}
