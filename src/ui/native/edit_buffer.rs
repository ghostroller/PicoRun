//! A bounded, reusable native Edit surface, owned only while selected text is visible.
use crate::{platform::windows::ffi::*, theme::Rgb};
use std::ptr::null_mut;

const MAX_BYTES: usize = 512 * 1024;
#[repr(C)]
struct BitmapInfo {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bits: u16,
    compression: u32,
    image_size: u32,
    xppm: i32,
    yppm: i32,
    used: u32,
    important: u32,
    colors: [u32; 1],
}
pub(super) struct EditBuffer {
    pub dc: Handle,
    bitmap: Handle,
    previous: Handle,
    pixels: *mut u32,
    pub width: i32,
    pub height: i32,
}
impl EditBuffer {
    pub fn new(width: i32, height: i32) -> Option<Self> {
        if width <= 0
            || height <= 0
            || (width as usize)
                .checked_mul(height as usize)?
                .checked_mul(4)?
                > MAX_BYTES
        {
            return None;
        }
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return None;
            }
            let info = BitmapInfo {
                size: 40,
                width,
                height: -height,
                planes: 1,
                bits: 32,
                compression: 0,
                image_size: 0,
                xppm: 0,
                yppm: 0,
                used: 0,
                important: 0,
                colors: [0],
            };
            let mut pixels = null_mut();
            let bitmap = CreateDIBSection(
                dc,
                (&info as *const BitmapInfo).cast(),
                0,
                &mut pixels,
                null_mut(),
                0,
            );
            if bitmap.is_null() || pixels.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
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
                pixels: pixels.cast(),
                width,
                height,
            })
        }
    }
    pub fn bytes(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }
    pub fn recolor(&self, background: u32, foreground: u32, target_bg: Rgb, target_fg: Rgb) {
        // Caller has flushed GDI. This UI-thread-owned DIB is taken out of its Cell during
        // painting; no callback or GDI call occurs while this exclusive pixel slice exists.
        let pixels = unsafe { std::slice::from_raw_parts_mut(self.pixels, self.bytes() / 4) };
        let bg = ref_rgb(background);
        let fg = ref_rgb(foreground);
        recolor(
            pixels,
            self.width as usize,
            bg,
            fg,
            rgb(target_bg),
            rgb(target_fg),
        );
    }
}
fn ref_rgb(value: u32) -> u32 {
    (value & 0xff) << 16 | value & 0xff00 | value >> 16 & 0xff
}
fn rgb(Rgb(r, g, b): Rgb) -> u32 {
    u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}
fn channels(value: u32) -> [i32; 3] {
    [
        (value >> 16 & 255) as i32,
        (value >> 8 & 255) as i32,
        (value & 255) as i32,
    ]
}
fn recolor(pixels: &mut [u32], width: usize, bg: u32, fg: u32, target_bg: u32, target_fg: u32) {
    // Find the native highlight rectangle so ordinary text/background stays untouched.
    let (mut left, mut top, mut right, mut bottom) = (width, pixels.len() / width, 0, 0);
    for (i, &pixel) in pixels.iter().enumerate() {
        if pixel & 0xffffff == bg {
            left = left.min(i % width);
            top = top.min(i / width);
            right = right.max(i % width + 1);
            bottom = bottom.max(i / width + 1);
        }
    }
    if left >= right {
        return;
    }
    let (bgc, fgc, new_bg, new_fg) = (
        channels(bg),
        channels(fg),
        channels(target_bg),
        channels(target_fg),
    );
    let mut table = [[0u8; 256]; 3];
    for c in 0..3 {
        for (v, mapped) in table[c].iter_mut().enumerate() {
            let range = fgc[c] - bgc[c];
            let coverage = if range == 0 {
                0
            } else {
                ((v as i32 - bgc[c]) * 255 / range).clamp(0, 255)
            };
            *mapped = (new_bg[c] + (new_fg[c] - new_bg[c]) * coverage / 255).clamp(0, 255) as u8;
        }
    }
    for y in top..bottom {
        for pixel in &mut pixels[y * width + left..y * width + right] {
            let value = *pixel & 0xffffff;
            let mapped = if value == bg {
                target_bg
            } else if value == fg {
                target_fg
            } else {
                let values = channels(value);
                // Preserve channel coverage, including ClearType's independent subpixels.
                if !(0..3).all(|c| (bgc[c].min(fgc[c])..=bgc[c].max(fgc[c])).contains(&values[c])) {
                    continue;
                }
                u32::from(table[0][values[0] as usize]) << 16
                    | u32::from(table[1][values[1] as usize]) << 8
                    | u32::from(table[2][values[2] as usize])
            };
            *pixel = *pixel & 0xff000000 | mapped;
        }
    }
}
impl Drop for EditBuffer {
    fn drop(&mut self) {
        unsafe {
            GdiFlush();
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn highlight_recolors_subpixels_without_changing_unselected_text() {
        let bg = 0x0078d7;
        let fg = 0xffffff;
        let mut pixels = [
            0xffffff, 0x181a1e, bg, fg, bg, 0xffffff, bg, 0x7fc3eb, bg, 0xffffff,
        ];
        recolor(&mut pixels, 5, bg, fg, 0x3f4249, 0xebedf0);
        assert_eq!(pixels[0], 0xffffff);
        assert_eq!(pixels[1], 0x181a1e);
        assert_eq!(pixels[2], 0x3f4249);
        assert_eq!(pixels[3], 0xebedf0);
        assert_eq!(pixels[4], 0x3f4249);
        assert_ne!(pixels[7], 0x7fc3eb);
        assert_eq!(pixels[5], 0xffffff);
    }
    #[test]
    fn edit_surface_is_bounded_and_invalid_dimensions_are_rejected() {
        assert!(EditBuffer::new(0, 32).is_none());
        assert!(EditBuffer::new(4096, 256).is_none());
        assert!(EditBuffer::new(i32::MAX, i32::MAX).is_none());
    }
}
