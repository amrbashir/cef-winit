use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use cef::{BrowserHost, ImplBrowserHost};
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, define_class, extern_methods, msg_send,
    rc::Retained, runtime::Bool,
};
use objc2_app_kit::{NSApp, NSApplication, NSEvent, NSView};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

use super::NativeHandle;

#[derive(Default)]
struct CefWinitApplicationIvars {
    handling_send_event: std::cell::Cell<Bool>,
}

define_class!(
    #[unsafe(super(NSApplication))]
    #[ivars = CefWinitApplicationIvars]
    struct CefWinitApplication;

    impl CefWinitApplication {
        #[unsafe(method(sendEvent:))]
        unsafe fn send_event(&self, event: &NSEvent) {
            let was_handling = self.ivars().handling_send_event.get();
            self.ivars().handling_send_event.set(Bool::YES);
            let _: () = unsafe { msg_send![super(self), sendEvent: event] };
            self.ivars().handling_send_event.set(was_handling);
        }
    }

    unsafe impl CrAppControlProtocol for CefWinitApplication {
        #[unsafe(method(setHandlingSendEvent:))]
        unsafe fn set_handling_send_event(&self, handling_send_event: Bool) {
            self.ivars().handling_send_event.set(handling_send_event);
        }
    }

    unsafe impl CrAppProtocol for CefWinitApplication {
        #[unsafe(method(isHandlingSendEvent))]
        unsafe fn is_handling_send_event(&self) -> Bool {
            self.ivars().handling_send_event.get()
        }
    }

    unsafe impl CefAppProtocol for CefWinitApplication {}
);

impl CefWinitApplication {
    extern_methods! {
        #[unsafe(method(sharedApplication))]
        fn shared_application() -> Retained<Self>;
    }
}

pub(crate) fn setup_application() {
    let _ = CefWinitApplication::shared_application();
    let mtm = MainThreadMarker::new().expect("macOS application must start on the main thread");
    assert!(NSApp(mtm).isKindOfClass(CefWinitApplication::class()));
}

pub(crate) fn host_handle(window: &dyn Window) -> NativeHandle {
    let handle = window.window_handle().expect("failed to get window handle");
    match handle.as_raw() {
        RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr().cast(),
        other => panic!("expected AppKit window handle, got {other:?}"),
    }
}

pub(crate) fn set_browser_bounds(
    handle: NativeHandle,
    browser_host: &BrowserHost,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    browser_host.notify_move_or_resize_started();

    let view = handle.cast::<NSView>();
    let frame = NSRect::new(
        NSPoint::new(x as f64, y as f64),
        NSSize::new(width as f64, height as f64),
    );
    unsafe {
        (*view).setFrame(frame);
    }

    browser_host.was_resized();
}
