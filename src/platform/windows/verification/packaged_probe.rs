//! Current-user packaged discovery and real Edit/GDI queries. Never opens installed apps.
use super::*;
use crate::{model::LaunchTarget, search::SearchEngine};

fn target_hash(id: &str) -> isize {
    id.encode_utf16().fold(2166136261u32, |h, unit| {
        (h ^ u32::from(unit)).wrapping_mul(16777619)
    }) as isize
}

pub(super) fn run(reference: bool) -> io::Result<()> {
    let root = std::env::current_dir()?.join(if reference {
        "runtime/probe-packaged-reference"
    } else {
        "runtime/probe-packaged"
    });
    let data = root.join("data");
    fs::create_dir_all(&data)?;
    fs::write(data.join("english-input.txt"), "on\n")?;
    fs::write(data.join("theme.txt"), "dark\n")?;
    let exe = if reference {
        std::env::current_dir()?.join("runtime/packaged-baseline/picorun.exe")
    } else {
        std::env::current_exe()?.with_file_name("picorun.exe")
    };
    let args = [
        "--hidden".into(),
        "--hotkey".into(),
        "Ctrl+Alt+Shift+F9".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
        "--icons".into(),
        "off".into(),
        "--measure-icons".into(),
        "--hold-measurement-window".into(),
    ];
    let mut checks = String::new();
    let mut timings = String::new();
    let mut memory = String::from("stage,private_bytes,working_set_bytes,peak_commit_bytes,peak_working_set_bytes,gdi_handles,user_handles,cpu_100ns\n");
    let cache_path = data.join("apps-v1.bin");
    let _ = fs::remove_file(&cache_path);
    for run in 0..3 {
        let start = Instant::now();
        let mut child = spawn(&exe, &args)?;
        let (hwnd, edit) = wait_window(&mut child)?;
        let startup = start.elapsed().as_secs_f64() * 1000.0;
        thread::sleep(Duration::from_millis(100));
        if let Some(status) = child.0.try_wait()? {
            return Err(io::Error::other(format!(
                "launcher exited after creating window: {status}"
            )));
        }
        sample(child.0.id(), &format!("run{run}_hidden"), &mut memory)?;
        let catalog = cache::load(&cache_path)?;
        let packaged_count = catalog
            .entries()
            .iter()
            .filter(|entry| matches!(entry.target, LaunchTarget::AppUserModelId(_)))
            .count();
        timings.push_str(&format!(
            "run={run} cache_cold={} entries={} packaged={} startup_ms={startup:.4}\n",
            run == 0,
            catalog.entries().len(),
            packaged_count
        ));
        let mut engine = SearchEngine::default();
        let mut hits = Vec::new();
        // Names of the two user-reported missing apps; IDs and other app names stay in runtime cache.
        for (label, query) in [("chatgpt", "chatgpt"), ("store", "Microsoft Store")] {
            engine.search(catalog.entries(), query, &mut hits);
            let expected = hits.first().map(|hit| &catalog.entries()[hit.entry_index]);
            if reference {
                checks.push_str(&format!(
                    "run={run} reference_{label}_found={}\n",
                    expected.is_some()
                ));
                continue;
            }
            let id = match expected.map(|entry| &entry.target) {
                Some(LaunchTarget::AppUserModelId(id)) => id,
                _ => {
                    return Err(io::Error::other(format!(
                        "reported app {label} not indexed as packaged"
                    )))
                }
            };
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
                set_control_text(edit, wide(query).as_ptr());
                SendMessageW(hwnd, 0x8003, 0, 0);
                SendMessageW(hwnd, 0xf, 0, 0);
            }
            expect(
                unsafe { SendMessageW(hwnd, 0x800f, 0, 0) } == 2
                    && unsafe { SendMessageW(hwnd, 0x800f, 1, 0) } == target_hash(id),
                &format!("run={run} actual Edit selects indexed {label} packaged target"),
                &mut checks,
            )?;
            if run == 0 {
                screenshot(hwnd, &root.join(format!("{label}.bmp")))?;
            }
        }
        unsafe {
            SendMessageW(hwnd, 0x8001, 0, 0);
        }
        sample(child.0.id(), &format!("run{run}_shown"), &mut memory)?;
        let queries = ["chatgpt", "Microsoft Store", "store", "no-match", ""];
        for _ in 0..20 {
            for query in queries {
                query_and_paint(hwnd, edit, query);
            }
        }
        let mut samples = Vec::with_capacity(100);
        for i in 0..100 {
            let start = Instant::now();
            query_and_paint(hwnd, edit, queries[i % queries.len()]);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        sample(child.0.id(), &format!("run{run}_queried"), &mut memory)?;
        let start = Instant::now();
        unsafe {
            SendMessageW(hwnd, 0x8002, 0, 0);
        }
        let refresh = start.elapsed().as_secs_f64() * 1000.0;
        sample(child.0.id(), &format!("run{run}_refreshed"), &mut memory)?;
        if !reference {
            let refreshed = cache::load(&cache_path)?;
            expect(
                refreshed
                    .entries()
                    .iter()
                    .filter(|entry| matches!(entry.target, LaunchTarget::AppUserModelId(_)))
                    .count()
                    == packaged_count,
                "manual refresh retains packaged source and saves tagged cache",
                &mut checks,
            )?;
            query_and_paint(hwnd, edit, "chatgpt");
            expect(
                unsafe { SendMessageW(hwnd, 0x800f, 0, 0) } == 2,
                "query remains launchable after refresh",
                &mut checks,
            )?;
        }
        timings.push_str(&format!("run={run} query_samples=100 warmup=100 query_p50_ms={:.4} query_p95_ms={:.4} refresh_ms={refresh:.4}\n", samples[49], samples[94]));
        unsafe {
            SendMessageW(hwnd, 0x800a, 0, 0);
        }
        thread::sleep(Duration::from_millis(500));
        let before = sample(child.0.id(), &format!("run{run}_idle_start"), &mut memory)?;
        thread::sleep(Duration::from_secs(2));
        let after = sample(child.0.id(), &format!("run{run}_idle_end"), &mut memory)?;
        checks.push_str(&format!(
            "run={run} idle_interval_ms=2000 cpu_ms={:.3}\n",
            (after - before) as f64 / 10000.0
        ));
        unsafe {
            SendMessageW(hwnd, 0x10, 0, 0);
        }
        let status = child.0.wait()?;
        expect(
            status.success(),
            &format!("run={run} launcher exits normally: {status}"),
            &mut checks,
        )?;
    }
    if !reference {
        unsafe {
            if CoInitializeEx(null_mut(), 2) < 0 {
                return Err(io::Error::other("probe COM init failed"));
            }
        }
        // A nonexistent AUMID exercises actual activation COM/error propagation without opening an app.
        let result = discovery::launch(
            null_mut(),
            &LaunchTarget::AppUserModelId("PicoRun.AbsentProbe_1234567890abc!App".into()),
        );
        unsafe {
            CoUninitialize();
        }
        expect(
            result.is_err(),
            "nonexistent packaged activation returns an error",
            &mut checks,
        )?;
    }
    fs::write(root.join("checks.txt"), checks)?;
    fs::write(root.join("timings.txt"), timings)?;
    fs::write(root.join("memory.csv"), memory)?;
    Ok(())
}

fn query_and_paint(hwnd: Hwnd, edit: Hwnd, query: &str) {
    unsafe {
        set_control_text(edit, wide(query).as_ptr());
        SendMessageW(hwnd, 0x8003, 0, 0);
        SendMessageW(hwnd, 0xf, 0, 0);
    }
}

fn icon_stat(hwnd: Hwnd, key: usize) -> isize {
    unsafe { SendMessageW(hwnd, 0x8006, key, 0) }
}

fn icons_ready(hwnd: Hwnd) -> io::Result<()> {
    let started = Instant::now();
    while icon_stat(hwnd, 0) != 1 {
        if started.elapsed() > Duration::from_secs(15) {
            return Err(io::Error::other("packaged icon completion timed out"));
        }
        thread::sleep(Duration::from_millis(1));
    }
    // Completion is processed by the owning UI thread; synchronously finish its queued paint.
    unsafe {
        SendMessageW(hwnd, 0xf, 0, 0);
    }
    Ok(())
}

fn select_packaged(
    hwnd: Hwnd,
    edit: Hwnd,
    name: &str,
    id: &str,
    catalog: &crate::catalog::Catalog,
    checks: &mut String,
    label: &str,
) -> io::Result<usize> {
    let mut hits = Vec::new();
    SearchEngine::default().search(catalog.entries(), name, &mut hits);
    let selected = hits
        .iter()
        .position(|hit| {
            matches!(&catalog.entries()[hit.entry_index].target, LaunchTarget::AppUserModelId(actual) if actual == id)
        })
        .ok_or_else(|| io::Error::other(format!("{label}: packaged target absent from results")))?;
    query_and_paint(hwnd, edit, name);
    // Select the indexed identity even when another entry shares its localized name.
    // These are navigation messages only; the probe never sends Enter or a result click.
    let actual = unsafe { SendMessageW(hwnd, 0x800c, 6, 0) };
    let delta = selected as isize - actual;
    let direction = if delta < 0 { 0x26 } else { 0x28 };
    for _ in 0..delta.unsigned_abs() {
        unsafe {
            SendMessageW(edit, 0x100, direction, 1);
            SendMessageW(edit, 0x101, direction, 1);
        }
    }
    expect(
        unsafe { SendMessageW(hwnd, 0x800f, 0, 0) } == 2
            && unsafe { SendMessageW(hwnd, 0x800f, 1, 0) } == target_hash(id),
        &format!("{label}: actual Edit selects expected packaged AUMID"),
        checks,
    )?;
    Ok(hits.len())
}

fn icon_bounds(hwnd: Hwnd, checks: &mut String) -> io::Result<()> {
    expect(
        icon_stat(hwnd, 1) <= 48 && icon_stat(hwnd, 10) <= 512 && icon_stat(hwnd, 11) <= 131072,
        "combined path/AUMID icon cache <=48 and metadata <=512/128KiB",
        checks,
    )
}

/// Three independent processes, native Edit/GDI timing and full launcher memory.
/// Personal names/identities remain confined to the ignored runtime cache.
pub(super) fn run_icons(reference: bool) -> io::Result<()> {
    let root = std::env::current_dir()?.join(if reference {
        "runtime/probe-packaged-icons-reference"
    } else {
        "runtime/probe-packaged-icons"
    });
    let data = root.join("data");
    fs::create_dir_all(&data)?;
    fs::write(data.join("english-input.txt"), "on\n")?;
    fs::write(data.join("theme.txt"), "dark\n")?;
    let exe = if reference {
        std::env::current_dir()?.join("runtime/packaged-icons-baseline/picorun.exe")
    } else {
        std::env::current_exe()?.with_file_name("picorun.exe")
    };
    let args = [
        "--hidden".into(),
        "--hotkey".into(),
        "Ctrl+Alt+Shift+F9".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
        "--icons".into(),
        "off".into(),
        "--measure-icons".into(),
        "--hold-measurement-window".into(),
    ];
    let cache_path = data.join("apps-v1.bin");
    let _ = fs::remove_file(&cache_path);
    let mut checks = String::new();
    let mut timings = String::new();
    let mut memory = String::from("stage,private_bytes,working_set_bytes,peak_commit_bytes,peak_working_set_bytes,gdi_handles,user_handles,cpu_100ns\n");
    for run in 0..3 {
        fs::write(data.join("theme.txt"), "dark\n")?;
        let started = Instant::now();
        let mut child = spawn(&exe, &args)?;
        let (hwnd, edit) = wait_window(&mut child)?;
        let startup_ms = started.elapsed().as_secs_f64() * 1000.0;
        sample(child.0.id(), &format!("run{run}_hidden_off"), &mut memory)?;
        expect(
            icon_stat(hwnd, 17) == 0 && icon_stat(hwnd, 18) == 0,
            &format!("run={run}: disabled startup creates no icon worker"),
            &mut checks,
        )?;
        let catalog = cache::load(&cache_path)?;
        let packaged_count = catalog
            .entries()
            .iter()
            .filter(|entry| matches!(entry.target, LaunchTarget::AppUserModelId(_)))
            .count();
        let reported: Vec<_> = ["ChatGPT", "Microsoft Store"]
            .iter()
            .map(|name| {
                catalog
                    .entries()
                    .iter()
                    .find(|entry| {
                        entry.name.eq_ignore_ascii_case(name)
                            && matches!(entry.target, LaunchTarget::AppUserModelId(_))
                    })
                    .ok_or_else(|| {
                        io::Error::other(format!("reported app {name} not indexed as packaged"))
                    })
            })
            .collect::<io::Result<_>>()?;
        let LaunchTarget::AppUserModelId(first_id) = &reported[0].target else {
            unreachable!();
        };
        select_packaged(
            hwnd,
            edit,
            &reported[0].name,
            first_id,
            &catalog,
            &mut checks,
            "chatgpt",
        )?;
        unsafe {
            SendMessageW(hwnd, 0x8001, 0, 0);
            SendMessageW(hwnd, 0xf, 0, 0);
        }
        expect(
            icon_stat(hwnd, 18) == 0 && icon_stat(hwnd, 6) == 0,
            "disabled visible packaged query creates no worker or icon",
            &mut checks,
        )?;
        sample(child.0.id(), &format!("run{run}_shown_off"), &mut memory)?;
        let started = Instant::now();
        unsafe {
            SendMessageW(hwnd, 0x111, tray::ICONS as usize, 0);
        }
        icons_ready(hwnd)?;
        let cold_icons_ms = started.elapsed().as_secs_f64() * 1000.0;
        sample(child.0.id(), &format!("run{run}_cold_icons"), &mut memory)?;
        timings.push_str(&format!("run={run} cache_cold={} entries={} packaged={packaged_count} startup_ms={startup_ms:.4} cold_icons_ms={cold_icons_ms:.4}\n", run == 0, catalog.entries().len()));
        if !reference {
            expect(
                icon_stat(hwnd, 6) == 1 && icon_stat(hwnd, 14) >= 1 && icon_stat(hwnd, 7) == 0,
                "cold ChatGPT query has one extracted icon without fallback",
                &mut checks,
            )?;
        }
        for (index, entry) in reported.iter().enumerate() {
            let label = ["chatgpt", "store"][index];
            let LaunchTarget::AppUserModelId(id) = &entry.target else {
                unreachable!();
            };
            let extracts_before = icon_stat(hwnd, 14);
            let fallbacks_before = icon_stat(hwnd, 7);
            let visible =
                select_packaged(hwnd, edit, &entry.name, id, &catalog, &mut checks, label)?;
            icons_ready(hwnd)?;
            checks.push_str(&format!("run={run} {label} result_count={visible} icons={} extracts={} fallbacks={} reference={reference}\n", icon_stat(hwnd, 6), icon_stat(hwnd, 14), icon_stat(hwnd, 7)));
            if !reference {
                expect(
                    visible == 1
                        && icon_stat(hwnd, 6) == 1
                        && icon_stat(hwnd, 7) == fallbacks_before
                        && (index == 0 || icon_stat(hwnd, 14) > extracts_before),
                    &format!("run={run}: {label} uses its own extracted icon"),
                    &mut checks,
                )?;
            }
            if run == 0 {
                screenshot(hwnd, &root.join(format!("{label}-dark.bmp")))?;
                unsafe {
                    SendMessageW(hwnd, 0x111, tray::LIGHT as usize, 0);
                    SendMessageW(hwnd, 0xf, 0, 0);
                }
                screenshot(hwnd, &root.join(format!("{label}-light.bmp")))?;
                unsafe {
                    SendMessageW(hwnd, 0x111, tray::DARK as usize, 0);
                }
            }
            // Change the result snapshot and return, forcing a new request against cached identity.
            let hits_before = icon_stat(hwnd, 3);
            let extracts_before = icon_stat(hwnd, 14);
            query_and_paint(hwnd, edit, "PicoRun.NoMatch.qzxv");
            icons_ready(hwnd)?;
            select_packaged(hwnd, edit, &entry.name, id, &catalog, &mut checks, label)?;
            icons_ready(hwnd)?;
            if !reference {
                expect(
                    icon_stat(hwnd, 3) > hits_before && icon_stat(hwnd, 14) == extracts_before,
                    &format!("run={run}: repeated {label} request hits cache without extraction"),
                    &mut checks,
                )?;
            }
        }
        for (index, entry) in catalog.entries().iter().enumerate() {
            let LaunchTarget::AppUserModelId(id) = &entry.target else {
                continue;
            };
            let before = icon_stat(hwnd, 7);
            let visible = select_packaged(
                hwnd,
                edit,
                &entry.name,
                id,
                &catalog,
                &mut checks,
                &format!("run={run} packaged_index={index}"),
            )?;
            icons_ready(hwnd)?;
            if !reference {
                expect(
                    icon_stat(hwnd, 6) == visible as isize && icon_stat(hwnd, 7) == before,
                    &format!(
                        "run={run} packaged_index={index}: visible icons loaded without fallback"
                    ),
                    &mut checks,
                )?;
            }
            icon_bounds(hwnd, &mut checks)?;
        }
        sample(child.0.id(), &format!("run{run}_all_packaged"), &mut memory)?;
        let queries = [
            "chatgpt",
            "Microsoft Store",
            "store",
            "PicoRun.NoMatch.qzxv",
            "",
        ];
        for index in 0..100 {
            query_and_paint(hwnd, edit, queries[index % queries.len()]);
            icons_ready(hwnd)?;
        }
        let mut samples = Vec::with_capacity(100);
        for index in 0..100 {
            let started = Instant::now();
            query_and_paint(hwnd, edit, queries[index % queries.len()]);
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
            // Asynchronous icon readiness is deliberately outside the Edit/synchronous paint timer.
            icons_ready(hwnd)?;
        }
        samples.sort_by(f64::total_cmp);
        sample(child.0.id(), &format!("run{run}_queried"), &mut memory)?;
        select_packaged(
            hwnd,
            edit,
            &reported[0].name,
            first_id,
            &catalog,
            &mut checks,
            "chatgpt",
        )?;
        icons_ready(hwnd)?;
        let extracts_before = icon_stat(hwnd, 14);
        let fallbacks_before = icon_stat(hwnd, 7);
        let invalidations_before = icon_stat(hwnd, 15);
        let started = Instant::now();
        unsafe {
            SendMessageW(hwnd, 0x8002, 0, 0);
        }
        let refresh_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        icons_ready(hwnd)?;
        let refreshed_icons_ms = started.elapsed().as_secs_f64() * 1000.0;
        if !reference {
            expect(
                icon_stat(hwnd, 15) > invalidations_before
                    && icon_stat(hwnd, 14) > extracts_before
                    && icon_stat(hwnd, 7) == fallbacks_before
                    && icon_stat(hwnd, 6) == 1,
                "F5 invalidates AUMID metadata and re-extracts visible packaged icon",
                &mut checks,
            )?;
        }
        icon_bounds(hwnd, &mut checks)?;
        sample(child.0.id(), &format!("run{run}_refreshed"), &mut memory)?;
        timings.push_str(&format!("run={run} warmup=100 query_samples=100 query_p50_ms={:.4} query_p95_ms={:.4} refresh_ms={refresh_ms:.4} refreshed_icons_wait_ms={refreshed_icons_ms:.4} query_scope=WM_SETTEXT+sync_query+WM_PAINT readiness_outside_timer=true\n", samples[49], samples[94]));
        unsafe {
            SendMessageW(hwnd, 0x800a, 0, 0);
        }
        thread::sleep(Duration::from_millis(500));
        let before = sample(child.0.id(), &format!("run{run}_idle_start"), &mut memory)?;
        thread::sleep(Duration::from_secs(2));
        let after = sample(child.0.id(), &format!("run{run}_idle_end"), &mut memory)?;
        checks.push_str(&format!(
            "run={run} idle_interval_ms=2000 cpu_ms={:.3}\n",
            (after - before) as f64 / 10000.0
        ));
        unsafe {
            SendMessageW(hwnd, 0x111, tray::ICONS as usize, 0);
        }
        expect(
            icon_stat(hwnd, 17) == 0
                && icon_stat(hwnd, 18) == 0
                && icon_stat(hwnd, 6) == 0
                && (1..=15)
                    .filter(|key| *key != 4 && *key != 5)
                    .all(|key| icon_stat(hwnd, key) == 0),
            "disable releases worker, visible handles, resource cache and metadata",
            &mut checks,
        )?;
        unsafe {
            SendMessageW(hwnd, 0x8001, 0, 0);
        }
        query_and_paint(hwnd, edit, "Microsoft Store");
        expect(
            icon_stat(hwnd, 18) == 0 && icon_stat(hwnd, 6) == 0,
            "disabled later queries create no worker",
            &mut checks,
        )?;
        sample(child.0.id(), &format!("run{run}_disabled"), &mut memory)?;
        unsafe {
            SendMessageW(hwnd, 0x10, 0, 0);
        }
        expect(
            child.0.wait()?.success(),
            &format!("run={run}: launcher exits normally"),
            &mut checks,
        )?;
        // Preserve each completed process even if a later process finds a regression.
        fs::write(root.join("checks.txt"), &checks)?;
        fs::write(root.join("timings.txt"), &timings)?;
        fs::write(root.join("memory.csv"), &memory)?;
    }
    println!("Packaged icon probe completed: {}", root.display());
    Ok(())
}
