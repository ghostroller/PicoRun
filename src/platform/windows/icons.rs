//! Visible-result icons: a lazy STA worker, latest request, bounded caches, blocking idle.
use super::{ffi::*, shell_file_path, wide};
use crate::model::LaunchTarget;
mod dimensions;
mod packaged;
mod resource_cache;
pub use resource_cache::Stats;
use std::{
    cell::{Cell, RefCell},
    io,
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
};

pub const READY: u32 = 0x8005;
pub const CAPACITY: usize = 48;
pub const MAX_SIZE: u16 = 256;
pub(super) struct Extracted {
    pub icon: Arc<Icon>,
    pub cacheable: bool,
}
// HICON is an opaque, process-wide USER handle, not a pointer to Rust memory. The worker
// owns extraction; Arc keeps it alive across UI drawing. The final owner destroys it once.
pub struct Icon(usize);
impl Icon {
    pub fn dimensions(&self) -> Option<(i32, i32)> {
        dimensions::get(self.handle())
    }
    pub fn handle(&self) -> Handle {
        self.0 as Handle
    }
}
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            DestroyIcon(self.handle());
        }
    }
}
#[link(name = "shell32")]
unsafe extern "system" {
    fn SHDefExtractIconW(
        path: *const u16,
        index: i32,
        flags: u32,
        large: *mut Handle,
        small: *mut Handle,
        size: u32,
    ) -> i32;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn LoadIconW(instance: Handle, name: *const u16) -> Handle;
    fn CopyImage(image: Handle, kind: u32, width: i32, height: i32, flags: u32) -> Handle;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn ExpandEnvironmentStringsW(source: *const u16, output: *mut u16, size: u32) -> u32;
}
fn generic(size: u16) -> Option<Arc<Icon>> {
    // LoadIcon gives a shared system icon; CopyImage gives an owned, size-specific copy.
    let icon = unsafe {
        CopyImage(
            LoadIconW(null_mut(), 32512usize as *const u16),
            1,
            i32::from(size),
            i32::from(size),
            0x4000,
        )
    };
    (!icon.is_null()).then(|| Arc::new(Icon(icon as usize)))
}
pub(super) fn extract_file(path: &[u16], index: i32, size: u16) -> Option<Arc<Icon>> {
    let mut handle = null_mut();
    // The NUL-terminated caller buffer stays alive through the synchronous call.
    // A nonnull output is owned even on failure; Arc/Drop releases it on all paths.
    let code = unsafe {
        SHDefExtractIconW(
            path.as_ptr(),
            index,
            0,
            &mut handle,
            null_mut(),
            u32::from(size),
        )
    };
    let icon = (!handle.is_null()).then(|| Arc::new(Icon(handle as usize)));
    if code == 0 {
        icon
    } else {
        None
    }
}
type Request = (u64, u16, Vec<LaunchTarget>);
struct Shared {
    latest: Mutex<Option<Request>>,
    wake: Condvar,
    stop: AtomicBool,
    invalidate: AtomicBool,
}
impl Shared {
    fn stop(&self) {
        // Publish under the same lock as the worker's predicate check and wait.
        // Otherwise notification can fall between that check and entering wait,
        // leaving an idle worker asleep while Drop waits forever in join().
        let _next = self.latest.lock().unwrap();
        self.stop.store(true, Ordering::Release);
        self.wake.notify_one();
    }
}
pub struct Completed {
    generation: u64,
    pub icons: Vec<Option<Arc<Icon>>>,
    stats: Stats,
    pub failed: bool,
}
struct Worker {
    shared: Arc<Shared>,
    rx: Option<Receiver<Completed>>,
    join: Option<JoinHandle<()>>,
}
impl Worker {
    fn new(hwnd: Hwnd) -> io::Result<Self> {
        let shared = Arc::new(Shared {
            latest: Mutex::new(None),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
            invalidate: AtomicBool::new(false),
        });
        let work = Arc::clone(&shared);
        let (tx, rx) = mpsc::sync_channel(1);
        let owner = hwnd as usize;
        let join = thread::Builder::new()
            .name("picorun-icons".into())
            .spawn(move || {
                let initialized = unsafe { CoInitializeEx(null_mut(), 2) >= 0 };
                // No COM/UI borrow or mutex survives native parsing/extraction. Loader and its Rc
                // resource descriptors stay on this thread; only Arc<Icon> and counters cross it.
                let mut loader = initialized.then(resource_cache::Loader::new);
                loop {
                    let request = {
                        let mut next = work.latest.lock().unwrap();
                        while next.is_none() && !work.stop.load(Ordering::Acquire) {
                            next = work.wake.wait(next).unwrap();
                        }
                        if work.stop.load(Ordering::Acquire) {
                            break;
                        }
                        next.take().unwrap()
                    };
                    let (generation, size, targets) = request;
                    let invalidate = work.invalidate.swap(false, Ordering::AcqRel);
                    if let Some(loader) = &mut loader {
                        loader.prepare(size, invalidate);
                    }
                    let mut icons = Vec::with_capacity(targets.len());
                    for target in targets {
                        if work.stop.load(Ordering::Acquire)
                            || work.latest.lock().unwrap().is_some()
                        {
                            break;
                        }
                        icons.push(loader.as_mut().and_then(|loader| loader.load(&target)));
                    }
                    let result = Completed {
                        generation,
                        icons,
                        stats: loader.as_ref().map_or_else(Stats::default, |l| l.stats()),
                        failed: !initialized,
                    };
                    if tx.send(result).is_err() {
                        break;
                    }
                    unsafe {
                        PostMessageW(owner as Hwnd, READY, 0, 0);
                    }
                }
                drop(loader); // Release cached icons and COM objects before uninitializing STA.
                if initialized {
                    unsafe {
                        CoUninitialize();
                    }
                }
            })?;
        Ok(Self {
            shared,
            rx: Some(rx),
            join: Some(join),
        })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.rx.take(); // Unblock a full completion channel before joining.
        self.shared.stop();
        // Caller holds no Runtime borrow. An in-flight native extraction must finish first.
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
pub struct Session {
    hwnd: Hwnd,
    targets: RefCell<Vec<LaunchTarget>>,
    size: Cell<u16>,
    worker: RefCell<Option<Worker>>,
    pub requested: Cell<u64>,
    pub completed: Cell<u64>,
    pub stats: RefCell<Stats>,
}
impl Session {
    pub fn new(hwnd: Hwnd) -> Self {
        Self {
            hwnd,
            targets: RefCell::new(Vec::new()),
            size: Cell::new(0),
            worker: RefCell::new(None),
            requested: Cell::new(0),
            completed: Cell::new(0),
            stats: RefCell::new(Stats::default()),
        }
    }
    pub fn started(&self) -> bool {
        self.worker.borrow().is_some()
    }
    pub fn request(&self, targets: Vec<LaunchTarget>, size: u16) -> io::Result<bool> {
        if !(1..=MAX_SIZE).contains(&size) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid icon pixel size",
            ));
        }
        if targets.is_empty() && self.worker.borrow().is_none() {
            // F5/DPI invalidation with no visible results must remain dormant.
            self.size.set(size);
            self.targets.borrow_mut().clear();
            self.completed.set(self.requested.get());
            return Ok(false);
        }
        let invalidating = self
            .worker
            .borrow()
            .as_ref()
            .is_some_and(|w| w.shared.invalidate.load(Ordering::Acquire));
        if targets == *self.targets.borrow()
            && size == self.size.get()
            && !invalidating
            && (self.requested.get() != 0 || targets.is_empty())
        {
            return Ok(false);
        }
        let mut worker = self.worker.borrow_mut();
        if worker.is_none() {
            *worker = Some(Worker::new(self.hwnd)?);
        }
        self.size.set(size);
        *self.targets.borrow_mut() = targets.clone();
        let worker = worker.as_ref().unwrap();
        let generation = self.requested.get() + 1;
        self.requested.set(generation);
        *worker.shared.latest.lock().unwrap() = Some((generation, size, targets));
        worker.shared.wake.notify_one();
        Ok(true)
    }
    pub fn invalidate(&self) {
        self.targets.borrow_mut().clear();
        self.requested.set(self.requested.get() + 1);
        if let Some(worker) = self.worker.borrow().as_ref() {
            worker.shared.invalidate.store(true, Ordering::Release);
        }
    }
    pub fn receive(&self) -> Option<Completed> {
        let worker = self.worker.borrow();
        let rx = worker.as_ref()?.rx.as_ref()?;
        let mut latest = None;
        while let Ok(result) = rx.try_recv() {
            *self.stats.borrow_mut() = result.stats.clone();
            if result.generation == self.requested.get() {
                self.completed.set(result.generation);
                latest = Some(result);
            }
        }
        latest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn stopping_cannot_notify_between_the_idle_check_and_wait() {
        let shared = Arc::new(Shared {
            latest: Mutex::new(None),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
            invalidate: AtomicBool::new(false),
        });
        let mut next = shared.latest.lock().unwrap();
        // Pause the worker after the idle predicate was checked but before wait.
        assert!(next.is_none() && !shared.stop.load(Ordering::Acquire));
        let (started_tx, started_rx) = mpsc::channel();
        let (stopped_tx, stopped_rx) = mpsc::channel();
        let stopping = Arc::clone(&shared);
        let stopper = thread::spawn(move || {
            started_tx.send(()).unwrap();
            stopping.stop();
            stopped_tx.send(()).unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let stopped_before_wait = stopped_rx.recv_timeout(Duration::from_millis(100)).is_ok();

        // Enter wait even if a faulty stopper published early: that is the exact
        // lost-notification interval. Bound waits so a regression fails, not hangs.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut timed_out = false;
        loop {
            let (guard, result) = shared
                .wake
                .wait_timeout(next, deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            next = guard;
            if result.timed_out() {
                timed_out = true;
                break;
            }
            if shared.stop.load(Ordering::Acquire) {
                break;
            }
        }
        let stopped = shared.stop.load(Ordering::Acquire);
        drop(next);
        stopper.join().unwrap();
        assert!(
            !stopped_before_wait,
            "stop must wait for the predicate lock"
        );
        assert!(stopped && !timed_out, "the idle worker must wake on stop");
    }

    #[test]
    fn generic_owns_a_copy_at_the_requested_physical_size() {
        for size in [20, 25, 40, 80] {
            let icon = generic(size).unwrap();
            assert_eq!(icon.dimensions(), Some((i32::from(size), i32::from(size))));
        }
    }
    #[test]
    fn invalid_size_is_rejected_before_starting_worker() {
        let session = Session::new(null_mut());
        for size in [0, MAX_SIZE + 1] {
            assert_eq!(
                session.request(Vec::new(), size).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        assert!(!session.started());
    }
    #[test]
    fn empty_results_after_refresh_or_size_change_keep_session_dormant() {
        let session = Session::new(null_mut());
        assert!(!session.request(Vec::new(), 25).unwrap());
        session.invalidate();
        assert!(!session.request(Vec::new(), 40).unwrap());
        assert!(!session.started());
        assert_eq!(session.requested.get(), session.completed.get());
        assert_eq!(session.stats.borrow().metadata_entries, 0);
    }
    #[test]
    fn empty_visible_results_do_not_start_worker_or_allocate_cache() {
        let session = Session::new(null_mut());
        assert!(!session.request(Vec::new(), 25).unwrap());
        assert!(!session.started());
        assert_eq!(session.stats.borrow().metadata_bytes, 0);
        assert_eq!(session.requested.get(), session.completed.get());
    }
}
