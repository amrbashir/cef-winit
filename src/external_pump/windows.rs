use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, HWND_MESSAGE, KillTimer,
    PostMessageW, RegisterClassExW, SetTimer, SetWindowLongPtrW, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_TIMER, WM_USER, WNDCLASSEXW,
};
use windows::core::w;

const MSG_HAVE_WORK: u32 = WM_USER + 1;
const TIMER_ID: usize = 1;
const TIMER_DELAY_PLACEHOLDER: i64 = i32::MAX as i64;
const MAX_TIMER_DELAY_MS: i64 = 1000 / 30;

#[derive(Clone)]
pub(crate) struct ExternalCefPump {
    inner: Arc<WindowsPumpInner>,
}

impl ExternalCefPump {
    pub(crate) fn new() -> Self {
        Self {
            inner: WindowsPumpInner::new(),
        }
    }

    pub(crate) fn schedule_message_pump_work(&self, delay_ms: i64) {
        unsafe {
            let _ = PostMessageW(
                Some(self.inner.hwnd()),
                MSG_HAVE_WORK,
                WPARAM(0),
                LPARAM(delay_ms as isize),
            );
        }
    }

    pub(crate) fn do_message_loop_work(&self) {
        self.inner.do_work();
    }

    pub(crate) fn poll_platform_events(&self) {}
}

struct WindowsPumpInner {
    hwnd: AtomicIsize,
    timer_pending: AtomicBool,
    is_active: AtomicBool,
    reentrancy_detected: AtomicBool,
}

unsafe impl Send for WindowsPumpInner {}
unsafe impl Sync for WindowsPumpInner {}

impl WindowsPumpInner {
    fn new() -> Arc<Self> {
        let inner = Arc::new(Self {
            hwnd: AtomicIsize::new(0),
            timer_pending: AtomicBool::new(false),
            is_active: AtomicBool::new(false),
            reentrancy_detected: AtomicBool::new(false),
        });

        unsafe {
            let instance = GetModuleHandleW(None).expect("failed to get module handle");
            let class_name = w!("CEFMainTargetHWND");
            let window_class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wnd_proc),
                hInstance: HINSTANCE(instance.0),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassExW(&window_class);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                None,
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
            .expect("failed to create CEF message-only window");

            inner.hwnd.store(hwnd.0 as isize, Ordering::SeqCst);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Arc::as_ptr(&inner) as isize);
        }

        inner
    }

    fn hwnd(&self) -> HWND {
        HWND(self.hwnd.load(Ordering::SeqCst) as *mut c_void)
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
            unsafe {
                let _ = PostMessageW(Some(self.hwnd()), MSG_HAVE_WORK, WPARAM(0), LPARAM(0));
            }
        } else if !self.is_timer_pending() {
            unsafe {
                let _ = PostMessageW(
                    Some(self.hwnd()),
                    MSG_HAVE_WORK,
                    WPARAM(0),
                    LPARAM(TIMER_DELAY_PLACEHOLDER as isize),
                );
            }
        }
    }

    fn perform_message_loop_work(&self) -> bool {
        if self.is_active.swap(true, Ordering::SeqCst) {
            self.reentrancy_detected.store(true, Ordering::SeqCst);
            return false;
        }

        self.reentrancy_detected.store(false, Ordering::SeqCst);
        cef::do_message_loop_work();
        self.is_active.store(false, Ordering::SeqCst);

        self.reentrancy_detected.load(Ordering::SeqCst)
    }

    fn set_timer(&self, delay_ms: i64) {
        debug_assert!(delay_ms > 0);
        debug_assert!(!self.is_timer_pending());
        self.timer_pending.store(true, Ordering::SeqCst);
        unsafe {
            let _ = SetTimer(Some(self.hwnd()), TIMER_ID, delay_ms as u32, None);
        }
    }

    fn kill_timer(&self) {
        if self.timer_pending.swap(false, Ordering::SeqCst) {
            unsafe {
                let _ = KillTimer(Some(self.hwnd()), TIMER_ID);
            }
        }
    }

    fn is_timer_pending(&self) -> bool {
        self.timer_pending.load(Ordering::SeqCst)
    }
}

impl Drop for WindowsPumpInner {
    fn drop(&mut self) {
        self.kill_timer();
        let hwnd = self.hwnd();
        if !hwnd.is_invalid() {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_TIMER || msg == MSG_HAVE_WORK {
        let ptr = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
        } as *const WindowsPumpInner;
        if !ptr.is_null() {
            let pump = unsafe { &*ptr };
            if msg == MSG_HAVE_WORK {
                pump.on_schedule_work(lparam.0 as i64);
            } else {
                pump.on_timer_timeout();
            }
        }
    }

    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
