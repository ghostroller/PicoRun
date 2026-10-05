//! Independently implemented from Microsoft's MS-SHLLINK format, not launcher source code.
//! Only same-name collisions need extra IO. Unknown/truncated launch data never forms a key.
use super::*;
use std::{collections::HashMap, io::Read, rc::Rc};

const MAX_LINK_BYTES: usize = 256 * 1024;
const MAX_KEY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Default)]
struct Targets {
    values: HashMap<Rc<str>, ()>,
    bytes: usize,
}
impl Targets {
    fn remember(&mut self, target: Option<&str>) -> Option<Rc<str>> {
        let target = target?;
        if let Some((value, ())) = self.values.get_key_value(target) {
            return Some(value.clone());
        }
        let cost = target.len() + 2 * std::mem::size_of::<usize>();
        if self.values.len() >= 512 || cost > (128 * 1024usize).saturating_sub(self.bytes) {
            return None;
        }
        let value: Rc<str> = target.into();
        self.values.insert(value.clone(), ());
        self.bytes += cost;
        Some(value)
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct LaunchKey {
    target: PathKey,
    arguments: Vec<u16>,
    directory: PathKey,
    show: u32,
    flags: u32,
    hotkey: u16,
    // Preserve property-store and tracking differences without interpreting unknown properties.
    extra: Vec<u8>,
}
impl LaunchKey {
    fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.target.owned_bytes()
            + self.directory.owned_bytes()
            + self.arguments.capacity() * 2
            + self.extra.capacity()
    }
}
fn remember(
    keys: &mut HashMap<(usize, LaunchKey), usize>,
    used: &mut usize,
    key: (usize, LaunchKey),
    index: usize,
) {
    let bytes = key.1.owned_bytes() + std::mem::size_of::<usize>();
    if bytes <= MAX_KEY_BYTES.saturating_sub(*used) {
        *used += bytes;
        keys.insert(key, index);
    }
    // Budget exhaustion leaves additional entries independently visible, never falsely merged.
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.position.checked_add(count)?;
        let result = self.bytes.get(self.position..end)?;
        self.position = end;
        Some(result)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn string(&mut self) -> Option<Vec<u16>> {
        let count = usize::from(self.u16()?);
        let bytes = self.take(count.checked_mul(2)?)?;
        let value: Vec<_> = bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        // Embedded NUL and invalid UTF-16 cannot be normalized reliably.
        if value.contains(&0) || String::from_utf16(&value).is_err() {
            return None;
        }
        Some(value)
    }
}

fn local_path(value: &str) -> Option<PathKey> {
    let bytes = value.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
        || value.contains(['%', '\0', '\u{fffd}'])
        || value.encode_utf16().count() >= 32767
    {
        return None;
    }
    // Lexical only: no canonicalize, environment expansion, Resolve, or target-file IO.
    Some(PathKey::new(Path::new(value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(arguments: &str, directory: &str) -> Vec<u8> {
        let mut b = vec![0u8; 0x4c];
        b[..4].copy_from_slice(&0x4cu32.to_le_bytes());
        b[4..20].copy_from_slice(&[1, 0x14, 2, 0, 0, 0, 0, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
        b[20..24].copy_from_slice(&0xb2u32.to_le_bytes());
        b[60..64].copy_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&0x1cu32.to_le_bytes());
        b.extend_from_slice(&0x1cu32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&[0; 16]);
        for s in [directory, arguments] {
            let units: Vec<_> = s.encode_utf16().collect();
            b.extend_from_slice(&(units.len() as u16).to_le_bytes());
            for u in units {
                b.extend_from_slice(&u.to_le_bytes());
            }
        }
        b.extend_from_slice(&[0; 4]);
        b
    }
    #[test]
    fn launch_differences_are_not_folded_or_concatenated() {
        let key = |args, dir| parse(&link(args, dir), "C:\\应用\\app.exe").unwrap();
        assert_ne!(
            key("--Profile A", "C:\\Work"),
            key("--profile A", "C:\\Work")
        );
        assert_ne!(key("A B", "C:\\Work"), key("A  B", "C:\\Work"));
        assert_ne!(key("\"A B\"", "C:\\Work"), key("A B", "C:\\Work"));
        assert_ne!(key("", ""), key("", "C:\\Work"));
        assert_ne!(key("xC:\\A", "C:\\B"), key("x", "C:\\AC:\\B"));
        assert_eq!(key("参数", "C:/Work"), key("参数", "c:\\work"));
        assert_ne!(
            parse(&link("参数", ""), "C:\\app.exe"),
            parse(&link("参数", ""), "C:\\other.exe")
        );
        let normal = link("", "C:\\Work");
        for (offset, value) in [(20, 0x20b2u32), (60, 3), (60, 7)] {
            let mut variant = normal.clone();
            variant[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert_ne!(
                parse(&normal, "C:\\app.exe"),
                parse(&variant, "C:\\app.exe")
            );
        }
    }
    #[test]
    fn unicode_targets_and_working_directories_do_not_form_the_same_launch_key() {
        let ordinary = link("same arguments", "C:\\Work");
        assert_ne!(
            parse(&ordinary, "C:\\İ.exe"),
            parse(&ordinary, "C:\\i\u{0307}.exe")
        );
        assert_ne!(
            parse(&link("same arguments", "C:\\İ"), "C:\\app.exe"),
            parse(&link("same arguments", "C:\\i\u{0307}"), "C:\\app.exe")
        );
        assert_eq!(
            parse(&link("same arguments", "C:/WORK"), "C:/APP.exe"),
            parse(&link("same arguments", "c:\\work"), "c:\\app.EXE")
        );
    }
    #[test]
    fn incomplete_special_and_large_links_have_no_key() {
        let ordinary = link("参数", "C:\\Work");
        for end in 0..ordinary.len() {
            assert!(
                parse(&ordinary[..end], "C:\\app.exe").is_none(),
                "prefix {end}"
            );
        }
        for flag in [0x100, 0x200, 0x400, 0x1000, 0x20000, 0x80000000] {
            let mut b = ordinary.clone();
            b[20..24].copy_from_slice(&(0xb2u32 | flag).to_le_bytes());
            assert!(parse(&b, "C:\\app.exe").is_none());
        }
        for signature in [
            0xa0000001u32,
            0xa0000002,
            0xa0000004,
            0xa0000008,
            0xa0000012,
        ] {
            let mut b = ordinary[..ordinary.len() - 4].to_vec();
            b.extend_from_slice(&8u32.to_le_bytes());
            b.extend_from_slice(&signature.to_le_bytes());
            b.extend_from_slice(&[0; 4]);
            assert!(parse(&b, "C:\\app.exe").is_none());
        }
        assert!(parse(&link("", "."), "C:\\app.exe").is_none());
        assert!(parse(&ordinary, "\\\\server\\app.exe").is_none());
        assert!(parse(&ordinary, "%ROOT%\\app.exe").is_none());
        assert!(parse(&vec![0; MAX_LINK_BYTES + 1], "C:\\app.exe").is_none());
    }
    #[test]
    fn long_arguments_and_property_metadata_are_compared_in_full() {
        let args = "x".repeat(40000);
        let mut second = args.clone();
        second.push('y');
        assert_ne!(
            parse(&link(&args, ""), "C:\\app.exe"),
            parse(&link(&second, ""), "C:\\app.exe")
        );
        let mut first = link("", "");
        first.truncate(first.len() - 4);
        first.extend_from_slice(&12u32.to_le_bytes());
        first.extend_from_slice(&0xa0000009u32.to_le_bytes());
        first.extend_from_slice(&[0; 8]);
        let mut second = first.clone();
        let last_property_byte = second.len() - 5;
        second[last_property_byte] = 1;
        assert!(parse(&first, "C:\\app.exe").is_some());
        assert_ne!(parse(&first, "C:\\app.exe"), parse(&second, "C:\\app.exe"));
    }
    #[test]
    fn metadata_budget_preserves_unremembered_keys() {
        let key = parse(&link("参数", ""), "C:\\app.exe").unwrap();
        let cost = key.owned_bytes() + std::mem::size_of::<usize>();
        let mut used = MAX_KEY_BYTES - cost;
        let mut keys = HashMap::new();
        remember(&mut keys, &mut used, (0, key), 0);
        assert_eq!(used, MAX_KEY_BYTES);
        remember(
            &mut keys,
            &mut used,
            (1, parse(&link("参数", ""), "C:\\app.exe").unwrap()),
            1,
        );
        assert_eq!(keys.len(), 1);
        assert_eq!(used, MAX_KEY_BYTES);
    }
    #[test]
    fn temporary_targets_share_storage_and_have_count_and_byte_bounds() {
        let mut targets = Targets::default();
        let first = targets.remember(Some("C:\\app.exe")).unwrap();
        let second = targets.remember(Some("C:\\app.exe")).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(targets.values.len(), 1);
        for i in 0..600 {
            targets.remember(Some(&format!("C:\\{i}.exe")));
        }
        assert_eq!(targets.values.len(), 512);
        assert!(targets.remember(Some("C:\\overflow.exe")).is_none());
        assert!(targets.remember(Some("C:\\app.exe")).is_some());
        let mut huge = Targets::default();
        assert!(huge.remember(Some(&"x".repeat(128 * 1024))).is_none());
        assert_eq!(huge.bytes, 0);
    }
    #[test]
    fn known_folder_metadata_merges_only_when_the_entire_block_matches() {
        let mut first = link("参数", "C:\\Work");
        first.truncate(first.len() - 4);
        first.extend_from_slice(&0x1cu32.to_le_bytes());
        first.extend_from_slice(&0xa000000bu32.to_le_bytes());
        first.extend_from_slice(&[0; 24]);
        let key = parse(&first, "C:\\app.exe").unwrap();
        assert_eq!(Some(&key), parse(&first, "C:\\app.exe").as_ref());
        let mut second = first.clone();
        let guid_start = second.len() - 24;
        second[guid_start] = 1;
        assert_ne!(Some(&key), parse(&second, "C:\\app.exe").as_ref());
        second = first.clone();
        let offset_start = second.len() - 8;
        second[offset_start] = 2;
        assert_ne!(Some(&key), parse(&second, "C:\\app.exe").as_ref());
        first.pop();
        assert!(parse(&first, "C:\\app.exe").is_none());
    }
}

fn parse(bytes: &[u8], target: &str) -> Option<LaunchKey> {
    if bytes.len() > MAX_LINK_BYTES {
        return None;
    }
    let mut c = Cursor { bytes, position: 0 };
    if c.u32()? != 0x4c || c.take(16)? != [1, 0x14, 2, 0, 0, 0, 0, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0x46]
    {
        return None;
    }
    let flags = c.u32()?;
    // Unicode local file links only. Installer, shim, environment targets, namespace,
    // 16-bit/unknown launch flags fall back to the original independently listed entry.
    const ALLOWED: u32 = 0xff | 0x2000 | 0x4000 | 0x80000;
    if flags & !ALLOWED != 0 || flags & 0x80 == 0 || flags & 3 == 0 {
        return None;
    }
    c.take(36)?;
    let show = c.u32()?;
    let hotkey = c.u16()?;
    if !matches!(show, 1 | 3 | 7) || c.take(10)?.iter().any(|&b| b != 0) {
        return None;
    }
    if flags & 1 != 0 {
        let size = usize::from(c.u16()?);
        c.take(size)?;
    }
    if flags & 2 != 0 {
        let info_start = c.position;
        let info_size = c.u32()? as usize;
        let header_size = c.u32()? as usize;
        let info_flags = c.u32()?;
        if info_size < 0x1c || header_size < 0x1c || header_size > info_size || info_flags != 1 {
            return None;
        }
        c.position = info_start;
        c.take(info_size)?;
    }
    let mut directory = PathKey::default();
    let mut arguments = Vec::new();
    for flag in [4, 8, 0x10, 0x20, 0x40] {
        if flags & flag != 0 {
            let value = c.string()?;
            match flag {
                0x10 => {
                    let text = String::from_utf16(&value).ok()?;
                    if !text.is_empty() {
                        directory = local_path(&text)?;
                    }
                }
                0x20 => arguments = value,
                _ => {}
            }
        }
    }
    let extra_start = c.position;
    loop {
        let start = c.position;
        let size = c.u32()? as usize;
        if size == 0 {
            if c.position != bytes.len() {
                return None;
            }
            break;
        }
        if size < 8 {
            return None;
        }
        let signature = c.u32()?;
        match signature {
            0xa0000003 if size == 0x60 => {}
            0xa0000009 if size >= 12 => {}
            0xa0000007 if size == 0x314 => {}
            // Ordinary redirected-folder shortcuts also carry this standard block.
            // Its GUID and IDList offset remain in the full-byte comparison key.
            0xa000000b if size == 0x1c => {}
            // In particular, console settings, Darwin/MSI, shim and unknown blocks stay separate.
            _ => return None,
        }
        c.position = start;
        c.take(size)?;
    }
    Some(LaunchKey {
        target: local_path(target)?,
        arguments,
        directory,
        show,
        flags: flags & !0xff,
        hotkey,
        extra: bytes[extra_start..].to_vec(),
    })
}

impl ShortcutReader {
    fn launch_key(&mut self, path: &Path, target: &str) -> Option<LaunchKey> {
        let file = fs::File::open(path).ok()?;
        if file.metadata().ok()?.len() > MAX_LINK_BYTES as u64 {
            return None;
        }
        self.metadata.clear();
        // The reusable buffer stays bounded even if the file grows while being read.
        file.take((MAX_LINK_BYTES + 1) as u64)
            .read_to_end(&mut self.metadata)
            .ok()?;
        parse(&self.metadata, target)
    }
}

struct Candidate {
    name: String,
    path: PathBuf,
    priority: usize,
    target: Option<Rc<str>>,
}
struct Group {
    first: usize,
    checked: bool,
}
#[derive(Default)]
pub(super) struct Collector {
    candidates: Vec<Candidate>,
    groups: HashMap<String, Group>,
    keys: HashMap<(usize, LaunchKey), usize>,
    targets: Targets,
    key_bytes: usize,
}
impl Collector {
    pub fn insert(
        &mut self,
        name: String,
        path: PathBuf,
        priority: usize,
        target: Option<String>,
        reader: &mut ShortcutReader,
    ) {
        let normalized = name.trim().to_lowercase();
        let next = self.candidates.len();
        let group = self.groups.entry(normalized).or_insert(Group {
            first: next,
            checked: false,
        });
        // Remember only the first member's already-read target. Identical targets share
        // one allocation; overflow merely falls back to a second COM read on collision.
        let initial_target = if group.first == next {
            self.targets.remember(target.as_deref())
        } else {
            None
        };
        if group.first != next {
            if !group.checked {
                let first = &self.candidates[group.first];
                if first
                    .path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
                {
                    let key = if let Some(target) = &first.target {
                        reader.launch_key(&first.path, target)
                    } else {
                        reader
                            .application_target(&first.path)
                            .and_then(|target| reader.launch_key(&first.path, &target))
                    };
                    if let Some(key) = key {
                        remember(
                            &mut self.keys,
                            &mut self.key_bytes,
                            (group.first, key),
                            group.first,
                        );
                    }
                }
                group.checked = true;
            }
            if let Some(key) = target.and_then(|target| reader.launch_key(&path, &target)) {
                let key = (group.first, key);
                if let Some(&index) = self.keys.get(&key) {
                    let old = &mut self.candidates[index];
                    let better = priority < old.priority
                        || priority == old.priority
                            && PathKey::new(&path) < PathKey::new(&old.path);
                    if better {
                        *old = Candidate {
                            name,
                            path,
                            priority,
                            target: None,
                        };
                    }
                    return;
                }
                remember(&mut self.keys, &mut self.key_bytes, key, next);
            }
        }
        self.candidates.push(Candidate {
            name,
            path,
            priority,
            target: initial_target,
        });
    }
    pub fn into_entries(self) -> Vec<AppEntry> {
        let Self {
            mut candidates,
            groups,
            keys,
            targets,
            ..
        } = self;
        drop(groups);
        drop(keys);
        drop(targets);
        for candidate in &mut candidates {
            candidate.target = None;
        }
        candidates
            .into_iter()
            .map(|c| AppEntry::new(c.name, LaunchTarget::ShellPath(c.path)))
            .collect()
    }
}
