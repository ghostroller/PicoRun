//! A reusable single-row surface. No whole-window backing bitmap is retained.
use crate::platform::windows::ffi::*;

pub(super) struct RowBuffer {
    pub dc: Handle,
    bitmap: Handle,
    previous: Handle,
    pub width: i32,
    pub height: i32,
}
impl RowBuffer {
    pub fn new(screen: Handle, width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        unsafe {
            let dc = CreateCompatibleDC(screen);
            if dc.is_null() {
                return None;
            }
            // Use the display DC, not the memory DC's initially monochrome stock bitmap.
            let bitmap = CreateCompatibleBitmap(screen, width, height);
            if bitmap.is_null() {
                DeleteDC(dc);
                return None;
            }
            let previous = SelectObject(dc, bitmap);
            if previous.is_null() || previous as isize == -1 {
                DeleteObject(bitmap);
                DeleteDC(dc);
                return None;
            }
            Some(Self {
                dc,
                bitmap,
                previous,
                width,
                height,
            })
        }
    }
}
impl Drop for RowBuffer {
    fn drop(&mut self) {
        unsafe {
            // Deselect the owned bitmap before deleting it, then release its DC.
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
