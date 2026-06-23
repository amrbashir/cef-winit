// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! External message pump integration for CEF.
//!
//! CEF can request immediate work while the platform event loop is already
//! dispatching nested native events. Calling `CefDoMessageLoopWork` inline from
//! that callback can re-enter winit's event handler and panic. Each platform
//! module owns its wake/timer integration while exposing the same pump API.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub(crate) use linux::ExternalCefPump;
#[cfg(target_os = "macos")]
pub(crate) use macos::ExternalCefPump;
#[cfg(target_os = "windows")]
pub(crate) use windows::ExternalCefPump;
