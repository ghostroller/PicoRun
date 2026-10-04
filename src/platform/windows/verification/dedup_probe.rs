//! Real Shell Link fixtures and two-release, complete-process comparison. No user app launches.
use super::*;
use crate::{catalog::Catalog, model::LaunchTarget};

fn expect(checks: &mut String, condition: bool, description: &str) -> io::Result<()> {
    super::expect(condition, description, checks)
}
fn same_entries(a: &[crate::model::AppEntry], b: &[crate::model::AppEntry]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.name == b.name && a.target == b.target && a.keys == b.keys)
}

fn selected(scan: &discovery::Scan, name: &str) -> Option<PathBuf> {
    scan.entries.iter().find_map(|e| {
        if e.name.eq_ignore_ascii_case(name) {
            if let LaunchTarget::ShellPath(path) = &e.target {
                return Some(path.clone());
            }
        }
        None
    })
}
fn patch(path: &Path, offset: usize, value: u32) -> io::Result<()> {
    let mut bytes = fs::read(path)?;
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    fs::write(path, bytes)
}
fn check(root: &Path, target: &Path) -> io::Result<()> {
    let user = root.join("user");
    let common = root.join("common");
    let desktop = root.join("desktop");
    let work = root.join("工作 目录");
    let other_work = root.join("其他 目录");
    for dir in [&user, &common, &desktop, &work, &other_work] {
        fs::create_dir_all(dir)?;
    }
    let marker = root.join("launch.txt");
    let args = format!(
        "--controlled-child \"{}\" \"参数 with spaces\"",
        marker.display()
    );
    let primary = user.join("微信 App.lnk");
    create_shortcut(&primary, target, &args, &work)?;
    let secondary = common.join("微信 App.lnk");
    fs::copy(&primary, &secondary)?;
    fs::copy(&primary, desktop.join("微信 App.lnk"))?;
    // Create a second equivalent link through COM rather than only copying the bytes.
    let alternate = desktop.join("second");
    fs::create_dir_all(&alternate)?;
    create_shortcut(&alternate.join("微信 App.lnk"), target, &args, &work)?;
    let case_variant = desktop.join("case");
    fs::create_dir_all(&case_variant)?;
    fs::copy(&primary, case_variant.join("微信 app.lnk"))?;
    fs::copy(&primary, desktop.join("别名 App.lnk"))?;
    let roots = [user.clone(), common.clone(), desktop.clone()];
    let mut checks = String::new();
    let first = discovery::discover(&roots, None)?;
    expect(
        &mut checks,
        first.entries.len() == 2,
        "same-name equal links merge, different names survive",
    )?;
    expect(
        &mut checks,
        selected(&first, "微信 App").as_ref() == Some(&primary),
        "user source wins over common and desktop",
    )?;
    let overlap = discovery::discover(
        &[user.clone(), user.clone(), common.clone(), desktop.clone()],
        None,
    )?;
    expect(
        &mut checks,
        same_entries(&overlap.entries, &first.entries),
        "overlapping sources do not alter entries",
    )?;
    let cache_path = root.join("snapshot.bin");
    cache::save(&cache_path, &Catalog::new(first.entries.clone()))?;
    expect(
        &mut checks,
        same_entries(cache::load(&cache_path)?.entries(), &first.entries),
        "deduplicated original paths and pinyin keys survive cache",
    )?;
    let mut engine = crate::search::SearchEngine::default();
    let mut hits = Vec::new();
    for query in ["微信", "weixin", "wx", "weixin app"] {
        engine.search(&first.entries, query, &mut hits);
        expect(
            &mut checks,
            hits.len() == 1,
            &format!("one deduplicated Chinese/full/initial/mixed result: {query}"),
        )?;
    }
    let winner = first.entries.iter().find(|e| e.name == "微信 App").unwrap();
    let _ = fs::remove_file(&marker);
    discovery::launch(null_mut(), &winner.target)?;
    let launched = wait_marker(&marker)?;
    expect(
        &mut checks,
        launched
            .lines()
            .find_map(|line| line.strip_prefix("cwd="))
            .is_some_and(|cwd| Path::new(cwd) == work)
            && launched.contains("args=参数 with spaces\n"),
        "Shell launches original winning .lnk with quoted Unicode args and working directory",
    )?;
    fs::remove_file(&primary)?;
    let fallback = discovery::discover(&roots, None)?;
    expect(
        &mut checks,
        fallback.entries.len() == 2 && selected(&fallback, "微信 App").as_ref() == Some(&secondary),
        "refresh promotes available common shortcut after preferred entry deletion",
    )?;

    // A separate group checks semantic differences, all bearing the identical display name.
    let variants = root.join("variants");
    fs::create_dir_all(&variants)?;
    for (index, arguments, directory) in [
        (0, "--Profile A", &work),
        (1, "--profile A", &work),
        (2, "--Profile  A", &work),
        (3, "--Profile A", &other_work),
        (4, "--Profile A", &PathBuf::new()),
    ] {
        let dir = variants.join(index.to_string());
        fs::create_dir_all(&dir)?;
        create_shortcut(&dir.join("同名.lnk"), target, arguments, directory)?;
    }
    let base = variants.join("0/同名.lnk");
    for (index, offset, value) in [(5, 60, 3), (6, 60, 7), (7, 20, 0x2000)] {
        let dir = variants.join(index.to_string());
        fs::create_dir_all(&dir)?;
        let path = dir.join("同名.lnk");
        fs::copy(&base, &path)?;
        let value = if offset == 20 {
            u32::from_le_bytes(fs::read(&path)?[20..24].try_into().unwrap()) | value
        } else {
            value
        };
        patch(&path, offset, value)?;
    }
    for (index, signature, size) in [(8, 0xa0000001u32, 0xccusize), (9, 0xa0001234, 8)] {
        let dir = variants.join(index.to_string());
        fs::create_dir_all(&dir)?;
        let mut bytes = fs::read(&base)?;
        bytes.truncate(bytes.len() - 4);
        bytes.extend_from_slice(&(size as u32).to_le_bytes());
        bytes.extend_from_slice(&signature.to_le_bytes());
        bytes.resize(bytes.len() + size - 8, 0);
        bytes.extend_from_slice(&[0; 4]);
        fs::write(dir.join("同名.lnk"), bytes)?;
    }
    let variant_scan = discovery::discover(std::slice::from_ref(&variants), None)?;
    expect(&mut checks, variant_scan.entries.len() == 10, "argument case/spaces, different/empty cwd, show mode, run-as, console and unknown blocks remain separate")?;
    let repeat = discovery::discover(std::slice::from_ref(&variants), None)?;
    expect(
        &mut checks,
        same_entries(&variant_scan.entries, &repeat.entries),
        "equal-name variants have deterministic result order",
    )?;
    let duplicates = variants.join("zzz");
    fs::create_dir_all(&duplicates)?;
    fs::copy(&base, duplicates.join("同名.lnk"))?;
    let tied = discovery::discover(std::slice::from_ref(&variants), None)?;
    expect(
        &mut checks,
        tied.entries.len() == 10 && selected(&tied, "同名").as_ref() == Some(&base),
        "same-source tie selects stable lexical path",
    )?;
    let bad_root = root.join("unreadable");
    let old = Catalog::new(vec![crate::model::AppEntry::new(
        "同名",
        LaunchTarget::ShellPath(bad_root.join("同名.lnk")),
    )]);
    let partial = discovery::discover(&[variants.clone(), bad_root], Some(&old))?;
    expect(
        &mut checks,
        partial.entries.len() == 11 && partial.failed.len() == 1,
        "unreadable recovered shortcut remains conservative because current metadata is unknown",
    )?;
    let exe_root = root.join("bare");
    fs::create_dir_all(&exe_root)?;
    fs::copy(target, exe_root.join("同名.exe"))?;
    create_shortcut(&exe_root.join("同名.lnk"), target, "", &work)?;
    expect(
        &mut checks,
        discovery::discover(&[exe_root], None)?.entries.len() == 2,
        "bare exe does not swallow a shortcut",
    )?;
    fs::write(root.join("checks.txt"), &checks)?;
    print!("{checks}");
    Ok(())
}

fn options(data: &Path, sources: &[PathBuf]) -> Vec<std::ffi::OsString> {
    let mut args = vec![
        "--hidden".into(),
        "--hotkey".into(),
        "Ctrl+Alt+F11".into(),
        "--icons".into(),
        "off".into(),
        "--measure-icons".into(),
        "--hold-measurement-window".into(),
        "--data-dir".into(),
        data.into(),
    ];
    for source in sources {
        args.push("--source".into());
        args.push(source.into());
    }
    args
}
fn milliseconds(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
fn benchmark(root: &Path, target: &Path) -> io::Result<()> {
    let current = std::env::current_exe()?.with_file_name("picorun.exe");
    let baseline = root.join("baseline/picorun.exe");
    if !baseline.is_file() {
        return Err(io::Error::other(
            "freeze the pre-dedup release in runtime/dedup/baseline first",
        ));
    }
    let mut times = String::from("version,entries,duplicate_percent,round,catalog_entries,startup_ms,refresh_ms,query_p50_ms,query_p95_ms\n");
    let mut memory = String::from("version,entries,duplicate_percent,round,stage,private,ws,peak_commit,peak_ws,gdi,user,cpu_100ns\n");
    for count in [500, 2000, 10000] {
        for percent in [0, 50] {
            let source = root.join(format!("fixture-{count}-{percent}"));
            fs::create_dir_all(&source)?;
            let a = source.join("a");
            let b = source.join("b");
            fs::create_dir_all(&a)?;
            fs::create_dir_all(&b)?;
            let unique = count * (100 - percent) / 100;
            for index in 0..unique {
                let name = format!("微信 App {index:05}.lnk");
                let path = a.join(&name);
                create_shortcut(&path, target, "--controlled-child unused.txt fixture", root)?;
                if percent == 50 {
                    fs::copy(&path, b.join(&name))?;
                }
            }
            for round in 0..5 {
                // Alternate version order to reduce warm-cache/time-of-run bias.
                let versions = if round % 2 == 0 {
                    [("before", &baseline), ("after", &current)]
                } else {
                    [("after", &current), ("before", &baseline)]
                };
                for (version, exe) in versions {
                    let data = root.join(format!("data-{count}-{percent}-{round}-{version}"));
                    fs::create_dir_all(&data)?;
                    let _ = fs::remove_file(data.join("apps-v1.bin"));
                    let start = Instant::now();
                    let mut child = spawn(exe, &options(&data, std::slice::from_ref(&source)))?;
                    let (hwnd, edit) = wait_window(&mut child)?;
                    // Window creation precedes tray initialization. A synchronous counter waits
                    // for the message loop and measures a fully initialized hidden application.
                    let catalog_entries = unsafe { SendMessageW(hwnd, 0x8008, 0, 0) };
                    let startup_ms = milliseconds(start);
                    let expected = if version == "after" { unique } else { count };
                    if catalog_entries != expected as isize {
                        return Err(io::Error::other(format!("benchmark entry mismatch: {version} {count}/{percent}: {catalog_entries}")));
                    }
                    let prefix = format!("{version},{count},{percent},{round}");
                    sample(child.0.id(), &format!("{prefix},hidden"), &mut memory)?;
                    unsafe {
                        SendMessageW(hwnd, 0x8001, 0, 0);
                    }
                    let queries = ["weixin", "wx app", "app 000", "不存在的应用"];
                    for _ in 0..5 {
                        for q in queries {
                            unsafe {
                                set_control_text(edit, wide(q).as_ptr());
                                SendMessageW(hwnd, 0x8003, 0, 0);
                            }
                        }
                    }
                    let mut response = Vec::new();
                    for _ in 0..25 {
                        for q in queries {
                            let start = Instant::now();
                            unsafe {
                                set_control_text(edit, wide(q).as_ptr());
                                SendMessageW(hwnd, 0x8003, 0, 0);
                                SendMessageW(hwnd, 0xf, 0, 0);
                            }
                            response.push(milliseconds(start));
                        }
                    }
                    response.sort_by(f64::total_cmp);
                    sample(
                        child.0.id(),
                        &format!("{prefix},visible_queries"),
                        &mut memory,
                    )?;
                    let start = Instant::now();
                    unsafe {
                        SendMessageW(hwnd, 0x8002, 0, 0);
                    }
                    let refresh_ms = milliseconds(start);
                    sample(child.0.id(), &format!("{prefix},refreshed"), &mut memory)?;
                    unsafe {
                        SendMessageW(hwnd, 0x800a, 0, 0);
                    }
                    let cpu = sample(child.0.id(), &format!("{prefix},idle_start"), &mut memory)?;
                    thread::sleep(Duration::from_millis(500));
                    let end_cpu = sample(child.0.id(), &format!("{prefix},idle_end"), &mut memory)?;
                    if end_cpu != cpu {
                        eprintln!("idle CPU observed {prefix}: {} ticks", end_cpu - cpu);
                    }
                    times.push_str(&format!(
                        "{prefix},{catalog_entries},{startup_ms:.4},{refresh_ms:.4},{:.4},{:.4}\n",
                        response[49], response[94]
                    ));
                    unsafe {
                        SendMessageW(hwnd, 0x10, 0, 0);
                    }
                    child.0.wait()?;
                    // Preserve partial results if a later environment failure interrupts the batch.
                    fs::write(root.join("times.csv"), &times)?;
                    fs::write(root.join("memory.csv"), &memory)?;
                }
            }
            println!("completed {count} entries, {percent}% duplicates, 5 rounds per version");
        }
    }
    // Only aggregate counts leave ignored storage; the private snapshot keeps original paths here.
    let (roots, failed_roots) = discovery::roots();
    let data = root.join("real-before");
    fs::create_dir_all(&data)?;
    let mut child = spawn(&baseline, &options(&data, &[]))?;
    let (hwnd, _) = wait_window(&mut child)?;
    let before = unsafe { SendMessageW(hwnd, 0x8008, 0, 0) };
    unsafe {
        SendMessageW(hwnd, 0x10, 0, 0);
    }
    child.0.wait()?;
    let start = Instant::now();
    let after = discovery::discover(&roots, None)?;
    let elapsed = milliseconds(start);
    fs::write(root.join("real-counts.txt"),format!("before={before}\nafter={}\nremoved={}\nfailed_directories={}\nunavailable_roots={failed_roots}\nscan_ms={elapsed:.4}\n",after.entries.len(),before-after.entries.len() as isize,after.failed.len()))?;
    Ok(())
}

pub fn run(bench: bool) -> io::Result<()> {
    super::super::window::enable_dpi();
    let root = std::env::current_dir()?.join("runtime/dedup");
    fs::create_dir_all(&root)?;
    let check_root = root.join(format!("checks-{}", std::process::id()));
    fs::create_dir_all(&check_root)?;
    let target = root.join("controlled child.exe");
    fs::copy(std::env::current_exe()?, &target)?;
    if unsafe { CoInitializeEx(null_mut(), 2) } < 0 {
        return Err(io::Error::other("dedup probe COM init failed"));
    }
    let result = (|| {
        check(&check_root, &target)?;
        if bench {
            benchmark(&root, &target)?;
        }
        Ok(())
    })();
    unsafe {
        CoUninitialize();
    }
    result
}
