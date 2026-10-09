//! Reversible application-discovery fixtures; never launches a user's installed application.
use super::*;
use crate::{catalog::Catalog, model::LaunchTarget, search::SearchEngine};
use std::os::windows::ffi::OsStrExt;

const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
const HKCU: Handle = -2147483647isize as Handle;

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegCreateKeyExW(
        key: Handle,
        name: *const u16,
        reserved: u32,
        class: *mut u16,
        options: u32,
        access: u32,
        security: *const c_void,
        result: *mut Handle,
        disposition: *mut u32,
    ) -> i32;
    fn RegSetValueExW(
        key: Handle,
        name: *const u16,
        reserved: u32,
        kind: u32,
        data: *const u8,
        size: u32,
    ) -> i32;
    fn RegCloseKey(key: Handle) -> i32;
    fn RegDeleteKeyExW(key: Handle, name: *const u16, view: u32, reserved: u32) -> i32;
}
fn registry_result(code: i32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code))
    }
}
struct Key(Handle);
impl Drop for Key {
    fn drop(&mut self) {
        // A real handle from RegCreateKeyExW; predefined HKCU is never closed.
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
struct Registration {
    name: String,
    view: u32,
}
impl Registration {
    fn create(name: &str, view: u32) -> io::Result<Self> {
        let name = format!(r"{APP_PATHS}\{name}");
        let mut key = null_mut();
        let mut disposition = 0;
        registry_result(unsafe {
            // All pointers remain live for this synchronous call; only a unique HKCU fixture is created.
            RegCreateKeyExW(
                HKCU,
                wide(&name).as_ptr(),
                0,
                null_mut(),
                0,
                2 | view,
                null(),
                &mut key,
                &mut disposition,
            )
        })?;
        drop(Key(key));
        if disposition != 1 {
            // Never alter or remove a preexisting registration, even if a fixture name collides.
            return Err(io::Error::other(
                "discovery registry fixture already exists",
            ));
        }
        Ok(Self { name, view })
    }
    fn set(&self, name: &str, value: &str, kind: u32) -> io::Result<()> {
        let mut key = null_mut();
        registry_result(unsafe {
            RegCreateKeyExW(
                HKCU,
                wide(&self.name).as_ptr(),
                0,
                null_mut(),
                0,
                2 | self.view,
                null(),
                &mut key,
                null_mut(),
            )
        })?;
        let key = Key(key);
        let value = wide(value);
        registry_result(unsafe {
            // UTF-16 data, including NUL, stays live until RegSetValueExW returns.
            RegSetValueExW(
                key.0,
                wide(name).as_ptr(),
                0,
                kind,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        })
    }
    fn remove(&self) -> io::Result<()> {
        let code = unsafe { RegDeleteKeyExW(HKCU, wide(&self.name).as_ptr(), self.view, 0) };
        if code == 2 {
            Ok(())
        } else {
            registry_result(code)
        }
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}
struct Files(Vec<PathBuf>);
impl Files {
    fn claim(&mut self, path: PathBuf) -> io::Result<PathBuf> {
        // create_new proves ownership of this exact path before any cleanup is allowed.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.0.push(path.clone());
        Ok(path)
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}
fn target_hash(path: &Path) -> isize {
    path.as_os_str()
        .encode_wide()
        .fold(2166136261u32, |hash, unit| {
            (hash ^ u32::from(unit)).wrapping_mul(16777619)
        }) as isize
}
pub(super) fn is_environment_child() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_owned()))
        .is_some_and(|name| name.to_string_lossy().starts_with("PicoRunDiscoveryEnv-"))
}
pub(super) fn environment_child() -> io::Result<()> {
    let marker = std::env::var_os("PICORUN_DISCOVERY_MARKER")
        .ok_or_else(|| io::Error::other("controlled registry child requires an isolated marker"))?;
    fs::write(
        marker,
        format!(
            "cwd={}\npath={}\nargs={}\n",
            std::env::current_dir()?.display(),
            std::env::var_os("PATH")
                .unwrap_or_default()
                .to_string_lossy(),
            std::env::args_os()
                .skip(1)
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("|"),
        ),
    )
}

const START_MENU: Guid = Guid {
    a: 0x625b53c3,
    b: 0xab48,
    c: 0x4ec1,
    d: [0xba, 0x1f, 0xa1, 0xef, 0x41, 0x46, 0xfc, 0x19],
};

fn options(data: &Path, source: Option<&Path>) -> Vec<std::ffi::OsString> {
    let mut args = vec![
        "--hidden".into(),
        "--hotkey".into(),
        "Ctrl+Alt+Shift+F10".into(),
        "--data-dir".into(),
        data.into(),
        "--icons".into(),
        "off".into(),
        "--measure-icons".into(),
        "--hold-measurement-window".into(),
    ];
    if let Some(source) = source {
        args.extend(["--source".into(), source.into()]);
    }
    args
}
fn start_launcher(exe: &Path, args: &[std::ffi::OsString], marker: &Path) -> io::Result<Running> {
    Ok(Running(
        Command::new(exe)
            .args(args)
            .env("PICORUN_DISCOVERY_MARKER", marker)
            .env("PICORUN_DISCOVERY_ROOT", marker.parent().unwrap())
            .creation_flags(0x08000000)
            .spawn()?,
    ))
}
fn query_and_paint(hwnd: Hwnd, edit: Hwnd, query: &str) {
    unsafe {
        set_control_text(edit, wide(query).as_ptr());
        SendMessageW(hwnd, 0x8003, 0, 0);
        SendMessageW(hwnd, 0xf, 0, 0);
    }
}
fn has_path(catalog: &Catalog, path: &Path) -> bool {
    catalog
        .entries()
        .iter()
        .any(|entry| matches!(&entry.target, LaunchTarget::ShellPath(actual) if actual == path))
}
fn assert_path_query(
    hwnd: Hwnd,
    edit: Hwnd,
    query: &str,
    expected: &Path,
    checks: &mut String,
) -> io::Result<()> {
    query_and_paint(hwnd, edit, query);
    expect(
        unsafe { SendMessageW(hwnd, 0x800f, 0, 0) } == 1
            && unsafe { SendMessageW(hwnd, 0x800f, 1, 0) } == target_hash(expected),
        &format!("native Edit query {query:?} selects original shortcut identity"),
        checks,
    )
}
fn quit(child: &mut Running, hwnd: Hwnd, checks: &mut String) -> io::Result<()> {
    unsafe {
        PostMessageW(hwnd, 0x10, 0, 0);
    }
    let start = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait()? {
            return expect(
                status.success(),
                &format!("controlled launcher exits normally: {status}"),
                checks,
            );
        }
        if start.elapsed() > Duration::from_secs(5) {
            return Err(io::Error::other("discovery launcher exit timed out"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn custom_sources(
    root: &Path,
    launcher: &Path,
    target: &Path,
    marker: &Path,
    work: &Path,
    checks: &mut String,
    memory: &mut String,
) -> io::Result<()> {
    let source = root.join(format!("custom-source-{}", std::process::id()));
    let data = root.join(format!("custom-data-{}", std::process::id()));
    fs::create_dir_all(&source)?;
    fs::create_dir_all(&data)?;
    fs::write(data.join("english-input.txt"), "on\n")?;
    let pid = std::process::id();
    let raw_name = format!("原始快捷方式{pid}");
    let shortcut = source.join(format!("{raw_name}.lnk"));
    create_shortcut(
        &shortcut,
        target,
        &format!(
            "--controlled-child \"{}\" \"参数 with spaces\" custom",
            marker.display()
        ),
        work,
    )?;
    let reference = source.join(format!("ClickOnce Probe {pid}.appref-ms"));
    // A controlled reference is indexed but never activated; activating it might install an app.
    fs::write(&reference, "https://invalid.example/PicoRunControlled.application#PicoRunControlled.application, Culture=neutral, PublicKeyToken=0000000000000000, processorArchitecture=msil")?;
    fs::write(source.join("document.txt"), "excluded")?;
    let cache_path = data.join("apps-v1.bin");
    let _ = fs::remove_file(&cache_path);
    let queries = [
        raw_name.clone(),
        format!("yuanshikuaijiefangshi{pid}"),
        format!("yskjfs{pid}"),
        target.file_stem().unwrap().to_string_lossy().into_owned(),
    ];
    for run in 0..2 {
        let mut child = start_launcher(launcher, &options(&data, Some(&source)), marker)?;
        let (hwnd, edit) = wait_window(&mut child)?;
        unsafe {
            SendMessageW(hwnd, 0x8008, 0, 0);
        }
        let catalog = cache::load(&cache_path)?;
        expect(catalog.entries().len() == 2 && has_path(&catalog, &shortcut) && has_path(&catalog, &reference)
            && catalog.entries().iter().all(|entry| matches!(entry.target, LaunchTarget::ShellPath(_))),
            &format!("custom run={run}: --source isolates shortcuts and ClickOnce from registered/packaged defaults"), checks)?;
        sample(child.0.id(), &format!("custom{run}_hidden"), memory)?;
        unsafe {
            SendMessageW(hwnd, 0x8001, 0, 0);
        }
        for query in &queries {
            assert_path_query(hwnd, edit, query, &shortcut, checks)?;
        }
        query_and_paint(hwnd, edit, &format!("ClickOnce Probe {pid}"));
        expect(
            unsafe { SendMessageW(hwnd, 0x800f, 1, 0) } == target_hash(&reference),
            "ClickOnce keeps original Shell path through index and cache reload",
            checks,
        )?;
        if run == 0 {
            assert_path_query(hwnd, edit, &queries[0], &shortcut, checks)?;
            let _ = fs::remove_file(marker);
            unsafe {
                SendMessageW(edit, 0x100, 0x0d, 1);
            }
            let launched = wait_marker(marker)?;
            check_shortcut_launch(&launched, work, "custom", &root.join("cwd-comparisons.txt"), "original .lnk launches controlled child preserving Unicode arguments and working directory", checks)?;
            fs::remove_file(&reference)?;
            expect(
                discovery::launch(null_mut(), &LaunchTarget::ShellPath(reference.clone())).is_err(),
                "missing ClickOnce reference reports failure without activating an installer",
                checks,
            )?;
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
                SendMessageW(hwnd, 0x8002, 0, 0);
            }
            expect(
                cache::load(&cache_path)?.entries().len() == 1,
                "manual refresh drops removed ClickOnce reference",
                checks,
            )?;
            fs::write(&reference, "https://invalid.example/PicoRunControlled.application#PicoRunControlled.application")?;
            unsafe {
                SendMessageW(hwnd, 0x8002, 0, 0);
            }
            expect(
                has_path(&cache::load(&cache_path)?, &reference),
                "manual refresh rediscovers ClickOnce reference",
                checks,
            )?;
            assert_path_query(hwnd, edit, &queries[0], &shortcut, checks)?;
            screenshot(hwnd, &root.join("custom-shortcut.bmp"))?;
        }
        sample(child.0.id(), &format!("custom{run}_queried"), memory)?;
        quit(&mut child, hwnd, checks)?;
    }
    checks.push_str("BOUNDARY ClickOnce successful activation was not tested: no ClickOnce application installed and no file association changed.\n");
    Ok(())
}

fn registered<'a>(
    catalog: &'a Catalog,
    executable: &Path,
) -> Option<&'a crate::model::RegisteredApp> {
    catalog
        .entries()
        .iter()
        .find_map(|entry| match &entry.target {
            LaunchTarget::AppPath(app) if app.executable == executable => Some(app.as_ref()),
            _ => None,
        })
}
fn registered_query(
    hwnd: Hwnd,
    edit: Hwnd,
    query: &str,
    path: &Path,
    catalog: &Catalog,
    checks: &mut String,
) -> io::Result<()> {
    let mut hits = Vec::new();
    SearchEngine::default().search(catalog.entries(), query, &mut hits);
    expect(hits.first().is_some_and(|hit| matches!(&catalog.entries()[hit.entry_index].target, LaunchTarget::AppPath(app) if app.executable == path)),
        "precomputed registry key/executable alias selects the registered fixture", checks)?;
    query_and_paint(hwnd, edit, query);
    expect(
        unsafe { SendMessageW(hwnd, 0x800f, 0, 0) } == 3
            && unsafe { SendMessageW(hwnd, 0x800f, 1, 0) } == target_hash(path),
        "native Edit selects exact registered executable",
        checks,
    )
}
fn open_registered(
    hwnd: Hwnd,
    edit: Hwnd,
    query: &str,
    expected: (&Path, &Path),
    marker: &Path,
    catalog: &Catalog,
    checks: &mut String,
) -> io::Result<()> {
    let (target, expected_path) = expected;
    registered_query(hwnd, edit, query, target, catalog, checks)?;
    let _ = fs::remove_file(marker);
    unsafe {
        SendMessageW(edit, 0x100, 0x0d, 1);
    }
    let launched = wait_marker(marker)?;
    let path = launched
        .lines()
        .find_map(|line| line.strip_prefix("path="))
        .unwrap_or_default();
    expect(
        std::env::split_paths(path).any(|component| component == expected_path)
            && launched.ends_with("args=\n"),
        "registered launch passes App Paths Path to controlled no-argument child",
        checks,
    )?;
    unsafe {
        SendMessageW(hwnd, 0x8001, 0, 0);
    }
    Ok(())
}
struct Reports {
    checks: String,
    timings: String,
    memory: String,
}
fn default_sources(
    root: &Path,
    launcher: &Path,
    controlled: &Path,
    marker: &Path,
    work: &Path,
    reports: &mut Reports,
) -> io::Result<()> {
    let Reports {
        checks,
        timings,
        memory,
    } = reports;
    let pid = std::process::id();
    let data = root.join("default-data");
    fs::create_dir_all(&data)?;
    fs::write(data.join("english-input.txt"), "on\n")?;
    let cache_path = data.join("apps-v1.bin");
    let _ = fs::remove_file(&cache_path);
    let mut files = Files(Vec::new());
    let root_name = format!("PicoRunRootProbe-{pid}");
    let root_shortcut =
        files.claim(discovery::known_folder(&START_MENU)?.join(format!("{root_name}.lnk")))?;
    create_shortcut(
        &root_shortcut,
        controlled,
        &format!(
            "--controlled-child \"{}\" \"参数 with spaces\" root",
            marker.display()
        ),
        work,
    )?;
    let key32 = format!("PicoRunDiscoveryKey32-{pid}.exe");
    let key64 = format!("PicoRunDiscoveryKey64-{pid}.exe");
    let target32 = root.join(format!("PicoRunDiscoveryEnv-32-{pid}.exe"));
    let target64 = root.join(format!("PicoRunDiscoveryEnv-64-{pid}.exe"));
    fs::copy(std::env::current_exe()?, &target32)?;
    fs::copy(std::env::current_exe()?, &target64)?;
    let path_a = root.join("注册 Path A");
    let path_b = root.join("注册 Path B");
    fs::create_dir_all(&path_a)?;
    fs::create_dir_all(&path_b)?;
    let reg32 = Registration::create(&key32, 0x200)?;
    reg32.set("", &format!("\"{}\"", target32.display()), 1)?;
    reg32.set("Path", &path_a.to_string_lossy(), 1)?;
    let reg64 = Registration::create(&key64, 0x100)?;
    reg64.set(
        "",
        &format!(
            "%PICORUN_DISCOVERY_ROOT%\\{}",
            target64.file_name().unwrap().to_string_lossy()
        ),
        2,
    )?;
    reg64.set("Path", &path_a.to_string_lossy(), 1)?;
    let stale = Registration::create(&format!("PicoRunDiscoveryStale-{pid}.exe"), 0x100)?;
    let stale_path = root.join(format!("PicoRunDiscoveryAbsent-{pid}.exe"));
    stale.set("", &stale_path.to_string_lossy(), 1)?;
    let invalid = Registration::create(&format!("PicoRunDiscoveryInvalid-{pid}.exe"), 0x100)?;
    invalid.set(
        "",
        &format!("\"{}\" --unsupported-argument", target32.display()),
        1,
    )?;
    let parent_path = std::env::var_os("PATH");
    for run in 0..3 {
        let start = Instant::now();
        let mut child = start_launcher(launcher, &options(&data, None), marker)?;
        let (hwnd, edit) = wait_window(&mut child)?;
        unsafe {
            SendMessageW(hwnd, 0x8008, 0, 0);
        }
        let startup = start.elapsed().as_secs_f64() * 1000.0;
        let catalog = cache::load(&cache_path)?;
        expect(
            has_path(&catalog, &root_shortcut),
            "default scan discovers a controlled shortcut in StartMenu root",
            checks,
        )?;
        expect(registered(&catalog, &target32).is_some() && registered(&catalog, &target64).is_some()
            && registered(&catalog, &stale_path).is_none(),
            "both HKCU registry view fixtures discovered; stale and argument-bearing registrations excluded", checks)?;
        expect(catalog.entries().iter().filter(|entry| matches!(&entry.target, LaunchTarget::AppPath(app) if app.executable == target32)).count() == 1,
            "shared HKCU view registration produces one stable identity", checks)?;
        sample(child.0.id(), &format!("default{run}_hidden"), memory)?;
        unsafe {
            SendMessageW(hwnd, 0x8001, 0, 0);
        }
        assert_path_query(hwnd, edit, &root_name, &root_shortcut, checks)?;
        registered_query(
            hwnd,
            edit,
            key32.trim_end_matches(".exe"),
            &target32,
            &catalog,
            checks,
        )?;
        registered_query(
            hwnd,
            edit,
            key64.trim_end_matches(".exe"),
            &target64,
            &catalog,
            checks,
        )?;
        if run == 0 {
            open_registered(
                hwnd,
                edit,
                key32.trim_end_matches(".exe"),
                (&target32, &path_a),
                marker,
                &catalog,
                checks,
            )?;
            let updated = path_b.to_string_lossy();
            reg32.set("Path", &updated, 1)?;
            refresh_from_edit(edit, &cache_path, |catalog| {
                registered(catalog, &target32).and_then(|app| app.path.as_deref())
                    == Some(updated.as_ref())
            })?;
            let refreshed = cache::load(&cache_path)?;
            expect(
                registered(&refreshed, &target32).and_then(|app| app.path.as_deref())
                    == Some(updated.as_ref()),
                "F5 refresh persists updated App Paths Path metadata",
                checks,
            )?;
            open_registered(
                hwnd,
                edit,
                key32.trim_end_matches(".exe"),
                (&target32, &path_b),
                marker,
                &refreshed,
                checks,
            )?;
            open_registered(
                hwnd,
                edit,
                key64.trim_end_matches(".exe"),
                (&target64, &path_a),
                marker,
                &refreshed,
                checks,
            )?;
            let launched = fs::read_to_string(marker)?;
            let inherited = launched
                .lines()
                .find_map(|line| line.strip_prefix("path="))
                .unwrap_or_default();
            expect(
                !std::env::split_paths(inherited).any(|component| component == path_b),
                "second registered launch proves first launch did not mutate launcher PATH",
                checks,
            )?;
            reg32.remove()?;
            unsafe {
                SendMessageW(hwnd, 0x8002, 0, 0);
            }
            expect(
                registered(&cache::load(&cache_path)?, &target32).is_none(),
                "F5 drops removed App Paths registration",
                checks,
            )?;
            reg32.set("", &format!("\"{}\"", target32.display()), 1)?;
            reg32.set("Path", &updated, 1)?;
            unsafe {
                SendMessageW(hwnd, 0x8002, 0, 0);
            }
            expect(
                registered(&cache::load(&cache_path)?, &target32).is_some(),
                "F5 rediscovers restored App Paths registration",
                checks,
            )?;
            assert_path_query(hwnd, edit, &root_name, &root_shortcut, checks)?;
            let _ = fs::remove_file(marker);
            unsafe {
                SendMessageW(edit, 0x100, 0x0d, 1);
            }
            let launched = wait_marker(marker)?;
            check_shortcut_launch(
                &launched,
                work,
                "root",
                &root.join("cwd-comparisons.txt"),
                "new StartMenu root shortcut keeps original Shell arguments and working directory",
                checks,
            )?;
            unsafe {
                SendMessageW(hwnd, 0x8001, 0, 0);
            }
            assert_path_query(hwnd, edit, &root_name, &root_shortcut, checks)?;
            screenshot(hwnd, &root.join("start-menu-root.bmp"))?;
        } else {
            expect(
                registered(&catalog, &target32).and_then(|app| app.path.as_deref())
                    == Some(path_b.to_string_lossy().as_ref()),
                "independent process reloads updated registered launch metadata",
                checks,
            )?;
        }
        sample(child.0.id(), &format!("default{run}_shown"), memory)?;
        let queries = [
            root_name.as_str(),
            key32.trim_end_matches(".exe"),
            key64.trim_end_matches(".exe"),
            "PicoRun.NoMatch.qzxv",
            "",
        ];
        for index in 0..50 {
            query_and_paint(hwnd, edit, queries[index % queries.len()]);
        }
        let mut response = Vec::with_capacity(100);
        for index in 0..100 {
            let start = Instant::now();
            query_and_paint(hwnd, edit, queries[index % queries.len()]);
            response.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        response.sort_by(f64::total_cmp);
        sample(child.0.id(), &format!("default{run}_queried"), memory)?;
        let start = Instant::now();
        unsafe {
            SendMessageW(hwnd, 0x8002, 0, 0);
        }
        let refresh = start.elapsed().as_secs_f64() * 1000.0;
        sample(child.0.id(), &format!("default{run}_refreshed"), memory)?;
        unsafe {
            SendMessageW(hwnd, 0x800a, 0, 0);
        }
        thread::sleep(Duration::from_millis(500));
        let before = sample(child.0.id(), &format!("default{run}_idle_start"), memory)?;
        thread::sleep(Duration::from_secs(2));
        let after = sample(child.0.id(), &format!("default{run}_idle_end"), memory)?;
        timings.push_str(&format!("run={run} cache_cold={} entries={} startup_ms={startup:.4} query_samples=100 warmup=50 query_p50_ms={:.4} query_p95_ms={:.4} refresh_ms={refresh:.4} idle_interval_ms=2000 idle_cpu_ms={:.3}\n",
            run == 0, catalog.entries().len(), response[49], response[94], (after-before) as f64 / 10000.0));
        quit(&mut child, hwnd, checks)?;
        fs::write(root.join("checks-progress.txt"), &*checks)?;
        fs::write(root.join("timings-progress.txt"), &*timings)?;
        fs::write(root.join("memory-progress.csv"), &*memory)?;
    }
    expect(
        std::env::var_os("PATH") == parent_path,
        "verification parent PATH stays unchanged",
        checks,
    )?;
    drop(files);
    expect(
        !root_shortcut.exists(),
        "exact owned StartMenu root fixture removed",
        checks,
    )?;
    checks.push_str("BOUNDARY HKLM is read by production but never mutated by this probe; private-source tests cover distinct registry view mappings; actual HKLM fixtures and Windows conflict precedence are not tested. Startup exclusion uses isolated scanner tests; no real Startup entries are created.\n");
    Ok(())
}

pub(super) fn run() -> io::Result<()> {
    super::super::window::enable_dpi();
    let root = std::env::current_dir()?.join("runtime/probe-discovery-sources");
    let work = root.join("受控工作目录");
    let _ = fs::remove_file(root.join("error.txt"));
    fs::create_dir_all(&work)?;
    let launcher = std::env::current_exe()?.with_file_name("picorun.exe");
    let controlled = root.join(format!("PicoRunDiscoveryAlias-{}.exe", std::process::id()));
    fs::copy(std::env::current_exe()?, &controlled)?;
    let marker = root.join("controlled-marker.txt");
    let mut reports = Reports { checks: String::new(), timings: String::new(), memory: String::from("stage,private_bytes,working_set_bytes,peak_commit_bytes,peak_working_set_bytes,gdi_handles,user_handles,cpu_100ns\n") };
    if unsafe { CoInitializeEx(null_mut(), 2) } < 0 {
        return Err(io::Error::other(
            "discovery probe COM initialization failed",
        ));
    }
    let result: io::Result<()> = (|| {
        default_sources(&root, &launcher, &controlled, &marker, &work, &mut reports)?;
        custom_sources(
            &root,
            &launcher,
            &controlled,
            &marker,
            &work,
            &mut reports.checks,
            &mut reports.memory,
        )?;
        Ok(())
    })();
    unsafe {
        CoUninitialize();
    }
    fs::write(root.join("checks.txt"), &reports.checks)?;
    fs::write(root.join("timings.txt"), &reports.timings)?;
    fs::write(root.join("memory.csv"), &reports.memory)?;
    if let Err(error) = &result {
        fs::write(root.join("error.txt"), error.to_string())?;
    }
    println!("Discovery probe: {}\n{}", root.display(), reports.checks);
    result
}

fn refresh_from_edit(
    edit: Hwnd,
    cache_path: &Path,
    ready: impl Fn(&Catalog) -> bool,
) -> io::Result<()> {
    unsafe {
        SendMessageW(edit, 0x100, 0x74, 1);
        SendMessageW(edit, 0x101, 0x74, 1);
    }
    let start = Instant::now();
    loop {
        if cache::load(cache_path).is_ok_and(|catalog| ready(&catalog)) {
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(5) {
            return Err(io::Error::other("native Edit F5 refresh timed out"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn check_shortcut_launch(
    launched: &str,
    work: &Path,
    tag: &str,
    diagnostics: &Path,
    description: &str,
    checks: &mut String,
) -> io::Result<()> {
    let actual_cwd = launched
        .lines()
        .find_map(|line| line.strip_prefix("cwd="))
        .ok_or_else(|| io::Error::other("controlled child marker lacks cwd"))?;
    let actual_args = launched
        .lines()
        .find_map(|line| line.strip_prefix("args="))
        .ok_or_else(|| io::Error::other("controlled child marker lacks args"))?;
    let expected_args = format!("参数 with spaces|{tag}");
    let expected_cwd = fs::canonicalize(work)?;
    let received_cwd = fs::canonicalize(actual_cwd)?;
    let args_match = actual_args == expected_args;
    let cwd_match = received_cwd == expected_cwd;
    let trace = format!("tag={tag}\nexpected_raw_cwd={work:?}\nactual_raw_cwd={actual_cwd:?}\nexpected_canonical_cwd={expected_cwd:?}\nactual_canonical_cwd={received_cwd:?}\nexpected_args={expected_args:?}\nactual_args={actual_args:?}\noriginal_cwd_contains={}\nargs_equal={args_match}\ncanonical_cwd_equal={cwd_match}\nmarker={launched:?}\n\n", launched.contains(&format!("cwd={}", work.display())));
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(diagnostics)?
        .write_all(trace.as_bytes())?;
    expect(args_match && cwd_match, description, checks)
}
