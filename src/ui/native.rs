//! GDI renderer. Snapshot data and Theme are independent of search and Win32 input.
use crate::platform::windows::icons::Icon;
use crate::{
    platform::windows::{ffi::*, wide},
    theme::{Rgb, Theme},
};
use std::{cell::Cell, io, ptr::null, rc::Rc, sync::Arc};
mod row_buffer;
use row_buffer::RowBuffer;

#[derive(Clone, Default)]
pub struct View {
    pub rows: Rc<[Vec<u16>]>,
    pub selected: Option<usize>,
    pub status: Rc<[u16]>,
    pub show_icons: bool,
    pub icons: Rc<[Option<Arc<Icon>>]>,
}
pub struct Renderer {
    pub theme: Theme,
    pub dpi: u32,
    pub font: Handle,
    pub background: Handle,
    selection: Handle,
    help: Vec<u16>,
    empty: Vec<u16>,
    row_buffer: Cell<Option<RowBuffer>>,
}
pub fn color(Rgb(r, g, b): Rgb) -> u32 {
    u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16
}
fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
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
            row_buffer: Cell::new(None),
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
    /// Hit only a visible result's painted selection area, excluding padding and footer.
    pub fn row_at(&self, x: i32, y: i32, rows: usize) -> Option<usize> {
        if x < self.scale(self.theme.padding)
            || x >= self.scale(self.theme.width - self.theme.padding)
            || y < self.top()
        {
            return None;
        }
        let index = ((y - self.top()) / self.scale(self.theme.row_height)) as usize;
        (index < rows).then_some(index)
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
            self.draw(dc, client, paint.rect, view);
            EndPaint(hwnd, &paint);
        }
    }
    pub fn print(&self, hwnd: Hwnd, dc: Handle, view: &View) {
        unsafe {
            let mut client = Rect::default();
            GetClientRect(hwnd, &mut client);
            // WM_PRINTCLIENT's target DC has no BeginPaint update region. Render its
            // full client explicitly instead of treating an empty region as a repaint.
            self.draw(dc, client, client, view);
            if IsWindowVisible(hwnd) == 0 {
                self.release_row_buffer();
            }
        }
    }
    unsafe fn draw(&self, dc: Handle, client: Rect, dirty: Rect, view: &View) {
        // Do not clear visible rows before their complete replacement is ready.
        let header = Rect {
            bottom: self.top(),
            ..client
        };
        let footer = Rect {
            top: self.top() + self.scale(self.theme.row_height) * view.rows.len().max(1) as i32,
            ..client
        };
        FillRect(dc, &header, self.background);
        FillRect(dc, &footer, self.background);
        let previous_font = SelectObject(dc, self.font);
        SetBkMode(dc, 1);
        SetTextColor(dc, color(self.theme.foreground));
        let pad = self.scale(self.theme.padding);
        let row_height = self.scale(self.theme.row_height);
        // Take ownership out of Cell before native calls: no RefCell/mutable borrow
        // crosses BeginPaint/BitBlt or a possible nested window message.
        let mut buffer = self.row_buffer.take();
        if buffer
            .as_ref()
            .is_none_or(|b| b.width != client.right || b.height != row_height)
        {
            buffer = RowBuffer::new(dc, client.right, row_height);
        }
        let buffered_font = buffer.as_ref().map(|b| {
            SetBkMode(b.dc, 1);
            SelectObject(b.dc, self.font)
        });
        for index in 0..view.rows.len().max(1) {
            let top = self.top() + index as i32 * row_height;
            let row = Rect {
                left: 0,
                top,
                right: client.right,
                bottom: top + row_height,
            };
            if !overlaps(&dirty, &row) {
                continue;
            }
            let text_left = pad * 2
                + if view.show_icons && !view.rows.is_empty() {
                    self.scale(20) + self.scale(8)
                } else {
                    0
                };
            let text_region = Rect {
                left: text_left,
                right: client.right - pad,
                ..row
            };
            let text_dirty = overlaps(&dirty, &text_region);
            if let Some(buffer) = &buffer {
                self.paint_row(
                    buffer.dc,
                    Rect {
                        top: 0,
                        bottom: row_height,
                        ..row
                    },
                    view,
                    index,
                    text_dirty,
                );
                // BeginPaint's clipping limits the copy to the actual update region.
                if BitBlt(
                    dc,
                    0,
                    top,
                    client.right,
                    row_height,
                    buffer.dc,
                    0,
                    0,
                    0x00cc0020,
                ) != 0
                {
                    continue;
                }
            }
            // Resource exhaustion still produces a usable list via the direct path.
            self.paint_row(dc, row, view, index, text_dirty);
        }
        if let (Some(buffer), Some(previous)) = (&buffer, buffered_font) {
            SelectObject(buffer.dc, previous);
        }
        self.row_buffer.set(buffer);
        SetTextColor(dc, color(self.theme.muted));
        let mut status = Rect {
            left: pad,
            top: client.bottom - self.scale(51),
            right: client.right - pad,
            bottom: client.bottom - self.scale(28),
        };
        if overlaps(&dirty, &status) {
            DrawTextW(
                dc,
                view.status.as_ptr(),
                view.status.len() as i32,
                &mut status,
                0x20 | 4 | 0x8000 | 0x800,
            );
        }
        status.top = client.bottom - self.scale(28);
        status.bottom = client.bottom - self.scale(5);
        if overlaps(&dirty, &status) {
            DrawTextW(dc, self.help.as_ptr(), -1, &mut status, 0x20 | 4 | 0x800);
        }
        SelectObject(dc, previous_font);
    }
    unsafe fn paint_row(&self, dc: Handle, row: Rect, view: &View, index: usize, text_dirty: bool) {
        FillRect(dc, &row, self.background);
        let pad = self.scale(self.theme.padding);
        let mut text = Rect {
            left: pad,
            right: row.right - pad,
            ..row
        };
        if view.selected == Some(index) {
            FillRect(dc, &text, self.selection);
        }
        text.left += pad;
        if view.show_icons && !view.rows.is_empty() {
            let size = self.scale(20);
            if let Some(Some(icon)) = view.icons.get(index) {
                DrawIconEx(
                    dc,
                    text.left,
                    row.top + (row.bottom - row.top - size) / 2,
                    icon.handle(),
                    size,
                    size,
                    0,
                    std::ptr::null_mut(),
                    3,
                );
            }
            text.left += size + self.scale(8);
        }
        if text_dirty {
            if let Some(name) = view.rows.get(index) {
                SetTextColor(dc, color(self.theme.foreground));
                DrawTextW(
                    dc,
                    name.as_ptr(),
                    name.len() as i32,
                    &mut text,
                    0x20 | 4 | 0x8000 | 0x800,
                );
            } else {
                SetTextColor(dc, color(self.theme.muted));
                DrawTextW(dc, self.empty.as_ptr(), -1, &mut text, 0x20 | 4 | 0x800);
            }
        }
    }
    pub fn release_row_buffer(&self) {
        drop(self.row_buffer.take());
    }
    pub(crate) fn row_buffer_pixels(&self) -> usize {
        let buffer = self.row_buffer.take();
        let pixels = buffer
            .as_ref()
            .map_or(0, |b| b.width as usize * b.height as usize);
        self.row_buffer.set(buffer);
        pixels
    }
    pub fn invalidate_selection(
        &self,
        hwnd: Hwnd,
        previous: Option<usize>,
        selected: Option<usize>,
    ) {
        if previous == selected {
            return;
        }
        for index in [previous, selected].into_iter().flatten() {
            let row = Rect {
                left: self.scale(self.theme.padding),
                top: self.top() + self.scale(self.theme.row_height) * index as i32,
                right: self.scale(self.theme.width - self.theme.padding),
                bottom: self.top() + self.scale(self.theme.row_height) * (index + 1) as i32,
            };
            unsafe {
                InvalidateRect(hwnd, &row, 0);
            }
        }
    }
    pub fn invalidate(&self, hwnd: Hwnd) {
        unsafe {
            InvalidateRect(hwnd, null(), 0);
        }
    }
    pub fn invalidate_icons(&self, hwnd: Hwnd, rows: usize) {
        let left = self.scale(self.theme.padding) * 2;
        let region = Rect {
            left,
            top: self.top(),
            right: left + self.scale(20),
            bottom: self.top() + self.scale(self.theme.row_height) * rows as i32,
        };
        // Only the icon column changed. Existing list text/background remain valid; GDI's
        // update-region clipping applies to paint(), with no additional full-window bitmap.
        unsafe { InvalidateRect(hwnd, &region, 0) };
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icon_column_does_not_overlap_text_or_footer() {
        let icon = Rect {
            left: 24,
            top: 58,
            right: 44,
            bottom: 96,
        };
        let text = Rect {
            left: 52,
            top: 58,
            right: 548,
            bottom: 96,
        };
        let footer = Rect {
            left: 12,
            top: 100,
            right: 548,
            bottom: 123,
        };
        assert!(!overlaps(&icon, &text));
        assert!(!overlaps(&icon, &footer));
        let all = Rect {
            left: 0,
            top: 0,
            right: 560,
            bottom: 160,
        };
        assert!(overlaps(&all, &icon));
        assert!(overlaps(&all, &text));
        assert!(overlaps(&all, &footer));
    }
}
