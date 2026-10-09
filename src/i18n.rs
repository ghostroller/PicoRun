//! Two built-in UI languages. Translation happens when a notice changes, never during search.
use std::{cell::Cell, fmt, io};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    Chinese,
    English,
}
impl Language {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "zh-CN" => Some(Self::Chinese),
            "en" => Some(Self::English),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Chinese => "zh-CN",
            Self::English => "en",
        }
    }
}
thread_local! { static LANGUAGE: Cell<Language> = const { Cell::new(Language::Chinese) }; }
pub fn current() -> Language {
    LANGUAGE.get()
}
pub fn set(language: Language) {
    LANGUAGE.set(language);
}

// Keeping both translations in each entry makes missing translations a compile-time error.
macro_rules! texts {
    ($($key:ident => ($zh:literal, $en:literal),)*) => {
        #[derive(Clone, Copy, Debug)]
        pub enum Text { $($key,)* }
        impl Text {
            pub fn get(self, language: Language) -> &'static str {
                match (self, language) {
                    $((Self::$key, Language::Chinese) => $zh,
                      (Self::$key, Language::English) => $en,)*
                }
            }
        }
    }
}
texts! {
    Help => ("↑↓ 选择   Enter 打开   Esc 隐藏   F5 刷新   Ctrl+Q 退出", "↑↓ Select   Enter Open   Esc Hide   F5 Refresh   Ctrl+Q Quit"),
    Empty => ("没有匹配的应用", "No matching apps"),
    Cue => ("搜索应用 / 拼音 / 首字母", "Search apps / pinyin / initials"),
    Open => ("打开 PicoRun(&O)", "&Open PicoRun"),
    Refresh => ("刷新应用索引(&R)", "&Refresh app index"),
    Light => ("亮色主题(&L)", "&Light theme"),
    Dark => ("暗色主题(&D)", "&Dark theme"),
    EnglishInput => ("呼出时使用英文输入(&E)", "&English input when opened"),
    Icons => ("显示应用图标(&I)", "Show app &icons"),
    Startup => ("登录时启动 PicoRun(&S)", "&Start PicoRun at sign-in"),
    Quit => ("退出 PicoRun(&Q)", "&Quit PicoRun"),
    Refreshing => ("正在刷新应用索引…", "Refreshing app index…"),
    IconLoad => ("图标加载失败", "Could not load icons"),
    IconSave => ("应用图标选项保存失败；重启后可能恢复原设置", "Could not save icon preference; it may reset after restart"),
    InputBackup => ("输入模式备份失败", "Could not save the previous input mode"),
    InputEnglish => ("英文输入切换失败", "Could not switch to English input"),
    InputRestore => ("输入模式恢复失败", "Could not restore the previous input mode"),
    InputChange => ("输入模式切换失败", "Could not change input mode"),
    InputSave => ("英文输入选项保存失败；重启后可能恢复原设置", "Could not save English input preference; it may reset after restart"),
    ThemeSave => ("主题保存失败；重启后可能恢复原主题", "Could not save theme; it may reset after restart"),
    ThemeChangedSave => ("主题已切换，但保存失败；重启后可能恢复原主题", "Theme changed but could not be saved; it may reset after restart"),
    ThemeChange => ("主题切换失败", "Could not change theme"),
    LanguageSave => ("语言已切换，但保存失败；重启后可能恢复原语言", "Language changed but could not be saved; it may reset after restart"),
    StartupOn => ("已启用登录自启动（隐藏到托盘）", "Sign-in startup enabled (hidden in tray)"),
    StartupOff => ("已关闭登录自启动", "Sign-in startup disabled"),
    StartupChange => ("自启动设置失败", "Could not change sign-in startup"),
    StartupRead => ("读取自启动状态失败", "Could not read sign-in startup"),
    ShellCom => ("无法初始化 Shell COM", "Could not initialize Shell COM"),
    InstanceStarting => ("已有 PicoRun 实例尚未完成启动，请稍后重试", "Another PicoRun instance is still starting; try again shortly"),
    InstanceExited => ("已有 PicoRun 实例在启动期间退出，请重新打开", "The existing PicoRun instance exited during startup; open PicoRun again"),
    ForegroundDenied => ("窗口已显示，请点击输入框继续输入", "Window shown; click the input field to continue typing"),
    TrayAdd => ("无法添加 PicoRun 托盘图标", "Could not add the PicoRun tray icon"),
    TrayEvents => ("无法初始化托盘图标事件", "Could not initialize tray icon events"),
    HotkeyFunction => ("热键功能键应为 F1–F24", "Hotkey function key must be F1–F24"),
    HotkeyFormat => ("热键格式例如 Alt+Space 或 Ctrl+Alt+P", "Use a hotkey such as Alt+Space or Ctrl+Alt+P"),
    HotkeyMissing => ("热键需要修饰键和一个按键", "Hotkey needs a modifier and one key"),
    LayoutRead => ("无法读取可用的键盘布局", "Could not read available keyboard layouts"),
    LayoutEnglish => ("未找到可用的英文键盘布局", "No English keyboard layout is available"),
    EditContext => ("原生输入框的输入法上下文已失效", "The native input field's IME context is no longer valid"),
    ConversionRestore => ("输入法转换模式恢复失败", "Could not restore IME conversion mode"),
    ImeRestore => ("输入法中英文模式恢复失败", "Could not restore the IME input mode"),
    OriginalLayout => ("原窗口的输入布局恢复请求失败", "Could not request the original window's input layout"),
    OriginalContext => ("原窗口的输入法上下文已失效", "The original window's IME context is no longer valid"),
    OriginalConversion => ("原窗口的输入法转换模式恢复请求失败", "Could not request the original window's IME conversion mode"),
    OriginalIme => ("原窗口的输入法中英文模式恢复请求失败", "Could not request the original window's IME input mode"),
    KnownFolders => ("无法读取系统已知目录", "Could not read Windows known folders"),
    LinkCreate => ("无法创建 Shell Link", "Could not create Shell Link"),
    LinkRead => ("无法读取 Shell Link", "Could not read Shell Link"),
    RootsMissing => ("无法确定应用目录；保留原有索引，可按 F5 重试", "Could not find app folders; previous index retained. Press F5 to retry"),
    RootsUnreadable => ("所有应用目录均无法读取；保留原有索引", "All app folders are unreadable; previous index retained"),
    EntryMissing => ("入口已失效，请按 F5 刷新索引", "This entry is no longer available. Press F5 to refresh"),
    StartupInvalid => ("自启动注册内容无效", "Invalid sign-in startup registry data"),
    StartupExtra => ("自启动注册内容包含额外数据", "Sign-in startup registry data contains extra data"),
    StartupLong => ("自启动命令超过 Windows Run 项的 260 字符上限；请缩短程序或数据目录路径", "Sign-in command exceeds the Windows Run limit of 260 characters; shorten the program or data path"),
    StartupNul => ("自启动参数包含 NUL", "Sign-in startup argument contains NUL"),
    ArgProbe => ("--startup-probe 缺少验证标识", "--startup-probe requires a verification token"),
    ArgIcons => ("--icons 需要 on 或 off", "--icons requires on or off"),
    ArgTheme => ("--theme 需要 light 或 dark", "--theme requires light or dark"),
    ArgHotkey => ("--hotkey 缺少热键", "--hotkey requires a hotkey"),
    ArgData => ("--data-dir 缺少目录", "--data-dir requires a folder"),
    ArgSource => ("--source 缺少目录", "--source requires a folder"),
    ArgUnknown => ("未知选项；使用 --help 查看说明", "Unknown option; use --help for usage"),
    Usage => ("直接运行打开窗口。Alt+Space 呼出/隐藏，↑↓ 选择，Enter 打开，Esc 隐藏，F5 刷新，Ctrl+Q 退出。托盘右键可刷新、切换主题、界面语言、英文输入、应用图标、登录自启动和退出。\n选项：--hidden、--theme light|dark、--icons on|off（本次启动覆盖）、--hotkey Ctrl+Alt+P、--data-dir <数据目录>、--source <应用入口目录>（可重复；替代系统目录）。", "Run PicoRun to open the window. Alt+Space shows/hides it; arrows select, Enter opens, Esc hides, F5 refreshes, Ctrl+Q quits. Right-click the tray icon to refresh, change theme or UI language, toggle English input, app icons or sign-in startup, and quit.\nOptions: --hidden, --theme light|dark, --icons on|off (this launch only), --hotkey Ctrl+Alt+P, --data-dir <data folder>, --source <app entry folder> (repeatable; replaces system folders)."),
}
impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.get(current()))
    }
}
impl std::error::Error for Text {}

#[derive(Debug)]
pub enum Failure {
    Launch(u32),
    Packaged(i32),
    PackagedDiscovery(i32),
    Hotkey { hotkey: String, code: u32 },
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self, current()) {
            (Self::Launch(code), Language::Chinese) => write!(f, "打开失败（Windows 错误 {code}），可按 F5 刷新"),
            (Self::Launch(code), Language::English) => write!(f, "Could not open app (Windows error {code}); press F5 to refresh"),
            (Self::Packaged(code), Language::Chinese) => write!(f, "打开商店应用失败（HRESULT 0x{code:08X}），可按 F5 刷新"),
            (Self::Packaged(code), Language::English) => write!(f, "Could not open packaged app (HRESULT 0x{code:08X}); press F5 to refresh"),
            (Self::PackagedDiscovery(code), Language::Chinese) => write!(f, "读取商店应用失败（HRESULT 0x{code:08X}）"),
            (Self::PackagedDiscovery(code), Language::English) => write!(f, "Could not read packaged apps (HRESULT 0x{code:08X})"),
            (Self::Hotkey { hotkey, code }, Language::Chinese) => write!(f, "热键 {hotkey} 注册失败（可能已被占用）。请用 --hotkey Ctrl+Alt+P 等组合重启。Windows 错误 {code}"),
            (Self::Hotkey { hotkey, code }, Language::English) => write!(f, "Could not register hotkey {hotkey} (it may be in use). Restart with --hotkey Ctrl+Alt+P or another combination. Windows error {code}"),
        }
    }
}
impl std::error::Error for Failure {}

// Preserve the error value instead of a translated snapshot, so existing notices also switch.
#[derive(Debug)]
pub enum Notice {
    Text(Text),
    Error {
        prefix: Option<Text>,
        source: io::Error,
    },
    Catalog {
        count: usize,
        failed: usize,
        refreshed: bool,
        cache_failed: bool,
        source: Option<io::Error>,
    },
}
impl From<Text> for Notice {
    fn from(text: Text) -> Self {
        Self::Text(text)
    }
}
impl From<io::Error> for Notice {
    fn from(source: io::Error) -> Self {
        Self::Error {
            prefix: None,
            source,
        }
    }
}
impl Notice {
    pub fn error(prefix: Text, source: io::Error) -> Self {
        Self::Error {
            prefix: Some(prefix),
            source,
        }
    }
}
impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => text.fmt(f),
            Self::Error { prefix, source } => {
                if let Some(prefix) = prefix {
                    write!(f, "{prefix}: ")?;
                }
                source.fmt(f)
            }
            Self::Catalog {
                count,
                failed,
                refreshed,
                cache_failed,
                source,
            } => {
                match current() {
                    Language::Chinese => write!(f, "{count} 个应用")?,
                    Language::English => {
                        write!(f, "{count} {}", if *count == 1 { "app" } else { "apps" })?
                    }
                }
                if let Some(source) = source {
                    write!(f, " · {source}")?;
                } else {
                    if *refreshed {
                        f.write_str(match current() {
                            Language::Chinese => " · 刷新完成",
                            Language::English => " · Refreshed",
                        })?;
                    }
                    match current() {
                        Language::Chinese => write!(f, " · {failed} 个来源读取失败")?,
                        Language::English => write!(
                            f,
                            " · {failed} unreadable {}",
                            if *failed == 1 { "source" } else { "sources" }
                        )?,
                    }
                }
                if *cache_failed {
                    f.write_str(match current() {
                        Language::Chinese => " · 缓存保存失败，本次索引仍可用",
                        Language::English => " · Cache not saved; current index is usable",
                    })?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_notices_and_nested_errors_follow_language_changes() {
        let notice = Notice::error(Text::InputEnglish, io::Error::other(Text::LayoutEnglish));
        set(Language::Chinese);
        assert!(notice.to_string().contains("英文键盘布局"));
        set(Language::English);
        assert_eq!(
            notice.to_string(),
            "Could not switch to English input: No English keyboard layout is available"
        );
        let status = Notice::Catalog {
            count: 1,
            failed: 1,
            refreshed: true,
            cache_failed: false,
            source: None,
        };
        assert_eq!(
            status.to_string(),
            "1 app · Refreshed · 1 unreadable source"
        );
        set(Language::Chinese);
        assert_eq!(status.to_string(), "1 个应用 · 刷新完成 · 1 个来源读取失败");
    }
}
