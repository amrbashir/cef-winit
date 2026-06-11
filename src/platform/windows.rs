use cef::{BrowserHost, ImplBrowserHost};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos},
};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

use super::NativeHandle;

pub(crate) fn host_handle(window: &dyn Window) -> NativeHandle {
    let handle = window.window_handle().expect("failed to get window handle");
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get() as usize as NativeHandle,
        other => panic!("expected Win32 window handle, got {other:?}"),
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
    unsafe {
        let _ = SetWindowPos(
            HWND(handle),
            None,
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    browser_host.was_resized();
}
