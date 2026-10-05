//! Visible-result icons: a lazy STA worker, latest request, bounded caches, blocking idle.
use super::{ffi::*, wide};
mod resource_cache;
pub use resource_cache::Stats;
use std::{
    cell::{Cell, RefCell},
    io,
    path::PathBuf,
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
// HICON is an opaque, process-wide USER handle, not a pointer to Rust memory. The worker
// owns extraction; Arc keeps it alive across UI drawing. The final owner destroys it once.
pub struct Icon(usize);
impl Icon {
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
    fn ExtractIconExW(
        path: *const u16,
        index: i32,
        large: *mut Handle,
        small: *mut Handle,
        count: u32,
    ) -> u32;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn LoadIconW(instance: Handle, name: *const u16) -> Handle;
    fn CopyIcon(icon: Handle) -> Handle;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn ExpandEnvironmentStringsW(source: *const u16, output: *mut u16, size: u32) -> u32;
}
fn generic() -> Option<Arc<Icon>> {
    // LoadIcon returns a shared system handle. Copy it so only our owned copy is destroyed.
    let icon = unsafe { CopyIcon(LoadIconW(null_mut(), 32512usize as *const u16)) };
    (!icon.is_null()).then(|| Arc::new(Icon(icon as usize)))
}
type Request = (u64, Vec<PathBuf>);
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
                    let (generation, paths) = request;
                    if work.invalidate.swap(false, Ordering::AcqRel) {
                        if let Some(loader) = &mut loader {
                            loader.clear();
                        }
                    }
                    let mut icons = Vec::with_capacity(paths.len());
                    for path in paths {
                        if work.stop.load(Ordering::Acquire)
                            || work.latest.lock().unwrap().is_some()
                        {
                            break;
                        }
                        icons.push(if path.as_os_str().is_empty() {
                            None
                        } else {
                            loader.as_mut().and_then(|loader| loader.load(&path))
                        });
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
    paths: RefCell<Vec<PathBuf>>,
    worker: RefCell<Option<Worker>>,
    pub requested: Cell<u64>,
    pub completed: Cell<u64>,
    pub stats: RefCell<Stats>,
}
impl Session {
    pub fn new(hwnd: Hwnd) -> Self {
        Self {
            hwnd,
            paths: RefCell::new(Vec::new()),
            worker: RefCell::new(None),
            requested: Cell::new(0),
            completed: Cell::new(0),
            stats: RefCell::new(Stats::default()),
        }
    }
    pub fn started(&self) -> bool {
        self.worker.borrow().is_some()
    }
    pub fn request(&self, paths: Vec<PathBuf>) -> io::Result<bool> {
        let invalidating = self
            .worker
            .borrow()
            .as_ref()
            .is_some_and(|w| w.shared.invalidate.load(Ordering::Acquire));
        if paths == *self.paths.borrow()
            && !invalidating
            && (self.requested.get() != 0 || paths.is_empty())
        {
            return Ok(false);
        }
        let mut worker = self.worker.borrow_mut();
        if worker.is_none() {
            *worker = Some(Worker::new(self.hwnd)?);
        }
        *self.paths.borrow_mut() = paths.clone();
        let worker = worker.as_ref().unwrap();
        let generation = self.requested.get() + 1;
        self.requested.set(generation);
        *worker.shared.latest.lock().unwrap() = Some((generation, paths));
        worker.shared.wake.notify_one();
        Ok(true)
    }
    pub fn invalidate(&self) {
        self.paths.borrow_mut().clear();
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
    fn empty_visible_results_do_not_start_worker_or_allocate_cache() {
        let session = Session::new(null_mut());
        assert!(!session.request(Vec::new()).unwrap());
        assert!(!session.started());
        assert_eq!(session.stats.borrow().metadata_bytes, 0);
        assert_eq!(session.requested.get(), session.completed.get());
    }
}
