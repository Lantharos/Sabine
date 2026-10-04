//! Guests are secondary Chromium views hosted inside a window. Pages control
//! them with `sabine.guest.*` commands, which the browser host answers itself;
//! Rust drives them with [`GuestHostControl`].

mod create;
mod host_control;

pub use create::{GuestBounds, GuestCreateOptions, GuestPopupPolicy};
pub use host_control::{GuestDownloadAction, GuestHostControl};

pub(crate) const COMMANDS: [&str; 16] = [
    "sabine.guest.create",
    "sabine.guest.destroy",
    "sabine.guest.navigate",
    "sabine.guest.setBounds",
    "sabine.guest.setVisible",
    "sabine.guest.setCovered",
    "sabine.guest.capturePreview",
    "sabine.guest.focus",
    "sabine.guest.reload",
    "sabine.guest.goBack",
    "sabine.guest.goForward",
    "sabine.guest.list",
    "sabine.guest.get",
    "sabine.guest.setZoom",
    "sabine.guest.executeJavaScript",
    "sabine.guest.downloadAction",
];
