use std::ffi::c_void;

use cef::BrowserHost;
use winit::window::Window;

pub(crate) type NativeHandle = *mut c_void;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "windows")]
use windows as imp;

pub(crate) fn host_handle(window: &dyn Window) -> NativeHandle {
    imp::host_handle(window)
}

pub(crate) fn set_browser_bounds(
    handle: NativeHandle,
    browser_host: &BrowserHost,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    imp::set_browser_bounds(handle, browser_host, x, y, width, height);
}

#[cfg(target_os = "macos")]
pub(crate) fn setup_application() {
    imp::setup_application();
}
