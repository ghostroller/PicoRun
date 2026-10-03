#![cfg_attr(windows, windows_subsystem = "windows")]
use picorun::{
    catalog::Catalog,
    model::{AppEntry, LaunchTarget},
    ui::controller::Controller,
};
use std::path::PathBuf;

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|s| s == "--demo") {
        #[cfg(windows)]
        picorun::platform::windows::attach_console();
        demo(
            &arguments[1..]
                .iter()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" "),
        );
    } else {
        #[cfg(windows)]
        if let Err(error) = native(arguments) {
            picorun::platform::windows::error_box(&error.to_string());
            std::process::exit(1);
        }
        #[cfg(not(windows))]
        eprintln!("原生启动器需要 Windows；核心演示可用 --demo wx。");
    }
}

#[cfg(windows)]
fn native(arguments: Vec<std::ffi::OsString>) -> std::io::Result<()> {
    use picorun::platform::windows::{run, Options};
    let mut options = Options::default();
    let mut args = arguments.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--hidden") => options.hidden = true,
            Some("--icons") => {
                options.icons = Some(
                    match args.next().and_then(|s| s.into_string().ok()).as_deref() {
                        Some("on") => true,
                        Some("off") => false,
                        _ => return Err(std::io::Error::other("--icons 需要 on 或 off")),
                    },
                );
            }
            Some("--measure-icons") => options.measure_icons = true,
            Some("--hold-measurement-window") => options.hold_measurement_window = true,
            Some("--theme") => {
                options.theme = Some(
                    args.next()
                        .and_then(|s| s.into_string().ok())
                        .and_then(|s| picorun::theme::ThemeMode::parse(&s))
                        .ok_or_else(|| std::io::Error::other("--theme 需要 light 或 dark"))?,
                );
            }
            Some("--hotkey") => {
                options.hotkey = args
                    .next()
                    .and_then(|s| s.into_string().ok())
                    .ok_or_else(|| std::io::Error::other("--hotkey 缺少热键"))?
            }
            Some("--data-dir") => {
                options.data_dir =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        std::io::Error::other("--data-dir 缺少目录")
                    })?))
            }
            Some("--source") => options.sources.push(PathBuf::from(
                args.next()
                    .ok_or_else(|| std::io::Error::other("--source 缺少目录"))?,
            )),
            Some("--help") => {
                picorun::platform::windows::error_box("直接运行打开窗口。Alt+Space 呼出/隐藏，↑↓ 选择，Enter 打开，Esc 隐藏，F5 刷新，Ctrl+Q 退出。托盘右键可刷新、切换主题、英文输入、应用图标和退出。\n选项：--hidden、--theme light|dark、--icons on|off（本次启动覆盖）、--hotkey Ctrl+Alt+P、--data-dir <数据目录>、--source <应用入口目录>（可重复；替代系统目录）。");
                return Ok(());
            }
            _ => return Err(std::io::Error::other("未知选项；使用 --help 查看说明")),
        }
    }
    run(options)
}
fn demo(query: &str) {
    // Synthetic paths: the demo only searches and never starts an application.
    let entries = [
        "微信",
        "记事本",
        "Visual Studio Code",
        "网易云音乐",
        "重庆银行",
    ]
    .into_iter()
    .map(|name| {
        AppEntry::new(
            name,
            LaunchTarget::ShellPath(PathBuf::from(format!("demo/{name}.lnk"))),
        )
    })
    .collect();
    let mut controller = Controller::new(Catalog::new(entries));
    controller.set_query(query);
    for hit in controller.results() {
        println!("{}", controller.catalog().entries()[hit.entry_index].name);
    }
}
