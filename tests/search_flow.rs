use picorun::{
    catalog::{AppSource, Catalog},
    model::{AppEntry, LaunchTarget},
    search::{SearchEngine, SearchHit, MAX_RESULTS},
    ui::controller::Controller,
};
use std::{io, path::PathBuf};

fn entry(name: &str) -> AppEntry {
    AppEntry::new(
        name,
        LaunchTarget::ShellPath(PathBuf::from(format!("demo/{name}.lnk"))),
    )
}

fn find<'a>(entries: &'a [AppEntry], query: &str) -> Vec<&'a str> {
    let mut results = Vec::<SearchHit>::with_capacity(MAX_RESULTS);
    SearchEngine::default().search(entries, query, &mut results);
    results
        .iter()
        .map(|hit| entries[hit.entry_index].name.as_str())
        .collect()
}

#[test]
fn required_search_inputs() {
    let entries = [
        entry("微信"),
        entry("記事本"),
        entry("网易云音乐"),
        entry("腾讯QQ"),
        entry("Visual Studio Code"),
    ];
    for (query, expected) in [
        ("WEIXIN", "微信"),
        ("wx", "微信"),
        ("微信", "微信"),
        ("jishiben", "記事本"),
        ("wyyyy", "网易云音乐"),
        ("txqq", "腾讯QQ"),
        ("vsc", "Visual Studio Code"),
    ] {
        assert_eq!(find(&entries, query).first().copied(), Some(expected));
    }
    assert!(find(&entries, "zqxvnomatch").is_empty());
}

#[test]
fn priority_stability_and_limit() {
    let entries = [
        entry("xnote"),
        entry("n o t e"),
        entry("Notebook"),
        entry("Note"),
        entry("note"),
    ];
    assert_eq!(
        find(&entries, "note"),
        ["Note", "note", "Notebook", "xnote", "n o t e"]
    );
    let many: Vec<_> = (0..100).map(|i| entry(&format!("App {i}"))).collect();
    assert_eq!(find(&many, "").len(), MAX_RESULTS);
    assert_eq!(find(&many, "")[0], "App 0");
}

#[test]
fn empty_catalog_and_unicode_do_not_panic() {
    assert!(find(&[], "wx").is_empty());
    let entries = [entry("😀工具"), entry("İstanbul")];
    assert_eq!(find(&entries, "😀gj"), ["😀工具"]);
    assert_eq!(find(&entries, "İST"), ["İstanbul"]);
}

#[test]
fn delete_paste_and_immediate_activation_use_current_results() {
    let mut controller = Controller::new(Catalog::new(vec![entry("微信"), entry("记事本")]));
    assert!(controller.set_query("weixin"));
    assert_eq!(
        controller.selected_target(),
        Some(&LaunchTarget::ShellPath(PathBuf::from("demo/微信.lnk")))
    );
    assert!(!controller.set_query("weixin"));
    assert!(controller.set_query(""));
    controller.select_relative(1);
    assert_eq!(controller.selected_index(), Some(1));
    assert!(controller.set_query("jsb"));
    assert_eq!(
        controller.selected_target(),
        Some(&LaunchTarget::ShellPath(PathBuf::from("demo/记事本.lnk")))
    );
    controller.set_query("no-match");
    controller.select_relative(-1);
    assert_eq!(controller.selected_target(), None);
}

#[test]
fn failed_refresh_preserves_previous_catalog() {
    struct FailingSource;
    impl AppSource for FailingSource {
        fn discover(&mut self) -> io::Result<Vec<AppEntry>> {
            Err(io::Error::other("synthetic failure"))
        }
    }
    let mut catalog = Catalog::new(vec![entry("微信")]);
    assert!(catalog.refresh(&mut FailingSource).is_err());
    assert_eq!(catalog.entries()[0].name, "微信");
}
