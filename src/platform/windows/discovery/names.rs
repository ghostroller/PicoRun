//! Scan-only Shell display names and two bounded alternate names.
use super::*;

#[repr(C)]
#[cfg_attr(target_pointer_width = "32", repr(packed(1)))]
struct FileInfo {
    icon: Handle,
    index: i32,
    attributes: u32,
    name: [u16; 260],
    kind: [u16; 80],
}
#[link(name = "shell32")]
unsafe extern "system" {
    fn SHGetFileInfoW(
        path: *const u16,
        attributes: u32,
        info: *mut FileInfo,
        size: u32,
        flags: u32,
    ) -> usize;
}
fn display_name(path: &Path) -> Option<String> {
    let text = shell_file_path(path);
    // This API's filesystem-path contract is MAX_PATH; longer entries retain
    // their original names and remain launchable through the original path.
    if text.len() > 260 {
        return None;
    }
    let mut info = FileInfo {
        icon: null_mut(),
        index: 0,
        attributes: 0,
        name: [0; 260],
        kind: [0; 80],
    };
    // No icon flags: no image list or owned HICON is requested. The initialized
    // scan STA and bounded buffers outlive this synchronous Shell call.
    if unsafe {
        SHGetFileInfoW(
            text.as_ptr(),
            0,
            &mut info,
            std::mem::size_of::<FileInfo>() as u32,
            0x200,
        )
    } == 0
    {
        return None;
    }
    let end = info.name.iter().position(|&unit| unit == 0)?;
    let mut name = String::from_utf16(&info.name[..end]).ok()?;
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        let suffix = format!(".{extension}");
        if name.len() >= suffix.len()
            && name
                .get(name.len() - suffix.len()..)
                .is_some_and(|end| end.eq_ignore_ascii_case(&suffix))
        {
            name.truncate(name.len() - suffix.len());
        }
    }
    (!name.trim().is_empty()).then_some(name)
}
pub(super) fn entry(original: String, path: PathBuf, target: Option<&str>) -> AppEntry {
    // ClickOnce uses its original reference filename; scanning does not invoke its deployment handler.
    let display = if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
    {
        display_name(&path).unwrap_or_else(|| original.clone())
    } else {
        original.clone()
    };
    from_names(display, original, path, target)
}
fn from_names(display: String, original: String, path: PathBuf, target: Option<&str>) -> AppEntry {
    let mut entry = AppEntry::new(display, LaunchTarget::ShellPath(path));
    entry.add_alias(&original);
    if let Some(target) = target.and_then(|path| Path::new(path).file_stem()) {
        entry.add_alias(&target.to_string_lossy());
    }
    entry
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn localized_name_keeps_original_name_executable_and_both_pinyin_forms() {
        let entry = from_names(
            "本地编辑器".into(),
            "原始快捷方式".into(),
            "demo/raw.lnk".into(),
            Some("C:/Apps/target-editor.exe"),
        );
        let mut engine = crate::search::SearchEngine::default();
        let mut hits = Vec::new();
        for query in [
            "本地编辑器",
            "bendibianjiqi",
            "bdbjq",
            "原始快捷方式",
            "yuanshikuaijiefangshi",
            "yskjfs",
            "target-editor",
        ] {
            engine.search(std::slice::from_ref(&entry), query, &mut hits);
            assert_eq!(hits.len(), 1, "{query}");
        }
        assert_eq!(entry.name, "本地编辑器");
        assert_eq!(entry.target, LaunchTarget::ShellPath("demo/raw.lnk".into()));
    }
}
