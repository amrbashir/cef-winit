use std::{
    io,
    mem::size_of,
    os::fd::RawFd,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const TIMER_DELAY_PLACEHOLDER: i64 = i32::MAX as i64;
const MAX_TIMER_DELAY_MS: i64 = 1000 / 30;

#[derive(Clone)]
pub(crate) struct ExternalCefPump {
    inner: Arc<LinuxPumpInner>,
}

impl ExternalCefPump {
    pub(crate) fn new() -> Self {
        Self {
            inner: LinuxPumpInner::new(),
        }
    }

    pub(crate) fn schedule_message_pump_work(&self, delay_ms: i64) {
        self.inner.write_delay(delay_ms);
    }

    pub(crate) fn do_message_loop_work(&self) {
        self.inner.do_work();
    }

    pub(crate) fn poll_platform_events(&self) {
        self.inner.read_scheduled_work();
        if self.inner.timer_expired() {
            self.inner.on_timer_timeout();
        }
    }
}

struct LinuxPumpInner {
    read_fd: RawFd,
    write_fd: RawFd,
    delayed_work_time: Mutex<Option<Instant>>,
    is_active: AtomicBool,
    reentrancy_detected: AtomicBool,
}

unsafe impl Send for LinuxPumpInner {}
unsafe impl Sync for LinuxPumpInner {}

impl LinuxPumpInner {
    fn new() -> Arc<Self> {
        let mut fds = [0; 2];
        let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
        if ret != 0 {
            panic!(
                "failed to create CEF wakeup pipe: {}",
                io::Error::last_os_error()
            );
        }

        set_nonblocking(fds[0]);

        Arc::new(Self {
            read_fd: fds[0],
            write_fd: fds[1],
            delayed_work_time: Mutex::new(None),
            is_active: AtomicBool::new(false),
            reentrancy_detected: AtomicBool::new(false),
        })
    }

    fn write_delay(&self, delay_ms: i64) {
        let bytes = delay_ms.to_ne_bytes();
        let mut written = 0;
        while written < bytes.len() {
            let ret = unsafe {
                libc::write(
                    self.write_fd,
                    bytes[written..].as_ptr().cast(),
                    bytes.len() - written,
                )
            };

            if ret < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                panic!("failed to write to CEF wakeup pipe: {error}");
            }

            written += ret as usize;
        }
    }

    fn read_scheduled_work(&self) {
        let mut delays = [0_i64; 2];
        let ret = loop {
            let ret = unsafe {
                libc::read(
                    self.read_fd,
                    delays.as_mut_ptr().cast(),
                    delays.len() * size_of::<i64>(),
                )
            };
            if ret < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                if error.kind() == io::ErrorKind::WouldBlock {
                    return;
                }
            }
            break ret;
        };

        if ret < size_of::<i64>() as isize {
            return;
        }

        self.on_schedule_work(delays[0]);
        if ret >= (size_of::<i64>() * 2) as isize {
            self.on_schedule_work(delays[1]);
        }
    }

    fn on_schedule_work(&self, delay_ms: i64) {
        if delay_ms == TIMER_DELAY_PLACEHOLDER && self.is_timer_pending() {
            return;
        }

        self.kill_timer();

        if delay_ms <= 0 {
            self.do_work();
        } else {
            self.set_timer(delay_ms.min(MAX_TIMER_DELAY_MS));
        }
    }

    fn on_timer_timeout(&self) {
        self.kill_timer();
        self.do_work();
    }

    fn do_work(&self) {
        if self.perform_message_loop_work() {
            self.write_delay(0);
        } else if !self.is_timer_pending() {
            self.write_delay(TIMER_DELAY_PLACEHOLDER);
        }
    }

    fn perform_message_loop_work(&self) -> bool {
        if self.is_active.swap(true, Ordering::SeqCst) {
            self.reentrancy_detected.store(true, Ordering::SeqCst);
            return false;
        }

        self.reentrancy_detected.store(false, Ordering::SeqCst);
        drain_glib();
        cef::do_message_loop_work();
        drain_glib();
        self.is_active.store(false, Ordering::SeqCst);

        self.reentrancy_detected.load(Ordering::SeqCst)
    }

    fn set_timer(&self, delay_ms: i64) {
        debug_assert!(delay_ms > 0);
        let mut delayed_work_time = self.delayed_work_time.lock().unwrap();
        *delayed_work_time = Some(Instant::now() + Duration::from_millis(delay_ms as u64));
    }

    fn kill_timer(&self) {
        let mut delayed_work_time = self.delayed_work_time.lock().unwrap();
        *delayed_work_time = None;
    }

    fn is_timer_pending(&self) -> bool {
        self.delayed_work_time
            .lock()
            .unwrap()
            .map(|deadline| Instant::now() < deadline)
            .unwrap_or(false)
    }

    fn timer_expired(&self) -> bool {
        self.delayed_work_time
            .lock()
            .unwrap()
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
    }
}

impl Drop for LinuxPumpInner {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.read_fd);
            libc::close(self.write_fd);
        }
    }
}

fn set_nonblocking(fd: RawFd) {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        panic!(
            "failed to read CEF wakeup pipe flags: {}",
            io::Error::last_os_error()
        );
    }

    let ret = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if ret < 0 {
        panic!(
            "failed to set CEF wakeup pipe nonblocking: {}",
            io::Error::last_os_error()
        );
    }
}

fn drain_glib() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}
