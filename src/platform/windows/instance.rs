//! Startup handshake only. The primary keeps the existing installer mutex and blocks
//! in its ordinary message loop after startup; there is no polling thread or timer.
use super::{ffi::*, wide};
use crate::i18n::Text;
use std::{io, ptr::null};

const MUTEX: &str = "Local\\PicoRun.Native.v1";
const READY: &str = "Local\\PicoRun.Native.v1.Ready";
const WAIT_OBJECT_0: u32 = 0;
const WAIT_ABANDONED_0: u32 = 0x80;
const WAIT_TIMEOUT: u32 = 0x102;

pub(super) struct Instance {
    mutex: Handle,
    ready: Handle,
    primary: bool,
}
impl Instance {
    pub fn acquire() -> io::Result<Self> {
        Self::named(MUTEX, READY)
    }
    fn named(mutex: &str, ready: &str) -> io::Result<Self> {
        // Initial ownership lets a secondary wait for either readiness or primary exit.
        // Existing mutexes ignore initial ownership, so a secondary does not acquire it here.
        let name = wide(mutex);
        let handle = unsafe { CreateMutexW(null(), 1, name.as_ptr()) };
        let code = unsafe { GetLastError() };
        if handle.is_null() {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let primary = code != 183;
        let mut instance = Self {
            mutex: handle,
            ready: std::ptr::null_mut(),
            primary,
        };
        instance.ready = unsafe { CreateEventW(null(), 1, 0, wide(ready).as_ptr()) };
        if instance.ready.is_null() {
            return Err(io::Error::last_os_error());
        }
        // A secondary can create the event during the small mutex/event creation gap.
        // Reset only from the primary, which owns the mutex, before doing any startup work.
        if primary && unsafe { ResetEvent(instance.ready) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(instance)
    }
    pub fn is_primary(&self) -> bool {
        self.primary
    }
    pub fn signal_ready(&self) -> io::Result<()> {
        if unsafe { SetEvent(self.ready) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    pub fn wait_ready(&self, timeout: u32) -> io::Result<()> {
        // Both handles stay owned for the entire wait. The primary releases the mutex
        // on every ordinary return; a crashed primary also wakes us through abandonment.
        let handles = [self.ready, self.mutex];
        let result = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, timeout) };
        match result {
            WAIT_OBJECT_0 => Ok(()),
            value if value == WAIT_OBJECT_0 + 1 || value == WAIT_ABANDONED_0 + 1 => {
                // Waiting on an available/abandoned mutex acquires it on this thread.
                unsafe { ReleaseMutex(self.mutex) };
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    Text::InstanceExited,
                ))
            }
            WAIT_TIMEOUT => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                Text::InstanceStarting,
            )),
            _ => Err(io::Error::last_os_error()),
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        // Created and dropped on the same UI thread. No Rust UI borrow spans these calls.
        unsafe {
            if self.primary {
                ReleaseMutex(self.mutex);
            }
            if !self.ready.is_null() {
                CloseHandle(self.ready);
            }
            CloseHandle(self.mutex);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread, time::Duration};

    fn names(suffix: &str) -> (String, String) {
        let mutex = format!(
            "Local\\PicoRun.Verification.Instance.{}.{suffix}",
            std::process::id()
        );
        let ready = format!("{mutex}.Ready");
        (mutex, ready)
    }
    #[test]
    fn secondary_waits_until_primary_finishes_initializing() {
        let (mutex, ready) = names("ready");
        let primary = Instance::named(&mutex, &ready).unwrap();
        assert!(primary.is_primary());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let secondary = thread::spawn(move || {
            let instance = Instance::named(&mutex, &ready).unwrap();
            assert!(!instance.is_primary());
            entered_tx.send(()).unwrap();
            done_tx.send(instance.wait_ready(2000)).unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_millis(30)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        primary.signal_ready().unwrap();
        done_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        secondary.join().unwrap();
    }
    #[test]
    fn secondary_observes_timeout_and_primary_exit_without_a_window() {
        let (mutex, ready) = names("exit");
        let primary = Instance::named(&mutex, &ready).unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let secondary = thread::spawn(move || {
            let instance = Instance::named(&mutex, &ready).unwrap();
            assert_eq!(
                instance.wait_ready(0).unwrap_err().kind(),
                io::ErrorKind::TimedOut
            );
            entered_tx.send(()).unwrap();
            done_tx.send(instance.wait_ready(2000)).unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(primary);
        assert_eq!(
            done_rx
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
        secondary.join().unwrap();
    }
}
