//! System discovery, hotkeys, app launching, and process lifetime live here.
//! Keep Windows FFI away from search, pinyin, theme, and the controller.
use crate::model::LaunchTarget;
use std::io;

pub trait AppLauncher {
    fn launch(&self, target: &LaunchTarget) -> io::Result<()>;
}

#[cfg(windows)]
pub mod windows;
