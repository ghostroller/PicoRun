//! Win32 ABI declarations. Handles are borrowed unless their owning wrapper says otherwise.
#![allow(non_snake_case)]
use std::ffi::c_void;
pub type Handle = *mut c_void;
pub type Hwnd = Handle;
pub type WndProc = unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
#[repr(C)]
#[derive(Default)]
pub struct LogFont {
    pub height: i32,
    pub width: i32,
    pub escapement: i32,
    pub orientation: i32,
    pub weight: i32,
    pub italic: u8,
    pub underline: u8,
    pub strikeout: u8,
    pub charset: u8,
    pub out_precision: u8,
    pub clip_precision: u8,
    pub quality: u8,
    pub pitch: u8,
    pub face: [u16; 32],
}
#[repr(C)]
#[derive(Default)]
pub struct Message {
    pub hwnd: Hwnd,
    pub message: u32,
    pub wparam: usize,
    pub lparam: isize,
    pub time: u32,
    pub pt: Point,
    pub private: u32,
}
#[repr(C)]
pub struct WndClass {
    pub size: u32,
    pub style: u32,
    pub proc: Option<WndProc>,
    pub class_extra: i32,
    pub window_extra: i32,
    pub instance: Handle,
    pub icon: Handle,
    pub cursor: Handle,
    pub background: Handle,
    pub menu: *const u16,
    pub name: *const u16,
    pub small_icon: Handle,
}
#[repr(C)]
pub struct Paint {
    pub dc: Handle,
    pub erase: i32,
    pub rect: Rect,
    pub restore: i32,
    pub update: i32,
    pub reserved: [u8; 32],
}
impl Default for Paint {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}
#[repr(C)]
pub struct MonitorInfo {
    pub size: u32,
    pub monitor: Rect,
    pub work: Rect,
    pub flags: u32,
}
#[repr(C)]
#[derive(Default)]
pub struct GuiThreadInfo {
    pub size: u32,
    pub flags: u32,
    pub active: Hwnd,
    pub focus: Hwnd,
    pub capture: Hwnd,
    pub menu_owner: Hwnd,
    pub move_size: Hwnd,
    pub caret: Hwnd,
    pub caret_rect: Rect,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Guid {
    pub a: u32,
    pub b: u16,
    pub c: u16,
    pub d: [u8; 8],
}
#[repr(C)]
pub struct ShellExecuteInfo {
    pub size: u32,
    pub mask: u32,
    pub hwnd: Hwnd,
    pub verb: *const u16,
    pub file: *const u16,
    pub parameters: *const u16,
    pub directory: *const u16,
    pub show: i32,
    pub instance: Handle,
    pub id_list: *mut c_void,
    pub class: *const u16,
    pub class_key: Handle,
    pub hotkey: u32,
    pub icon_or_monitor: Handle,
    pub process: Handle,
}
#[repr(C)]
pub struct NotifyIconData {
    pub size: u32,
    pub hwnd: Hwnd,
    pub id: u32,
    pub flags: u32,
    pub callback: u32,
    pub icon: Handle,
    pub tip: [u16; 128],
    pub state: u32,
    pub state_mask: u32,
    pub info: [u16; 256],
    pub version: u32,
    pub info_title: [u16; 64],
    pub info_flags: u32,
    pub guid: Guid,
    pub balloon_icon: Handle,
}
#[repr(C)]
pub struct NotifyIconIdentifier {
    pub size: u32,
    pub hwnd: Hwnd,
    pub id: u32,
    pub guid: Guid,
}

#[link(name = "user32")]
unsafe extern "system" {
    pub fn RegisterWindowMessageW(name: *const u16) -> u32;
    pub fn CreateIcon(
        instance: Handle,
        width: i32,
        height: i32,
        planes: u8,
        bits: u8,
        and: *const u8,
        xor: *const u8,
    ) -> Handle;
    pub fn DestroyIcon(icon: Handle) -> i32;
    pub fn DrawIconEx(
        dc: Handle,
        x: i32,
        y: i32,
        icon: Handle,
        width: i32,
        height: i32,
        step: u32,
        brush: Handle,
        flags: u32,
    ) -> i32;
    pub fn CreatePopupMenu() -> Handle;
    pub fn AppendMenuW(menu: Handle, flags: u32, id: usize, text: *const u16) -> i32;
    pub fn CheckMenuRadioItem(menu: Handle, first: u32, last: u32, check: u32, flags: u32) -> i32;
    pub fn DestroyMenu(menu: Handle) -> i32;
    pub fn TrackPopupMenuEx(
        menu: Handle,
        flags: u32,
        x: i32,
        y: i32,
        hwnd: Hwnd,
        params: *const c_void,
    ) -> u32;
    pub fn GetCursorPos(point: *mut Point) -> i32;
    pub fn RegisterClassExW(class: *const WndClass) -> u16;
    pub fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: Handle,
        instance: Handle,
        param: *mut c_void,
    ) -> Hwnd;
    pub fn DefWindowProcW(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize;
    pub fn CallWindowProcW(
        proc: Option<WndProc>,
        hwnd: Hwnd,
        msg: u32,
        wp: usize,
        lp: isize,
    ) -> isize;
    pub fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: isize) -> isize;
    pub fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
    pub fn GetMessageW(msg: *mut Message, hwnd: Hwnd, min: u32, max: u32) -> i32;
    pub fn TranslateMessage(msg: *const Message) -> i32;
    pub fn DispatchMessageW(msg: *const Message) -> isize;
    pub fn PostQuitMessage(code: i32);
    pub fn PostMessageW(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> i32;
    pub fn SendMessageW(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize;
    pub fn SendMessageTimeoutW(
        hwnd: Hwnd,
        msg: u32,
        wp: usize,
        lp: isize,
        flags: u32,
        timeout_ms: u32,
        result: *mut usize,
    ) -> isize;
    pub fn DestroyWindow(hwnd: Hwnd) -> i32;
    pub fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
    pub fn IsWindowVisible(hwnd: Hwnd) -> i32;
    pub fn SetForegroundWindow(hwnd: Hwnd) -> i32;
    pub fn AllowSetForegroundWindow(process: u32) -> i32;
    pub fn GetForegroundWindow() -> Hwnd;
    pub fn GetGUIThreadInfo(thread: u32, info: *mut GuiThreadInfo) -> i32;
    pub fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
    pub fn SetFocus(hwnd: Hwnd) -> Hwnd;
    pub fn GetFocus() -> Hwnd;
    pub fn WindowFromDC(dc: Handle) -> Hwnd;
    pub fn GetSysColor(index: i32) -> u32;
    pub fn HideCaret(hwnd: Hwnd) -> i32;
    pub fn ShowCaret(hwnd: Hwnd) -> i32;
    pub fn SetWindowPos(
        hwnd: Hwnd,
        after: Hwnd,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
    pub fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    pub fn MoveWindow(hwnd: Hwnd, x: i32, y: i32, cx: i32, cy: i32, repaint: i32) -> i32;
    pub fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
    pub fn GetWindowTextW(hwnd: Hwnd, text: *mut u16, max: i32) -> i32;
    pub fn SetWindowTextW(hwnd: Hwnd, text: *const u16) -> i32;
    pub fn FindWindowW(class: *const u16, title: *const u16) -> Hwnd;
    pub fn FindWindowExW(parent: Hwnd, after: Hwnd, class: *const u16, title: *const u16) -> Hwnd;
    pub fn RegisterHotKey(hwnd: Hwnd, id: i32, modifiers: u32, key: u32) -> i32;
    pub fn UnregisterHotKey(hwnd: Hwnd, id: i32) -> i32;
    pub fn LoadCursorW(instance: Handle, name: *const u16) -> Handle;
    pub fn MessageBoxW(hwnd: Hwnd, text: *const u16, title: *const u16, flags: u32) -> i32;
    pub fn GetKeyState(key: i32) -> i16;
    pub fn GetKeyboardLayout(thread: u32) -> Handle;
    pub fn GetKeyboardLayoutList(count: i32, layouts: *mut Handle) -> i32;
    pub fn ActivateKeyboardLayout(layout: Handle, flags: u32) -> Handle;
    pub fn BeginPaint(hwnd: Hwnd, paint: *mut Paint) -> Handle;
    pub fn EndPaint(hwnd: Hwnd, paint: *const Paint) -> i32;
    pub fn FillRect(dc: Handle, rect: *const Rect, brush: Handle) -> i32;
    pub fn DrawTextW(dc: Handle, text: *const u16, count: i32, rect: *mut Rect, flags: u32) -> i32;
    pub fn InvalidateRect(hwnd: Hwnd, rect: *const Rect, erase: i32) -> i32;
    pub fn UpdateWindow(hwnd: Hwnd) -> i32;
    pub fn GetUpdateRect(hwnd: Hwnd, rect: *mut Rect, erase: i32) -> i32;
    pub fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    pub fn MonitorFromWindow(hwnd: Hwnd, flags: u32) -> Handle;
    pub fn GetMonitorInfoW(monitor: Handle, info: *mut MonitorInfo) -> i32;
    pub fn SetProcessDPIAware() -> i32;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    pub fn CreateSolidBrush(color: u32) -> Handle;
    pub fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strikeout: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> Handle;
    pub fn DeleteObject(object: Handle) -> i32;
    pub fn GetObjectW(object: Handle, size: i32, buffer: *mut c_void) -> i32;
    pub fn SelectObject(dc: Handle, object: Handle) -> Handle;
    pub fn CreateCompatibleDC(dc: Handle) -> Handle;
    pub fn CreateCompatibleBitmap(dc: Handle, width: i32, height: i32) -> Handle;
    pub fn GdiFlush() -> i32;
    pub fn CreateDIBSection(
        dc: Handle,
        info: *const c_void,
        usage: u32,
        pixels: *mut *mut c_void,
        mapping: Handle,
        offset: u32,
    ) -> Handle;
    pub fn DeleteDC(dc: Handle) -> i32;
    pub fn BitBlt(
        destination: Handle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        source: Handle,
        source_x: i32,
        source_y: i32,
        operation: u32,
    ) -> i32;
    pub fn SetTextColor(dc: Handle, color: u32) -> u32;
    pub fn SetBkColor(dc: Handle, color: u32) -> u32;
    pub fn IntersectClipRect(dc: Handle, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    pub fn SetBkMode(dc: Handle, mode: i32) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn GetModuleHandleW(name: *const u16) -> Handle;
    pub fn GetProcAddress(module: Handle, name: *const u8) -> *mut c_void;
    pub fn GetLastError() -> u32;
    pub fn SetLastError(error: u32);
    pub fn CreateMutexW(attributes: *const c_void, owner: i32, name: *const u16) -> Handle;
    pub fn ReleaseMutex(mutex: Handle) -> i32;
    pub fn CreateEventW(
        attributes: *const c_void,
        manual: i32,
        initial: i32,
        name: *const u16,
    ) -> Handle;
    pub fn SetEvent(event: Handle) -> i32;
    pub fn ResetEvent(event: Handle) -> i32;
    pub fn WaitForMultipleObjects(
        count: u32,
        handles: *const Handle,
        all: i32,
        timeout: u32,
    ) -> u32;
    pub fn CloseHandle(handle: Handle) -> i32;
    pub fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    pub fn AttachConsole(process: u32) -> i32;
    pub fn GetPackagesByPackageFamily(
        family: *const u16,
        count: *mut u32,
        names: *mut *mut u16,
        buffer_len: *mut u32,
        buffer: *mut u16,
    ) -> i32;
}
#[link(name = "shell32")]
unsafe extern "system" {
    pub fn Shell_NotifyIconW(message: u32, data: *const NotifyIconData) -> i32;
    pub fn Shell_NotifyIconGetRect(id: *const NotifyIconIdentifier, rect: *mut Rect) -> i32;
    pub fn SHGetKnownFolderPath(
        id: *const Guid,
        flags: u32,
        token: Handle,
        path: *mut *mut u16,
    ) -> i32;
    pub fn SHCreateItemFromParsingName(
        name: *const u16,
        bind_ctx: *mut c_void,
        iid: *const Guid,
        object: *mut *mut c_void,
    ) -> i32;
    pub fn ShellExecuteW(
        hwnd: Hwnd,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> Handle;
    pub fn ShellExecuteExW(info: *mut ShellExecuteInfo) -> i32;
}
#[link(name = "ole32")]
unsafe extern "system" {
    pub fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
    pub fn CoUninitialize();
    pub fn CoTaskMemFree(memory: *mut c_void);
    pub fn PropVariantClear(value: *mut c_void) -> i32;
    pub fn CoCreateInstance(
        class: *const Guid,
        outer: *mut c_void,
        context: u32,
        iid: *const Guid,
        object: *mut *mut c_void,
    ) -> i32;
}
#[link(name = "imm32")]
unsafe extern "system" {
    pub fn ImmGetContext(hwnd: Hwnd) -> Handle;
    pub fn ImmGetOpenStatus(context: Handle) -> i32;
    pub fn ImmSetOpenStatus(context: Handle, open: i32) -> i32;
    pub fn ImmGetConversionStatus(context: Handle, conversion: *mut u32, sentence: *mut u32)
        -> i32;
    pub fn ImmSetConversionStatus(context: Handle, conversion: u32, sentence: u32) -> i32;
    pub fn ImmGetDefaultIMEWnd(hwnd: Hwnd) -> Hwnd;
    pub fn ImmGetVirtualKey(hwnd: Hwnd) -> u32;
    pub fn ImmSetCompositionFontW(context: Handle, font: *const LogFont) -> i32;
    pub fn ImmNotifyIME(context: Handle, action: u32, index: u32, value: u32) -> i32;
    pub fn ImmReleaseContext(hwnd: Hwnd, context: Handle) -> i32;
    pub fn ImmGetCompositionStringW(
        context: Handle,
        index: u32,
        buffer: *mut c_void,
        size: u32,
    ) -> i32;
}
