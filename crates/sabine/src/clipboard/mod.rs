//! The desktop clipboard, which pages reach through the window host. On
//! Linux Chromium renders offscreen and holds no focused window of its own,
//! so the window host owns the clipboard and primary selection outright. On
//! Windows and macOS Chromium keeps its own copy and paste, and the window
//! host adds every other type to what pages can read and write.

mod content;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod pipe;
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(not(target_os = "linux"))]
mod worker;

pub(crate) use content::{ClipboardContent, Selection};
#[cfg(target_os = "linux")]
pub(crate) use linux::{DropEvent, SystemClipboard, Waker};
#[cfg(not(target_os = "linux"))]
pub(crate) use worker::SystemClipboard;

pub(crate) type Reply = Box<dyn FnOnce(Result<ClipboardContent, String>) + Send>;

#[cfg(not(target_os = "linux"))]
impl SystemClipboard {
    pub(crate) fn connect() -> Self {
        #[cfg(target_os = "macos")]
        return Self::start::<macos::GeneralPasteboard>();
        #[cfg(target_os = "windows")]
        return Self::start::<windows::WindowsClipboard>();
    }
}
