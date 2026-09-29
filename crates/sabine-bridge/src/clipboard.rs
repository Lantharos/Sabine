//! Clipboard requests from pages, which the window host answers on Linux.

pub const COMMAND_PREFIX: &str = "sabine.clipboard.";
pub const READ_COMMAND: &str = "sabine.clipboard.read";
pub const WRITE_COMMAND: &str = "sabine.clipboard.write";
/// Evaluated in every frame by the browser host.
pub const SCRIPT: &str = include_str!("clipboard.js");
