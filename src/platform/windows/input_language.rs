//! Temporary input layout for this UI thread. No layouts are installed or unloaded.
use super::ffi::*;
use std::{cell::Cell, io, ptr::null_mut};

thread_local! {
    // HKLs are borrowed system identifiers, not resources owned by PicoRun.
    static PREVIOUS: Cell<Handle> = const { Cell::new(null_mut()) };
}

fn english(layout: Handle) -> bool {
    layout as usize & 0x3ff == 9 // PRIMARYLANGID(LANGID): LANG_ENGLISH, any region.
}

pub fn begin(original: Handle) -> io::Result<()> {
    unsafe {
        let current = GetKeyboardLayout(0);
        if current.is_null() || original.is_null() {
            return Err(io::Error::last_os_error());
        }
        let target = if english(current) {
            current
        } else {
            let count = GetKeyboardLayoutList(0, null_mut());
            if !(1..=1024).contains(&count) {
                return Err(io::Error::other("无法读取可用的键盘布局"));
            }
            let mut layouts = vec![null_mut(); count as usize];
            let copied = GetKeyboardLayoutList(count, layouts.as_mut_ptr());
            layouts[..copied.max(0).min(count) as usize]
                .iter()
                .copied()
                .find(|&layout| english(layout))
                .ok_or_else(|| io::Error::other("未找到可用的英文键盘布局"))?
        };
        let first = PREVIOUS.get().is_null();
        if first {
            PREVIOUS.set(original);
        }
        // Publish the backup before the reentrant call; no Rust borrow crosses the FFI boundary.
        // Flags=0 changes only this thread, preserving other applications and system preferences.
        if target != current && ActivateKeyboardLayout(target, 0).is_null() {
            if first {
                PREVIOUS.set(null_mut());
            }
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub fn restore() -> io::Result<()> {
    // Clear before calling Win32 so a reentrant hide/close cannot restore the same session twice.
    let previous = PREVIOUS.replace(null_mut());
    if !previous.is_null()
        && unsafe {
            GetKeyboardLayout(0) != previous && ActivateKeyboardLayout(previous, 0).is_null()
        }
    {
        PREVIOUS.set(previous); // A later hide/close can retry a failed restoration.
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
