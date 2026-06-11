use cef::{BrowserHost, ImplBrowserHost, get_xdisplay};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};
use x11_dl::xlib::{Display, Xlib};

use super::NativeHandle;

pub(crate) fn host_handle(window: &dyn Window) -> NativeHandle {
    let window_handle = window.window_handle().expect("failed to get window handle");
    match window_handle.as_raw() {
        RawWindowHandle::Xlib(handle) => handle.window as usize as NativeHandle,
        RawWindowHandle::Wayland(_) => {
            panic!(
                "CEF native child embedding requires X11/XWayland; run with WINIT_UNIX_BACKEND=x11"
            )
        }
        other => panic!("expected Xlib window handle, got {other:?}"),
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

    let display = {
        let display = get_xdisplay();
        if display.is_null() {
            panic!("CEF X11 display is not available");
        } else {
            display.cast::<Display>()
        }
    };
    let xlib = Xlib::open().expect("failed to open Xlib");

    unsafe {
        (xlib.XMoveResizeWindow)(display, handle as usize as _, x, y, width as _, height as _);
        (xlib.XSync)(display, 0);
        (xlib.XFlush)(display);
    }

    browser_host.was_resized();
}
