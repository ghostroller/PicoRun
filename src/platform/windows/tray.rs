//! A single owned notification icon; native menus exist only while open. No timers or polling.
use super::{ffi::*, wide};
use crate::i18n::{current, Language, Text};
use crate::theme::ThemeMode;
use std::{
    io,
    ptr::{null, null_mut},
};

pub(super) const CALLBACK: u32 = 0x8004;
pub(super) const ICON_ID: u32 = 1;
pub(super) const SHOW: u32 = 10;
pub(super) const REFRESH: u32 = 11;
pub(super) const LIGHT: u32 = 12;
pub(super) const DARK: u32 = 13;
pub(super) const EXIT: u32 = 14;
pub(super) const ENGLISH: u32 = 15;
pub(super) const ICONS: u32 = 16;
pub(super) const STARTUP: u32 = 17;
pub(super) const CHINESE_UI: u32 = 18;
pub(super) const ENGLISH_UI: u32 = 19;

pub struct Tray {
    hwnd: Hwnd,
    icon: Handle,
    tip: [u16; 128],
}
impl Tray {
    pub fn new(hwnd: Hwnd, hotkey: &str) -> io::Result<Self> {
        // One tiny, built-in blue P icon. Opaque BGRA pixels and a matching transparency mask.
        // CreateIcon copies both temporary buffers; DestroyIcon releases the returned owned handle.
        let mut pixels = [0u8; 32 * 32 * 4];
        let mut mask = [255u8; 32 * 4];
        for y in 2..30 {
            for x in 2..30 {
                let p = (y * 32 + x) * 4;
                let letter = (9..13).contains(&x) && (7..26).contains(&y)
                    || (13..22).contains(&x) && ((7..11).contains(&y) || (15..19).contains(&y))
                    || (20..24).contains(&x) && (9..17).contains(&y);
                pixels[p..p + 4].copy_from_slice(if letter {
                    &[255, 255, 255, 255]
                } else {
                    &[200, 110, 35, 255]
                });
                mask[y * 4 + x / 8] &= !(0x80 >> (x % 8));
            }
        }
        let icon = unsafe {
            CreateIcon(
                GetModuleHandleW(null()),
                32,
                32,
                1,
                32,
                mask.as_ptr(),
                pixels.as_ptr(),
            )
        };
        if icon.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut tray = Self {
            hwnd,
            icon,
            tip: [0; 128],
        };
        for (slot, ch) in tray.tip[..127]
            .iter_mut()
            .zip(format!("PicoRun · {hotkey}").encode_utf16())
        {
            *slot = ch;
        }
        tray.install()?;
        Ok(tray)
    }
    fn data(&self) -> NotifyIconData {
        // Zero initialization includes the unused balloon fields and GUID.
        let mut data: NotifyIconData = unsafe { std::mem::zeroed() };
        data.size = std::mem::size_of::<NotifyIconData>() as u32;
        data.hwnd = self.hwnd;
        data.id = ICON_ID;
        data.flags = 1 | 2 | 4 | 0x80; // message, icon, tooltip, standard tooltip with version 4.
        data.callback = CALLBACK;
        data.icon = self.icon;
        data.tip = self.tip;
        data
    }
    pub fn install(&self) -> io::Result<()> {
        let mut data = self.data();
        unsafe {
            // TaskbarCreated also uses this path: delete first makes repeated notifications idempotent.
            Shell_NotifyIconW(2, &data);
            if Shell_NotifyIconW(0, &data) == 0 {
                return Err(io::Error::other(Text::TrayAdd));
            }
            data.version = 4;
            if Shell_NotifyIconW(4, &data) == 0 {
                return Err(io::Error::other(Text::TrayEvents));
            }
        }
        Ok(())
    }
    pub fn menu(
        &self,
        mode: ThemeMode,
        start_english: bool,
        show_icons: bool,
        startup_enabled: bool,
        mut point: Point,
    ) -> io::Result<u32> {
        let handle = unsafe { CreatePopupMenu() };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let menu = Menu(handle);
        unsafe {
            for (id, label) in [
                (SHOW, Text::Open.get(current())),
                (REFRESH, Text::Refresh.get(current())),
                (0, ""),
                (LIGHT, Text::Light.get(current())),
                (DARK, Text::Dark.get(current())),
                (0, ""),
                (CHINESE_UI, "简体中文(&C)"),
                (ENGLISH_UI, "English(&G)"),
                (0, ""),
                (ENGLISH, Text::EnglishInput.get(current())),
                (ICONS, Text::Icons.get(current())),
                (STARTUP, Text::Startup.get(current())),
                (0, ""),
                (EXIT, Text::Quit.get(current())),
            ] {
                if AppendMenuW(
                    menu.0,
                    if id == 0 {
                        0x800
                    } else if id == ENGLISH && start_english
                        || id == ICONS && show_icons
                        || id == STARTUP && startup_enabled
                    {
                        8 // MF_CHECKED
                    } else {
                        0
                    },
                    id as usize,
                    wide(label).as_ptr(),
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
            CheckMenuRadioItem(
                menu.0,
                LIGHT,
                DARK,
                if mode == ThemeMode::Light {
                    LIGHT
                } else {
                    DARK
                },
                0,
            );
            CheckMenuRadioItem(
                menu.0,
                CHINESE_UI,
                ENGLISH_UI,
                if current() == Language::Chinese {
                    CHINESE_UI
                } else {
                    ENGLISH_UI
                },
                0,
            );
            if point.x == -1 && point.y == -1 {
                GetCursorPos(&mut point);
            }
            // Shell menu dismissal requires the owner to be foreground and a benign posted message.
            // No Runtime borrow is alive across this nested native message loop.
            SetForegroundWindow(self.hwnd);
            let command = TrackPopupMenuEx(
                menu.0,
                0x100 | 0x80 | 2,
                point.x,
                point.y,
                self.hwnd,
                null(),
            );
            PostMessageW(self.hwnd, 0, 0, 0);
            Ok(command)
        }
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            Shell_NotifyIconW(2, &self.data());
            DestroyIcon(self.icon);
        }
        self.icon = null_mut();
    }
}
struct Menu(Handle);
impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            DestroyMenu(self.0);
        }
    }
}
