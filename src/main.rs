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
    use picorun::{
        i18n::{self, Text},
        platform::windows::{prepare_language, run, Options},
    };
    prepare_language(None)?;
    let mut help = false;
    let mut options = Options::default();
    let mut args = arguments.into_iter();
    let parsed = (|| -> std::io::Result<()> {
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--hidden") => options.hidden = true,
                Some("--startup-probe") => {
                    options.startup_probe = Some(
                        args.next()
                            .and_then(|arg| arg.into_string().ok())
                            .ok_or_else(|| std::io::Error::other(Text::ArgProbe))?,
                    );
                }
                Some("--icons") => {
                    options.icons = Some(
                        match args.next().and_then(|s| s.into_string().ok()).as_deref() {
                            Some("on") => true,
                            Some("off") => false,
                            _ => return Err(std::io::Error::other(Text::ArgIcons)),
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
                            .ok_or_else(|| std::io::Error::other(Text::ArgTheme))?,
                    );
                }
                Some("--hotkey") => {
                    options.hotkey = args
                        .next()
                        .and_then(|s| s.into_string().ok())
                        .ok_or_else(|| std::io::Error::other(Text::ArgHotkey))?
                }
                Some("--data-dir") => {
                    options.data_dir = Some(PathBuf::from(
                        args.next()
                            .ok_or_else(|| std::io::Error::other(Text::ArgData))?,
                    ))
                }
                Some("--source") => options.sources.push(PathBuf::from(
                    args.next()
                        .ok_or_else(|| std::io::Error::other(Text::ArgSource))?,
                )),
                Some("--help") => help = true,
                _ => return Err(std::io::Error::other(Text::ArgUnknown)),
            }
        }
        Ok(())
    })();
    prepare_language(options.data_dir.as_deref())?;
    parsed?;
    if help {
        picorun::platform::windows::error_box(Text::Usage.get(i18n::current()));
        return Ok(());
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
