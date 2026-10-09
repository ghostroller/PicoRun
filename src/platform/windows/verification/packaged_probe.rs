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
