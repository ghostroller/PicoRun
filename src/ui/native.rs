//! GDI renderer. Snapshot data and Theme are independent of search and Win32 input.
use crate::{
    platform::windows::{ffi::*, wide},
    theme::{Rgb, Theme},
};
use std::{io, ptr::null, rc::Rc};

#[derive(Clone, Default)]
pub struct View {
    pub rows: Rc<[Vec<u16>]>,
    pub selected: Option<usize>,
    pub status: Rc<[u16]>,
}
pub struct Renderer {
    pub theme: Theme,
    pub dpi: u32,
    pub font: Handle,
    pub background: Handle,
    selection: Handle,
    help: Vec<u16>,
    empty: Vec<u16>,
}
pub fn color(Rgb(r, g, b): Rgb) -> u32 {
    u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16
}
impl Renderer {
    pub fn new(theme: Theme, dpi: u32) -> io::Result<Self> {
        let mut renderer = Self {
            theme,
            dpi,
            font: std::ptr::null_mut(),
            background: std::ptr::null_mut(),
            selection: std::ptr::null_mut(),
            help: wide("↑↓ 选择   Enter 打开   Esc 隐藏   F5 刷新   Ctrl+Q 退出"),
            empty: wide("没有匹配的应用"),
        };
        unsafe {
            renderer.font = CreateFontW(
                -renderer.scale(renderer.theme.font_size),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                wide(&renderer.theme.font_family).as_ptr(),
            );
            renderer.background = CreateSolidBrush(color(renderer.theme.background));
            renderer.selection = CreateSolidBrush(color(renderer.theme.selection));
        }
        if renderer.font.is_null() || renderer.background.is_null() || renderer.selection.is_null()
        {
            return Err(io::Error::last_os_error());
        }
        Ok(renderer)
    }
    pub fn scale(&self, value: u16) -> i32 {
        (u32::from(value) * self.dpi / 96) as i32
    }
    pub fn top(&self) -> i32 {
        self.scale(58)
    }
    pub fn height(&self, rows: usize) -> i32 {
        self.top() + self.scale(self.theme.row_height) * rows.max(1) as i32 + self.scale(60)
    }
    pub fn paint(&self, hwnd: Hwnd, view: &View) {
        // View owns all strings. No controller/RefCell borrow survives GDI or window calls.
        unsafe {
            let mut paint = Paint::default();
            let dc = BeginPaint(hwnd, &mut paint);
            let mut client = Rect::default();
            GetClientRect(hwnd, &mut client);
            FillRect(dc, &client, self.background);
            let previous_font = SelectObject(dc, self.font);
            SetBkMode(dc, 1);
            SetTextColor(dc, color(self.theme.foreground));
            let pad = self.scale(self.theme.padding);
            let row_height = self.scale(self.theme.row_height);
            for (index, row) in view.rows.iter().enumerate() {
                let top = self.top() + index as i32 * row_height;
                let mut rect = Rect {
                    left: pad,
                    top,
                    right: client.right - pad,
                    bottom: top + row_height,
                };
                if view.selected == Some(index) {
                    FillRect(dc, &rect, self.selection);
                }
                rect.left += pad;
                DrawTextW(
                    dc,
                    row.as_ptr(),
                    row.len() as i32,
                    &mut rect,
                    0x20 | 4 | 0x8000 | 0x800,
                );
            }
            SetTextColor(dc, color(self.theme.muted));
            if view.rows.is_empty() {
                let mut rect = Rect {
                    left: pad * 2,
                    top: self.top(),
                    right: client.right - pad,
                    bottom: self.top() + row_height,
                };
                DrawTextW(dc, self.empty.as_ptr(), -1, &mut rect, 0x20 | 4 | 0x800);
            }
            let mut status = Rect {
                left: pad,
                top: client.bottom - self.scale(51),
                right: client.right - pad,
                bottom: client.bottom - self.scale(28),
            };
            DrawTextW(
                dc,
                view.status.as_ptr(),
                view.status.len() as i32,
                &mut status,
                0x20 | 4 | 0x8000 | 0x800,
            );
            status.top = client.bottom - self.scale(28);
            status.bottom = client.bottom - self.scale(5);
            DrawTextW(dc, self.help.as_ptr(), -1, &mut status, 0x20 | 4 | 0x800);
            SelectObject(dc, previous_font);
            EndPaint(hwnd, &paint);
        }
    }
    pub fn invalidate(&self, hwnd: Hwnd) {
        unsafe {
            InvalidateRect(hwnd, null(), 0);
        }
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        // Window/control are destroyed before the final owner drops; font is deselected after paint.
        unsafe {
            for resource in [self.font, self.background, self.selection] {
                if !resource.is_null() {
                    DeleteObject(resource);
                }
            }
        }
    }
}
