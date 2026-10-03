//! Worker-local metadata and icon caches. Only visible requests cause file access.
use super::{generic, Icon, CAPACITY};
use std::{
    cell::Cell,
    collections::VecDeque,
    ffi::OsString,
    mem::size_of,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    ptr::null_mut,
    rc::Rc,
    sync::Arc,
    time::Instant,
};

const MAX_ENTRIES: usize = 512;
pub const BYTE_BUDGET: usize = 128 * 1024;

#[derive(Default, Clone)]
pub struct Stats {
    pub hits: usize,
    pub misses: usize,
    pub fallbacks: usize,
    pub max_load_us: usize,
    pub total_load_us: usize,
    pub cache: usize,
    pub parses: usize,
    pub metadata_hits: usize,
    pub metadata_entries: usize,
    pub metadata_bytes: usize,
    pub generic_copies: usize,
    pub shared_resources: usize,
    pub extracts: usize,
    pub invalidations: usize,
}

#[derive(PartialEq, Eq)]
struct Key {
    path: PathBuf,
    index: i32,
}
struct Resource {
    key: Key,
    absent: Cell<bool>,
}
impl Resource {
    fn bytes(&self) -> usize {
        size_of::<Self>() + 2 * size_of::<usize>() + self.key.path.capacity()
    }
}
struct Entry {
    path: PathBuf,
    resource: Option<Rc<Resource>>,
}
impl Entry {
    fn owned_bytes(&self) -> usize {
        self.path.capacity() + self.resource.as_ref().map_or(0, |r| r.bytes())
    }
}

pub struct Loader {
    entries: VecDeque<Entry>,
    // Fixed extraction size: all handles are ExtractIconExW large icons. Renderer is unchanged.
    icons: Vec<(Rc<Resource>, Arc<Icon>)>,
    generic: Option<Arc<Icon>>,
    generic_attempted: bool,
    resource_buffer: Vec<u16>,
    expanded_buffer: Vec<u16>,
    counters: Stats,
}
impl Loader {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            icons: Vec::with_capacity(CAPACITY),
            generic: None,
            generic_attempted: false,
            resource_buffer: vec![0; 32768],
            expanded_buffer: vec![0; 32768],
            counters: Stats::default(),
        }
    }
    fn bytes(&self) -> usize {
        // Shared resource strings are deliberately charged for every reference: conservative
        // retained-cache accounting including allocated container slots and Rc control blocks.
        self.entries.capacity() * size_of::<Entry>()
            + self.entries.iter().map(Entry::owned_bytes).sum::<usize>()
            + self.icons.capacity() * size_of::<(Rc<Resource>, Arc<Icon>)>()
            + self.icons.iter().map(|(r, _)| r.bytes()).sum::<usize>()
    }
    fn trim(&mut self) {
        while self.entries.len() > MAX_ENTRIES || self.bytes() > BYTE_BUDGET {
            if self.entries.pop_front().is_none() {
                if self.icons.is_empty() {
                    break;
                }
                self.icons.remove(0);
            }
        }
    }
    fn store(&mut self, path: &Path, resource: Option<Rc<Resource>>) {
        if self.entries.len() == MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            path: path.to_path_buf(),
            resource,
        });
        self.trim();
    }
    fn intern(&mut self, key: Key) -> Rc<Resource> {
        let existing = self
            .entries
            .iter()
            .filter_map(|e| e.resource.as_ref())
            .chain(self.icons.iter().map(|(r, _)| r))
            .find(|r| r.key == key);
        if let Some(resource) = existing {
            self.counters.shared_resources += 1;
            Rc::clone(resource)
        } else {
            Rc::new(Resource {
                key,
                absent: Cell::new(false),
            })
        }
    }
    fn fallback(&mut self) -> Option<Arc<Icon>> {
        self.counters.fallbacks += 1;
        if !self.generic_attempted {
            self.generic_attempted = true;
            self.generic = generic();
            self.counters.generic_copies += usize::from(self.generic.is_some());
        }
        self.generic.clone()
    }
    pub fn load(&mut self, path: &Path) -> Option<Arc<Icon>> {
        let started = Instant::now();
        let (resource, metadata_hit) =
            if let Some(index) = self.entries.iter().position(|e| e.path == path) {
                self.counters.metadata_hits += 1;
                let entry = self.entries.remove(index).unwrap();
                let resource = entry.resource.clone();
                self.entries.push_back(entry);
                (resource, true)
            } else {
                self.counters.parses += 1;
                let key = resolve(path, &mut self.resource_buffer, &mut self.expanded_buffer);
                let resource = key.map(|key| self.intern(key));
                self.store(path, resource.clone());
                (resource, false)
            };
        let Some(resource) = resource else {
            if metadata_hit {
                self.counters.hits += 1;
            } else {
                self.counters.misses += 1;
                self.record_load(started);
            }
            return self.fallback();
        };
        if resource.absent.get() {
            self.counters.hits += 1;
            return self.fallback();
        }
        if let Some(index) = self
            .icons
            .iter()
            .position(|(r, _)| Rc::ptr_eq(r, &resource))
        {
            self.counters.hits += 1;
            let cached = self.icons.remove(index);
            let icon = Arc::clone(&cached.1);
            self.icons.push(cached);
            return Some(icon);
        }
        self.counters.misses += 1;
        self.counters.extracts += 1;
        let icon = extract(&resource.key);
        self.record_load(started);
        if let Some(icon) = icon {
            if self.icons.len() == CAPACITY {
                self.icons.remove(0);
            }
            self.icons.push((resource, Arc::clone(&icon)));
            self.trim();
            Some(icon)
        } else {
            resource.absent.set(true);
            self.fallback()
        }
    }
    fn record_load(&mut self, started: Instant) {
        let micros = started.elapsed().as_micros() as usize;
        self.counters.total_load_us += micros;
        self.counters.max_load_us = self.counters.max_load_us.max(micros);
    }
    pub fn stats(&self) -> Stats {
        Stats {
            cache: self.icons.len(),
            metadata_entries: self.entries.len(),
            metadata_bytes: self.bytes(),
            ..self.counters.clone()
        }
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.icons.clear();
        self.counters.invalidations += 1;
    }
}

fn resolve(path: &Path, resource: &mut [u16], expanded: &mut [u16]) -> Option<Key> {
    let path_text = super::wide(path);
    if path_text.len() > resource.len() {
        return None;
    }
    resource[..path_text.len()].copy_from_slice(&path_text);
    let mut index = 0;
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
    {
        let (link, persist) = super::super::discovery::shell_link().ok()?;
        resource[0] = 0;
        // Worker has initialized STA COM; interface owners release both pointers on every exit.
        // Buffers outlive these synchronous calls. No UI Runtime borrow or request lock is held.
        unsafe {
            let load: unsafe extern "system" fn(*mut std::ffi::c_void, *const u16, u32) -> i32 =
                std::mem::transmute(persist.method(5));
            if load(persist.0, path_text.as_ptr(), 0) < 0 {
                return None;
            }
            let location: unsafe extern "system" fn(
                *mut std::ffi::c_void,
                *mut u16,
                i32,
                *mut i32,
            ) -> i32 = std::mem::transmute(link.method(16));
            if location(
                link.0,
                resource.as_mut_ptr(),
                resource.len() as i32,
                &mut index,
            ) < 0
            {
                return None;
            }
            if resource[0] == 0 {
                let target: unsafe extern "system" fn(
                    *mut std::ffi::c_void,
                    *mut u16,
                    i32,
                    *mut std::ffi::c_void,
                    u32,
                ) -> i32 = std::mem::transmute(link.method(3));
                if target(
                    link.0,
                    resource.as_mut_ptr(),
                    resource.len() as i32,
                    null_mut(),
                    4,
                ) < 0
                {
                    return None;
                }
                index = 0;
            }
        }
    }
    // Expand into a bounded reusable buffer before creating a stable resource cache key.
    let length = unsafe {
        super::ExpandEnvironmentStringsW(
            resource.as_ptr(),
            expanded.as_mut_ptr(),
            expanded.len() as u32,
        )
    };
    if length == 0 || length as usize > expanded.len() {
        return None;
    }
    Some(Key {
        path: PathBuf::from(OsString::from_wide(&expanded[..length as usize - 1])),
        index,
    })
}
fn extract(key: &Key) -> Option<Arc<Icon>> {
    let path = super::wide(&key.path);
    let mut icon = null_mut();
    // Owned HICON is released by Icon::drop after worker cache and UI snapshots let it go.
    let count =
        unsafe { super::ExtractIconExW(path.as_ptr(), key.index, &mut icon, null_mut(), 1) };
    if icon.is_null() {
        None
    } else if count == 1 {
        Some(Arc::new(Icon(icon as usize)))
    } else {
        unsafe { super::DestroyIcon(icon) };
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(index: i32) -> Key {
        Key {
            path: PathBuf::from("synthetic-resource.dll"),
            index,
        }
    }
    #[test]
    fn aliases_share_negative_state_but_different_indices_do_not() {
        let mut loader = Loader::new();
        let first = loader.intern(key(-154));
        first.absent.set(true);
        loader.store(Path::new("first.lnk"), Some(first.clone()));
        let alias = loader.intern(key(-154));
        assert!(Rc::ptr_eq(&first, &alias));
        assert!(alias.absent.get());
        let different = loader.intern(key(-155));
        assert!(!Rc::ptr_eq(&first, &different));
        assert!(!different.absent.get());
        loader.clear();
        let refreshed = loader.intern(key(-154));
        assert!(!refreshed.absent.get());
    }
    #[test]
    fn repeated_negative_requests_do_not_extract_or_copy_more_icons() {
        let mut loader = Loader::new();
        let absent = loader.intern(key(0));
        absent.absent.set(true);
        loader.store(Path::new("absent.lnk"), Some(absent));
        let first = loader.load(Path::new("absent.lnk")).unwrap();
        for _ in 0..100 {
            let next = loader.load(Path::new("absent.lnk")).unwrap();
            assert!(Arc::ptr_eq(&first, &next));
        }
        assert_eq!(loader.stats().extracts, 0);
        assert_eq!(loader.stats().parses, 0);
        assert_eq!(loader.stats().generic_copies, 1);
    }
    #[test]
    fn metadata_has_count_and_byte_bounds() {
        let mut loader = Loader::new();
        for index in 0..2000 {
            let path = PathBuf::from(format!("{}-{index}.lnk", "x".repeat(300)));
            loader.store(&path, None);
            assert!(loader.stats().metadata_entries <= MAX_ENTRIES);
            assert!(loader.stats().metadata_bytes <= BYTE_BUDGET);
        }
        let enormous = PathBuf::from("y".repeat(BYTE_BUDGET + 1));
        loader.store(&enormous, None);
        assert!(loader.stats().metadata_bytes <= BYTE_BUDGET);
        assert!(!loader.entries.iter().any(|e| e.path == enormous));
    }
}
