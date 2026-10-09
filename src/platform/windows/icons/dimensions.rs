//! Measurement-only HICON dimensions; GetIconInfo creates owned bitmap copies.
use super::super::ffi::*;
use std::mem::size_of;

#[repr(C)]
#[derive(Default)]
struct IconInfo {
    icon: i32,
    x_hotspot: u32,
    y_hotspot: u32,
    mask: Handle,
    color: Handle,
}
impl Drop for IconInfo {
    fn drop(&mut self) {
        // GetIconInfo returns new GDI bitmaps; release both, including failed output.
        unsafe {
            if !self.color.is_null() {
                DeleteObject(self.color);
            }
            if !self.mask.is_null() {
                DeleteObject(self.mask);
            }
        }
    }
}
#[repr(C)]
#[derive(Default)]
struct Bitmap {
    kind: i32,
    width: i32,
    height: i32,
    stride: i32,
    planes: u16,
    bits_per_pixel: u16,
    bits: *mut std::ffi::c_void,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn GetIconInfo(icon: Handle, info: *mut IconInfo) -> i32;
}
pub(super) fn get(icon: Handle) -> Option<(i32, i32)> {
    let mut info = IconInfo::default();
    // Caller holds an Arc<Icon> and no Runtime borrow; copied bitmap owners remain
    // alive through GetObjectW and are freed after their dimensions have been copied.
    if unsafe { GetIconInfo(icon, &mut info) } == 0 {
        return None;
    }
    let mut bitmap = Bitmap::default();
    let handle = if info.color.is_null() {
        info.mask
    } else {
        info.color
    };
    if handle.is_null()
        || unsafe {
            GetObjectW(
                handle,
                size_of::<Bitmap>() as i32,
                (&mut bitmap as *mut Bitmap).cast(),
            )
        } == 0
    {
        return None;
    }
    let height = if info.color.is_null() {
        bitmap.height / 2
    } else {
        bitmap.height
    };
    (bitmap.width > 0 && height > 0).then_some((bitmap.width, height))
}
