use picorun::{
    catalog::Catalog,
    model::{AppEntry, LaunchTarget},
    ui::controller::Controller,
};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--demo") {
        println!("PicoRun 基础框架：原生窗口、系统应用发现与启动尚待实现。\n使用 --demo <查询> 验证搜索，例如 --demo wx。");
        return;
    }
    let query = args.collect::<Vec<_>>().join(" ");
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
    controller.set_query(&query);
    for hit in controller.results() {
        println!("{}", controller.catalog().entries()[hit.entry_index].name);
    }
}
