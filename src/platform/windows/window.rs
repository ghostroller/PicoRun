use super::{
    discovery, ffi::*, icons, ime, input_language, settings, startup, tray, wide, Hotkey,
    DEFAULT_HOTKEY,
};
mod diagnostics;
use crate::{
    cache,
    catalog::Catalog,
    i18n::{self, Language, Notice, Text},
    theme::ThemeMode,
    ui::{
        controller::Controller,
        native::{color, Renderer, View},
    },
};
use std::{
    cell::{Cell, RefCell},
    io,
    path::PathBuf,
    ptr::{null, null_mut},
    rc::Rc,
};

const CLASS: &str = "PicoRun.Native.v1";
const SHOW: u32 = 0x8001;
const REFRESH: u32 = 0x8002;
const INPUT_CHANGED: u32 = 0x8003;
pub struct Options {
    pub hotkey: String,
    pub data_dir: Option<PathBuf>,
    pub sources: Vec<PathBuf>,
    pub hidden: bool,
    pub theme: Option<ThemeMode>,
    pub icons: Option<bool>,
    /// Enable own-window read-only counters/query probes. Normal input/focus/hotkeys stay active.
    pub measure_icons: bool,
    /// Test driver can hold the measurement window visible, matching the frozen prototype.
    pub hold_measurement_window: bool,
    /// Fixed non-Run registry namespace for native verification; never an actual autorun entry.
    pub startup_probe: Option<String>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.into(),
            data_dir: None,
            sources: Vec::new(),
            hidden: false,
            theme: None,
            icons: None,
            measure_icons: false,
            hold_measurement_window: false,
            startup_probe: None,
        }
    }
}
struct Runtime {
    controller: Controller,
    renderer: Rc<Renderer>,
    view: View,
    view_dirty: bool,
    theme_mode: ThemeMode,
    settings: PathBuf,
    start_english: bool,
    input_settings: PathBuf,
    tray: Option<Rc<tray::Tray>>,
    roots: Vec<PathBuf>,
    cache: PathBuf,
    status: Notice,
    status_dirty: bool,
    language_settings: PathBuf,
    text_buffer: Vec<u16>,
    text: String,
    refreshing: bool,
    system_sources: bool,
    icons: Option<Rc<icons::Session>>,
    icon_settings: PathBuf,
    measure_icons: bool,
    hold_measurement_window: bool,
    startup: Rc<startup::Registration>,
}
thread_local! {
    static STATE: RefCell<Option<Runtime>> = const { RefCell::new(None) };
    static WINDOW: Cell<Hwnd> = const { Cell::new(null_mut()) };
    static EDIT: Cell<Hwnd> = const { Cell::new(null_mut()) };
    static EDIT_PROC: Cell<Option<WndProc>> = const { Cell::new(None) };
    static EDIT_REDRAW_ENABLED: Cell<bool> = const { Cell::new(true) };
    static EDIT_UPDATING: Cell<bool> = const { Cell::new(false) };
    static IME: Cell<ime::State> = const { Cell::new(ime::State::EMPTY) };
    static INPUT_PENDING: Cell<bool> = const { Cell::new(false) };
    static HOTKEY_SET: Cell<bool> = const { Cell::new(false) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
    static TRAY_MENU_OPEN: Cell<bool> = const { Cell::new(false) };
}
fn state<T>(f: impl FnOnce(&mut Runtime) -> T) -> Option<T> {
    STATE.with(|cell| cell.borrow_mut().as_mut().map(f))
}
fn renderer() -> Option<Rc<Renderer>> {
    state(|s| Rc::clone(&s.renderer))
}
fn update_ime(f: impl FnOnce(&mut ime::State)) {
    let mut state = IME.get();
    f(&mut state);
    IME.set(state);
}
fn post_query() {
    if !INPUT_PENDING.replace(true) {
        unsafe {
            PostMessageW(WINDOW.get(), INPUT_CHANGED, 0, 0);
        }
    }
}
fn view() -> Option<View> {
    state(|s| {
        if s.view_dirty {
            s.view.rows = s
                .controller
                .results()
                .iter()
                .map(|hit| {
                    s.controller.catalog().entries()[hit.entry_index]
                        .name
                        .encode_utf16()
                        .collect()
                })
                .collect();
            s.view_dirty = false;
        }
        s.view.selected = s.controller.selected_index();
        if s.status_dirty {
            s.view.status = s.status.to_string().encode_utf16().collect();
            s.status_dirty = false;
        }
        s.view.clone()
    })
}
fn repaint(resize: bool) {
    let hwnd = WINDOW.get();
    if state(|s| s.view.show_icons).unwrap_or(false) && unsafe { IsWindowVisible(hwnd) } != 0 {
        request_icons();
    }
    if let Some(renderer) = renderer() {
        if resize {
            let rows = state(|s| s.controller.results().len()).unwrap_or(0);
            unsafe {
                let mut client = Rect::default();
                GetClientRect(hwnd, &mut client);
                let width = renderer.scale(renderer.theme.width);
                let height = renderer.height(rows);
                if client.right != width || client.bottom != height {
                    SetWindowPos(hwnd, null_mut(), 0, 0, width, height, 0x16);
                }
            }
        }
        renderer.invalidate(hwnd);
    }
}
fn request_icons() {
    let Some(session) = state(|s| {
        if !s.view.show_icons {
            return None;
        }
        Some(Rc::clone(s.icons.get_or_insert_with(|| {
            Rc::new(icons::Session::new(WINDOW.get()))
        })))
    })
    .flatten() else {
        return;
    };
    let paths = state(|s| {
        s.controller
            .results()
            .iter()
            .map(|hit| {
                match &s.controller.catalog().entries()[hit.entry_index].target {
                    crate::model::LaunchTarget::ShellPath(path) => path.clone(),
                    // Preserve row alignment for unsupported Store entries, without file access.
                    _ => PathBuf::new(),
                }
            })
            .collect()
    })
    .unwrap_or_default();
    let _ = view();
    match session.request(paths) {
        Ok(true) => {
            let previous = state(|s| {
                std::mem::replace(&mut s.view.icons, vec![None; s.view.rows.len()].into())
            });
            drop(previous); // HICON destruction must happen outside the Runtime borrow.
        }
        Ok(false) => {}
        Err(error) => {
            drop(session);
            set_icons(false);
            report(Notice::error(Text::IconLoad, error));
        }
    }
}
fn set_icons(enabled: bool) {
    let previous = state(|s| {
        s.view.show_icons = enabled;
        (s.icons.take(), std::mem::take(&mut s.view.icons))
    });
    // Shutdown can wait for an in-flight extraction. No mutable UI borrow survives the join,
    // channel destruction or native icon release. A late READY message cannot restore old icons.
    drop(previous);
    repaint(false);
}
fn toggle_icons() {
    let Some((enabled, path)) = state(|s| (!s.view.show_icons, s.icon_settings.clone())) else {
        return;
    };
    set_icons(enabled);
    if settings::save_toggle(&path, enabled).is_err() {
        report(Text::IconSave);
    }
}
fn invalidate_icons() {
    if let Some(session) = state(|s| s.icons.clone()).flatten() {
        session.invalidate();
    }
}
fn sync_query() -> bool {
    let edit = EDIT.get();
    if edit.is_null() {
        return false;
    }
    // Leave native preedit text/candidate geometry alone. Read the completed Edit text on end/close.
    if is_composing() {
        return false;
    }
    let Some((mut buffer, mut text)) = state(|s| {
        (
            std::mem::take(&mut s.text_buffer),
            std::mem::take(&mut s.text),
        )
    }) else {
        return false;
    };
    // Buffer is local, with no Rust borrow of Runtime while the native Edit can reenter.
    unsafe {
        let length = GetWindowTextLengthW(edit).max(0) as usize;
        buffer.resize(length + 1, 0);
        let read = GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
        text.clear();
        text.extend(
            std::char::decode_utf16(buffer[..read].iter().copied())
                .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)),
        );
    }
    let changed = state(|s| {
        let changed = s.controller.set_query(&text);
        s.view_dirty |= changed;
        s.text = text;
        s.text_buffer = buffer;
        changed
    })
    .unwrap_or(false);
    if changed {
        repaint(true);
    }
    changed
}
fn show() {
    let hwnd = WINDOW.get();
    if hwnd.is_null() {
        return;
    }
    if state(|s| s.start_english).unwrap_or(false) {
        if let Err(error) = input_language::remember(hwnd, EDIT.get(), true) {
            report(Notice::error(Text::InputBackup, error));
        }
    }
    unsafe {
        cancel_composition();
        // Retain the native Edit text/results between invocations. Select all after focus so
        // the next input replaces the previous query without clearing it just by opening.
        sync_query();
        let foreground = GetForegroundWindow();
        let monitor = MonitorFromWindow(foreground, 2);
        let mut info = MonitorInfo {
            size: std::mem::size_of::<MonitorInfo>() as u32,
            monitor: Rect::default(),
            work: Rect::default(),
            flags: 0,
        };
        if let Some(renderer) = renderer() {
            let rows = state(|s| s.controller.results().len()).unwrap_or(0);
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                let bounds = renderer.placement(info.work, rows);
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    bounds.left,
                    bounds.top,
                    bounds.right - bounds.left,
                    bounds.bottom - bounds.top,
                    0x14,
                );
                // Moving to another monitor can synchronously replace the renderer/DPI.
                if let Some(updated) = self::renderer().filter(|r| r.dpi != renderer.dpi) {
                    let bounds = updated.placement(info.work, rows);
                    SetWindowPos(
                        hwnd,
                        null_mut(),
                        bounds.left,
                        bounds.top,
                        bounds.right - bounds.left,
                        bounds.bottom - bounds.top,
                        0x14,
                    );
                }
            }
        }
        // The backup precedes activation; apply English after TSF focuses the Edit.
        ShowWindow(hwnd, 5);
        SetForegroundWindow(hwnd);
        SetFocus(EDIT.get());
        if state(|s| s.start_english).unwrap_or(false) && IsWindowVisible(hwnd) != 0 {
            if let Err(error) = input_language::begin(hwnd, EDIT.get()) {
                report(Notice::error(Text::InputEnglish, error));
            }
        }
        SendMessageW(EDIT.get(), 0xb1, 0, -1);
        request_icons();
        UpdateWindow(hwnd);
    }
}
fn hide() {
    cancel_composition();
    // Restore while Edit still has its input context. Restoring after focus loss can leave
    // TSF remembering the English layout and selecting it again on the next invocation.
    if let Err(error) = input_language::restore() {
        report(Notice::error(Text::InputRestore, error));
    }
    unsafe {
        ShowWindow(WINDOW.get(), 0);
    }
    if let Some(renderer) = renderer() {
        renderer.release_row_buffer();
        renderer.release_edit_buffer();
    }
}
fn close() {
    cancel_composition();
    if let Err(error) = input_language::restore() {
        report(Notice::error(Text::InputRestore, error));
    }
    unsafe { DestroyWindow(WINDOW.get()) };
}
fn is_composing() -> bool {
    if IME.get().active() {
        return true;
    }
    unsafe {
        let context = ImmGetContext(EDIT.get());
        if context.is_null() {
            return false;
        }
        let composing = ImmGetCompositionStringW(context, 8, null_mut(), 0) > 0;
        ImmReleaseContext(EDIT.get(), context);
        composing
    }
}
fn cancel_composition() {
    // Only our Edit's context is affected. No input method/layout or global open status is changed.
    if is_composing() {
        unsafe {
            let edit = EDIT.get();
            let context = ImmGetContext(edit);
            if !context.is_null() {
                ImmNotifyIME(context, 0x15, 4, 0); // NI_COMPOSITIONSTR / CPS_CANCEL.
                ImmReleaseContext(edit, context);
            }
        }
    }
    IME.set(ime::State::EMPTY);
}
fn update_ime_font(hwnd: Hwnd) {
    // Read the control's actual font: WM_SETFONT can precede the Runtime swap on a DPI change.
    // The renderer owns the HFONT; copy its LOGFONT before IMM can reenter and change the theme.
    // No Runtime borrow survives these calls or the resulting IMN_SETCOMPOSITIONFONT.
    unsafe {
        let handle = CallWindowProcW(EDIT_PROC.get(), hwnd, 0x31, 0, 0) as Handle; // WM_GETFONT.
        let mut font = LogFont::default();
        let size = std::mem::size_of::<LogFont>() as i32;
        if GetObjectW(handle, size, (&mut font as *mut LogFont).cast()) != size {
            return;
        }
        let context = ImmGetContext(hwnd);
        if !context.is_null() {
            ImmSetCompositionFontW(context, &font);
            ImmReleaseContext(hwnd, context);
        }
    }
}
fn before_translate(message: &Message) {
    if message.hwnd == EDIT.get() && message.message == 0x100 {
        if message.wparam == 0xe5 {
            // VK_PROCESSKEY: recover the key before TranslateMessage changes it.
            let key = unsafe { ImmGetVirtualKey(message.hwnd) };
            update_ime(|s| s.claim_key(key));
        } else {
            let active = is_composing();
            update_ime(|s| s.key_down(message.wparam as u32, active));
        }
    }
}
fn activate() {
    if is_composing() {
        return;
    }
    // Flush even a coalesced EN_CHANGE before selecting an application.
    sync_query();
    let target = state(|s| s.controller.selected_target().cloned()).flatten();
    let Some(target) = target else { return };
    hide();
    if let Err(error) = discovery::launch(WINDOW.get(), &target) {
        state(|s| {
            s.status = error.into();
            s.status_dirty = true;
        });
        // Keep the failing query available for correction without changing its selection.
        if state(|s| s.start_english).unwrap_or(false) {
            let _ = input_language::remember(WINDOW.get(), EDIT.get(), true);
        }
        unsafe {
            ShowWindow(WINDOW.get(), 5);
            SetForegroundWindow(WINDOW.get());
            SetFocus(EDIT.get());
        }
        repaint(false);
    }
}
fn refresh() {
    let Some((mut roots, path, system_sources)) = state(|s| {
        if s.refreshing {
            return None;
        }
        s.refreshing = true;
        s.status = Text::Refreshing.into();
        s.status_dirty = true;
        Some((s.roots.clone(), s.cache.clone(), s.system_sources))
    })
    .flatten() else {
        return;
    };
    repaint(false);
    unsafe {
        UpdateWindow(WINDOW.get());
    }
    // COM/IO can dispatch messages. Neither Runtime nor controller is borrowed here.
    let unavailable = if system_sources {
        let found = discovery::roots();
        roots = found.0;
        found.1
    } else {
        0
    };
    let result = discovery::discover(&roots, None);
    match result {
        Ok(mut scan) => {
            // Preserve only unreadable entries after native discovery returns. No full index clone.
            state(|s| scan.preserve_unreadable(s.controller.catalog()));
            let catalog = Catalog::new(scan.entries);
            let count = catalog.entries().len();
            let saved = cache::save(&path, &catalog);
            let status = Notice::Catalog {
                count,
                failed: scan.failed.len() + unavailable,
                refreshed: true,
                cache_failed: saved.is_err(),
                source: None,
            };
            state(|s| {
                s.controller.replace_catalog(catalog);
                s.view_dirty = true;
                s.roots = roots;
                s.status = status;
                s.status_dirty = true;
                s.refreshing = false;
            });
        }
        Err(error) => {
            state(|s| {
                s.status = error.into();
                s.status_dirty = true;
                s.refreshing = false;
            });
        }
    }
    invalidate_icons();
    sync_query();
    repaint(true);
}
fn report(notice: impl Into<Notice>) {
    state(|s| {
        s.status = notice.into();
        s.status_dirty = true;
    });
    repaint(false);
}
fn change_theme(mode: ThemeMode) {
    let Some((current, dpi, path)) = state(|s| (s.theme_mode, s.renderer.dpi, s.settings.clone()))
    else {
        return;
    };
    if current == mode {
        if settings::save(&path, mode).is_err() {
            report(Text::ThemeSave);
        }
        return;
    }
    match Renderer::new(mode.theme(), dpi) {
        Ok(new) => {
            let new = Rc::new(new);
            // Keep the previous owner alive while switching the Edit font. No mutable state borrow.
            let previous = renderer();
            state(|s| {
                s.renderer = Rc::clone(&new);
                s.theme_mode = mode;
            });
            unsafe {
                SendMessageW(EDIT.get(), 0x30, new.font as usize, 1);
                InvalidateRect(EDIT.get(), null(), 1);
            }
            drop(previous);
            if settings::save(&path, mode).is_err() {
                report(Text::ThemeChangedSave);
            }
            repaint(false);
        }
        Err(error) => report(Notice::error(Text::ThemeChange, error)),
    }
}
fn change_language(language: Language) {
    let Some(path) = state(|s| {
        i18n::set(language);
        s.view.set_language(language);
        s.status_dirty = true;
        s.language_settings.clone()
    }) else {
        return;
    };
    // Only the cached UI text changes. No Runtime borrow survives native invalidation.
    unsafe {
        InvalidateRect(EDIT.get(), null(), 1);
    }
    if settings::save_language(&path, language).is_err() {
        report(Text::LanguageSave);
    }
    repaint(false);
}
fn toggle_english() {
    let Some((enabled, path)) = state(|s| {
        s.start_english = !s.start_english;
        (s.start_english, s.input_settings.clone())
    }) else {
        return;
    };
    cancel_composition();
    let result = if enabled && unsafe { IsWindowVisible(WINDOW.get()) != 0 } {
        input_language::begin(WINDOW.get(), EDIT.get())
    } else {
        input_language::restore()
    };
    if let Err(error) = result {
        report(Notice::error(Text::InputChange, error));
    }
    if settings::save_english(&path, enabled).is_err() {
        report(Text::InputSave);
    }
}
fn toggle_startup() {
    let Some(registration) = state(|s| Rc::clone(&s.startup)) else {
        return;
    };
    // Registry calls and reporting run after releasing the Runtime borrow.
    match registration.enabled().and_then(|enabled| {
        registration.set(!enabled)?;
        Ok(!enabled)
    }) {
        Ok(true) => report(Text::StartupOn),
        Ok(false) => report(Text::StartupOff),
        Err(error) => report(Notice::error(Text::StartupChange, error)),
    }
}
fn tray_command(command: u32) {
    match command {
        tray::SHOW => show(),
        tray::REFRESH => refresh(),
        tray::LIGHT => change_theme(ThemeMode::Light),
        tray::DARK => change_theme(ThemeMode::Dark),
        tray::ENGLISH => toggle_english(),
        tray::ICONS => toggle_icons(),
        tray::STARTUP => toggle_startup(),
        tray::CHINESE_UI => change_language(Language::Chinese),
        tray::ENGLISH_UI => change_language(Language::English),
        tray::EXIT => close(),
        _ => {}
    }
}
fn edit_changes_pixels(msg: u32, wp: usize) -> bool {
    matches!(
        msg,
        0xb1 | 0xb7 | 0xc | 0xc2 | 0x100 | 0x102 | 0x201 | 0x202 | 0x203 | 0x300..=0x304
    ) || msg == 0x200 && wp & 1 != 0
}
unsafe fn forward_edit(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize {
    let changes_pixels = edit_changes_pixels(msg, wp);
    // Edit can draw selection changes directly to its window DC, bypassing WM_PAINT.
    // WM_CTLCOLOREDIT clips that intermediate DC, then we publish one buffered repaint.
    // No Runtime borrow spans these calls. Reentrant updates belong to the outer call;
    // native selection, hit testing and scrolling stay enabled throughout.
    let updating = changes_pixels
        && !is_composing()
        && EDIT_REDRAW_ENABLED.get()
        && !EDIT_UPDATING.get()
        && GetWindowLongPtrW(hwnd, -16) & 0x10000000 != 0;
    if updating {
        EDIT_UPDATING.set(true);
    }
    let result = CallWindowProcW(EDIT_PROC.get(), hwnd, msg, wp, lp);
    if updating {
        EDIT_UPDATING.set(false);
    }
    if changes_pixels && !EDIT_UPDATING.get() && EDIT_REDRAW_ENABLED.get() && EDIT.get() == hwnd {
        InvalidateRect(hwnd, null(), 0);
        UpdateWindow(hwnd);
    }
    result
}
unsafe extern "system" fn edit_proc(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize {
    if msg == 0xf && state(|s| s.measure_icons).unwrap_or(false) {
        diagnostics::edit_paint();
    }
    match msg {
        0xb => EDIT_REDRAW_ENABLED.set(wp != 0),
        0x82 => {
            EDIT_REDRAW_ENABLED.set(false);
            EDIT.set(null_mut());
        }
        0xf if EDIT_UPDATING.get() && !is_composing() || !EDIT_REDRAW_ENABLED.get() => {
            let mut paint = Paint::default();
            BeginPaint(hwnd, &mut paint);
            EndPaint(hwnd, &paint);
            return 0;
        }
        0xf | 0x318 if !is_composing() => {
            if let (Some(renderer), Some(cue)) = (renderer(), state(|s| Rc::clone(&s.view.cue))) {
                if renderer.paint_edit(
                    hwnd,
                    EDIT_PROC.get(),
                    (msg == 0x318).then_some(wp as Handle),
                    &cue,
                ) {
                    return 0;
                }
            }
        }
        0x7 => {
            let result = CallWindowProcW(EDIT_PROC.get(), hwnd, msg, wp, lp);
            if state(|s| s.start_english).unwrap_or(false)
                && IsWindowVisible(WINDOW.get()) != 0
                && GetForegroundWindow() == WINDOW.get()
            {
                if let Err(error) = input_language::begin(WINDOW.get(), hwnd) {
                    report(Notice::error(Text::InputEnglish, error));
                }
            }
            return result;
        }
        0x10d => {
            update_ime(|s| s.start());
            if let Some(renderer) = renderer() {
                renderer.release_edit_buffer();
            }
            update_ime_font(hwnd);
        }
        0x30 => {
            let result = CallWindowProcW(EDIT_PROC.get(), hwnd, msg, wp, lp);
            update_ime_font(hwnd);
            return result;
        }
        0x10e => {
            // The original control commits its result before the coalesced search reads it.
            let result = CallWindowProcW(EDIT_PROC.get(), hwnd, msg, wp, lp);
            update_ime(|s| s.end());
            post_query();
            return result;
        }
        0x282 if wp == 5 || wp == 3 => update_ime(|s| s.candidates_open(lp as u32)),
        0x282 if wp == 4 => {
            let result = CallWindowProcW(EDIT_PROC.get(), hwnd, msg, wp, lp);
            update_ime(|s| s.candidates_close(lp as u32));
            post_query();
            return result;
        }
        0x290 => update_ime(|s| s.claim_key(wp as u32)), // WM_IME_KEYDOWN: never a launcher command.
        0x101 | 0x291 => update_ime(|s| s.key_up(wp as u32)),
        0x8 => {
            if let Err(error) = input_language::restore() {
                report(Notice::error(Text::InputRestore, error));
            }
            let result = CallWindowProcW(EDIT_PROC.get(), hwnd, msg, wp, lp);
            IME.set(ime::State::EMPTY); // No stale state after focus loss/cancel or a missed end notification.
            post_query();
            return result;
        }
        0x100 => {
            let active = is_composing();
            update_ime(|s| s.key_down(wp as u32, active));
            if active {
                return CallWindowProcW(EDIT_PROC.get(), hwnd, msg, wp, lp);
            }
            if IME.get().claimed(wp as u32) {
                return 0;
            }
            match wp {
                0x26 | 0x28 => {
                    sync_query();
                    let changed = state(|s| {
                        let previous = s.controller.selected_index();
                        s.controller
                            .select_relative(if wp == 0x26 { -1 } else { 1 });
                        (previous, s.controller.selected_index())
                    });
                    if let (Some(renderer), Some((previous, selected))) = (renderer(), changed) {
                        renderer.invalidate_selection(WINDOW.get(), previous, selected);
                    }
                    return 0;
                }
                0x0d => {
                    activate();
                    return 0;
                }
                0x1b => {
                    hide();
                    return 0;
                }
                0x74 => {
                    PostMessageW(WINDOW.get(), REFRESH, 0, 0);
                    return 0;
                }
                0x51 if GetKeyState(0x11) < 0 => {
                    close();
                    return 0;
                }
                _ => {}
            }
        }
        // Suppress the CR/Escape/control-Q character generated by TranslateMessage after our command.
        0x102 if [13, 27, 17].contains(&wp) && !is_composing() => return 0,
        _ => {}
    }
    forward_edit(hwnd, msg, wp, lp)
}
unsafe extern "system" fn window_proc(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize {
    if state(|s| s.measure_icons).unwrap_or(false) {
        if let Some(result) = diagnostics::message(hwnd, msg, wp) {
            return result;
        }
    }
    if msg == icons::READY {
        if let Some(session) = state(|s| s.icons.clone()).flatten() {
            if let Some(result) = session.receive() {
                if result.failed {
                    drop(session);
                    set_icons(false);
                    report(Notice::error(
                        Text::IconLoad,
                        io::Error::other(Text::ShellCom),
                    ));
                } else {
                    let previous =
                        state(|s| std::mem::replace(&mut s.view.icons, result.icons.into()));
                    drop(previous);
                    if let Some(renderer) = renderer() {
                        let rows = state(|s| s.view.rows.len()).unwrap_or(0);
                        renderer.invalidate_icons(hwnd, rows);
                    }
                }
            }
        }
        return 0;
    }
    if msg != 0 && msg == TASKBAR_CREATED.get() {
        if let Some(tray) = state(|s| s.tray.clone()).flatten() {
            if let Err(error) = tray.install() {
                report(error);
                show();
            }
        }
        return 0;
    }
    match msg {
        0x1 => {
            WINDOW.set(hwnd);
            let edit = CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                wide("").as_ptr(),
                0x50000000 | 0x80,
                12,
                12,
                500,
                32,
                hwnd,
                1usize as Handle,
                GetModuleHandleW(null()),
                null_mut(),
            );
            if edit.is_null() {
                return -1;
            }
            EDIT.set(edit);
            EDIT_REDRAW_ENABLED.set(true);
            EDIT_UPDATING.set(false);
            let previous = SetWindowLongPtrW(edit, -4, edit_proc as *const () as isize);
            EDIT_PROC.set(Some(std::mem::transmute::<isize, WndProc>(previous)));
            SendMessageW(edit, 0xc5, 1024, 0); // EM_SETLIMITTEXT: bound query memory.
            if let Some(renderer) = renderer() {
                SendMessageW(edit, 0x30, renderer.font as usize, 1);
            }
            return 0;
        }
        0x111 if (wp >> 16) & 0xffff == 0x300 && lp == EDIT.get() as isize => {
            post_query();
            return 0;
        }
        0x111 if lp == 0 && wp >> 16 == 0 => {
            tray_command(wp as u32);
            return 0;
        }
        tray::CALLBACK if (lp as u32 >> 16) == tray::ICON_ID => {
            match lp as u32 & 0xffff {
                0x400 | 0x401 => show(), // NIN_SELECT / NIN_KEYSELECT; version 4 handles clicks once.
                0x7b => {
                    if TRAY_MENU_OPEN.replace(true) {
                        return 0;
                    }
                    if let Some((tray, mode, start_english, show_icons, startup)) = state(|s| {
                        s.tray.clone().map(|t| {
                            (
                                t,
                                s.theme_mode,
                                s.start_english,
                                s.view.show_icons,
                                Rc::clone(&s.startup),
                            )
                        })
                    })
                    .flatten()
                    {
                        let point = Point {
                            x: wp as u16 as i16 as i32,
                            y: (wp >> 16) as u16 as i16 as i32,
                        };
                        let startup_enabled = match startup.enabled() {
                            Ok(enabled) => enabled,
                            Err(error) => {
                                report(Notice::error(Text::StartupRead, error));
                                false
                            }
                        };
                        match tray.menu(mode, start_english, show_icons, startup_enabled, point) {
                            Ok(command) => tray_command(command),
                            Err(error) => report(error),
                        }
                    }
                    TRAY_MENU_OPEN.set(false);
                }
                _ => {}
            }
            return 0;
        }
        INPUT_CHANGED => {
            INPUT_PENDING.set(false);
            sync_query();
            return 0;
        }
        SHOW => {
            show();
            return 0;
        }
        REFRESH => {
            refresh();
            return 0;
        }
        0x312 => {
            if IsWindowVisible(hwnd) != 0 {
                hide();
            } else {
                show();
            }
            return 0;
        }
        0x6 if wp & 0xffff == 0 => {
            if state(|s| s.measure_icons && s.hold_measurement_window).unwrap_or(false) {
                return 0;
            }
            hide();
            return 0;
        }
        0x5 => {
            if let Some(renderer) = renderer() {
                let pad = renderer.scale(renderer.theme.padding);
                let mut client = Rect::default();
                GetClientRect(hwnd, &mut client);
                MoveWindow(
                    EDIT.get(),
                    pad,
                    pad,
                    client.right - pad * 2,
                    renderer.scale(32),
                    1,
                );
            }
            return 0;
        }
        0x2e0 => {
            let dpi = (wp & 0xffff) as u32;
            if let Some(theme) = state(|s| s.renderer.theme.clone()) {
                if let Ok(new) = Renderer::new(theme, dpi) {
                    let new = Rc::new(new);
                    SendMessageW(EDIT.get(), 0x30, new.font as usize, 1);
                    state(|s| s.renderer = new);
                    let rect = *(lp as *const Rect);
                    SetWindowPos(
                        hwnd,
                        null_mut(),
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        0x14,
                    );
                    invalidate_icons();
                    repaint(true);
                }
            }
            return 0;
        }
        0x133 => {
            if lp == EDIT.get() as isize
                && EDIT_UPDATING.get()
                && !is_composing()
                && WindowFromDC(wp as Handle) == EDIT.get()
            {
                // This borrowed window DC is released by native Edit, resetting the
                // common DC's clipping. Do not clip DIB/printer targets used for printing.
                IntersectClipRect(wp as Handle, 0, 0, 0, 0);
            }
            if let Some(renderer) = renderer() {
                SetTextColor(wp as Handle, color(renderer.theme.foreground));
                SetBkColor(wp as Handle, color(renderer.theme.background));
                return renderer.background as isize;
            }
        }
        0xf => {
            if let (Some(renderer), Some(view)) = (renderer(), view()) {
                renderer.paint(hwnd, &view);
            }
            return 0;
        }
        0x318 => {
            if let (Some(renderer), Some(view)) = (renderer(), view()) {
                renderer.print(hwnd, wp as Handle, &view);
            }
            return 0;
        }
        0x14 => return 1,
        0x201 => {
            // Ignore queued clicks after activation hides the panel, and leave IME
            // preedit/candidates owned by Edit just as the Enter path does.
            if IsWindowVisible(hwnd) == 0 || is_composing() {
                return 0;
            }
            let query_changed = sync_query();
            if let Some(renderer) = renderer() {
                let x = lp as u16 as i16 as i32;
                let y = ((lp as u32 >> 16) as u16) as i16 as i32;
                let clicked = state(|s| {
                    renderer
                        .row_at(x, y, s.controller.results().len())
                        .map(|index| {
                            let previous = s.controller.selected_index();
                            if previous != Some(index) {
                                let old = previous.unwrap_or(0);
                                s.controller.select_relative(index as isize - old as isize);
                            }
                            (previous, Some(index))
                        })
                })
                .flatten();
                if let Some((previous, selected)) = clicked {
                    // A coalesced text change can replace the row under the cursor.
                    // Let that click select the new result; require another click to open it.
                    if previous == selected && !query_changed {
                        activate();
                    } else {
                        renderer.invalidate_selection(hwnd, previous, selected);
                        SetFocus(EDIT.get());
                    }
                }
            }
            return 0;
        }
        0x10 => {
            close();
            return 0;
        }
        0x2 => {
            let icons = state(|s| (s.icons.take(), std::mem::take(&mut s.view.icons)));
            drop(icons);
            let _ = input_language::restore();
            let tray = state(|s| s.tray.take()).flatten();
            drop(tray); // Native icon deletion runs outside the Runtime borrow.
            if HOTKEY_SET.replace(false) {
                UnregisterHotKey(hwnd, 1);
            }
            PostQuitMessage(0);
            return 0;
        }
        0x82 => {
            EDIT.set(null_mut());
            WINDOW.set(null_mut());
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
struct OwnedHandle(Handle);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Com;
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
pub(super) fn enable_dpi() {
    unsafe {
        let module = GetModuleHandleW(wide("user32.dll").as_ptr());
        let address = GetProcAddress(module, c"SetProcessDpiAwarenessContext".as_ptr().cast());
        if !address.is_null() {
            let set: unsafe extern "system" fn(isize) -> i32 = std::mem::transmute(address);
            if set(-4) != 0 {
                return;
            }
        }
        SetProcessDPIAware();
    }
}
fn dpi(hwnd: Hwnd) -> u32 {
    unsafe {
        let address = GetProcAddress(
            GetModuleHandleW(wide("user32.dll").as_ptr()),
            c"GetDpiForWindow".as_ptr().cast(),
        );
        if address.is_null() {
            96
        } else {
            let get: unsafe extern "system" fn(Hwnd) -> u32 = std::mem::transmute(address);
            get(hwnd).max(96)
        }
    }
}
pub fn run(options: Options) -> io::Result<()> {
    super::prepare_language(options.data_dir.as_deref())?;
    let hotkey = Hotkey::parse(&options.hotkey)?;
    unsafe {
        let handle = CreateMutexW(null(), 0, wide("Local\\PicoRun.Native.v1").as_ptr());
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let existed = GetLastError() == 183;
        let _mutex = OwnedHandle(handle);
        if existed {
            let existing = FindWindowW(wide(CLASS).as_ptr(), null());
            if !existing.is_null() {
                PostMessageW(existing, SHOW, 0, 0);
            }
            return Ok(());
        }
        if CoInitializeEx(null_mut(), 2) < 0 {
            return Err(io::Error::other(Text::ShellCom));
        }
        let _com = Com;
        enable_dpi();
        let startup = Rc::new(startup::Registration::new(&options)?);
        let data = options
            .data_dir
            .map(Ok)
            .unwrap_or_else(discovery::data_directory)?;
        let language_settings = data.join("language.txt");
        let path = data.join("apps-v1.bin");
        let settings_path = data.join("theme.txt");
        let input_settings = data.join("english-input.txt");
        let start_english = settings::load_english(&input_settings);
        let icon_settings = data.join("icons.txt");
        let show_icons = options
            .icons
            .unwrap_or_else(|| settings::load_toggle(&icon_settings));
        let mode = options
            .theme
            .unwrap_or_else(|| settings::load(&settings_path));
        let system_sources = options.sources.is_empty();
        let (roots, unavailable) = if system_sources {
            discovery::roots()
        } else {
            (options.sources, 0)
        };
        let mut cached = cache::load(&path).ok();
        let scan = discovery::discover(&roots, cached.as_ref());
        let (catalog, failed, source) = match scan {
            Ok(scan) => (
                Catalog::new(scan.entries),
                scan.failed.len() + unavailable,
                None,
            ),
            Err(error) => (cached.take().unwrap_or_default(), 0, Some(error)),
        };
        drop(cached); // Release the startup fallback after successful discovery.
        let status = Notice::Catalog {
            count: catalog.entries().len(),
            failed,
            refreshed: false,
            cache_failed: cache::save(&path, &catalog).is_err(),
            source,
        };
        let initial_view = View {
            show_icons,
            ..View::default()
        };
        let initial_renderer = Rc::new(Renderer::new(mode.theme(), 96)?);
        STATE.with(|s| {
            *s.borrow_mut() = Some(Runtime {
                controller: Controller::new(catalog),
                renderer: initial_renderer,
                view: initial_view,
                view_dirty: true,
                theme_mode: mode,
                settings: settings_path,
                start_english,
                input_settings,
                tray: None,
                startup,
                roots,
                cache: path,
                status,
                status_dirty: true,
                language_settings,
                text_buffer: Vec::with_capacity(128),
                text: String::with_capacity(128),
                refreshing: false,
                system_sources,
                icons: None,
                icon_settings,
                measure_icons: options.measure_icons,
                hold_measurement_window: options.hold_measurement_window,
            })
        });
        let class_name = wide(CLASS);
        let instance = GetModuleHandleW(null());
        let class = WndClass {
            size: std::mem::size_of::<WndClass>() as u32,
            style: 0,
            proc: Some(window_proc),
            class_extra: 0,
            window_extra: 0,
            instance,
            icon: null_mut(),
            cursor: LoadCursorW(null_mut(), 32512usize as *const u16),
            background: null_mut(),
            menu: null(),
            name: class_name.as_ptr(),
            small_icon: null_mut(),
        };
        if RegisterClassExW(&class) == 0 {
            STATE.with(|s| s.borrow_mut().take());
            return Err(io::Error::last_os_error());
        }
        let hwnd = CreateWindowExW(
            0x80 | 0x8,
            class_name.as_ptr(),
            wide("PicoRun").as_ptr(),
            0x80000000 | 0x02000000,
            100,
            100,
            560,
            500,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        if hwnd.is_null() {
            STATE.with(|s| s.borrow_mut().take());
            return Err(io::Error::last_os_error());
        }
        let result = (|| {
            let taskbar_created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
            if taskbar_created == 0 {
                return Err(io::Error::last_os_error());
            }
            TASKBAR_CREATED.set(taskbar_created);
            let actual_dpi = dpi(hwnd);
            if actual_dpi != 96 {
                let new = Rc::new(Renderer::new(mode.theme(), actual_dpi)?);
                SendMessageW(EDIT.get(), 0x30, new.font as usize, 1);
                state(|s| s.renderer = new);
            }
            if RegisterHotKey(hwnd, 1, hotkey.modifiers, hotkey.key) == 0 {
                return Err(io::Error::other(i18n::Failure::Hotkey {
                    hotkey: options.hotkey.clone(),
                    code: GetLastError(),
                }));
            }
            HOTKEY_SET.set(true);
            let tray = Rc::new(tray::Tray::new(hwnd, &options.hotkey)?);
            state(|s| s.tray = Some(tray));
            if !options.hidden {
                show();
            }
            let mut message = Message::default();
            loop {
                match GetMessageW(&mut message, null_mut(), 0, 0) {
                    -1 => return Err(io::Error::last_os_error()),
                    0 => break,
                    _ => {
                        before_translate(&message);
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            }
            Ok(())
        })();
        if !WINDOW.get().is_null() {
            DestroyWindow(hwnd);
        }
        let runtime = STATE.with(|s| s.borrow_mut().take());
        drop(runtime); // Drop native resources outside the borrow, after destroying Edit/window.
        result
    }
}
