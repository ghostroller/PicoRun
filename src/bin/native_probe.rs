//! Runs the real release launcher and a controlled child; raw evidence stays in /runtime.
#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    #[cfg(windows)]
    {
        picorun::platform::windows::attach_console();
        let _ = std::fs::remove_file("runtime/probe-error.txt");
        if let Err(error) = picorun::platform::windows::verification::run() {
            let _ = std::fs::create_dir_all("runtime");
            let _ = std::fs::write("runtime/probe-error.txt", error.to_string());
            eprintln!("native probe failed: {error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    eprintln!("This probe requires Windows.");
}
