//! Sabine's own windows: setup progress, update and uninstall prompts, and
//! notices about what went wrong, drawn in the desktop's light or dark style.

mod choice;
mod diagnostics;
mod identity;
mod paint;
mod palette;
mod progress;
mod text;

pub use choice::{Prompt, confirm, show_notice};
pub use diagnostics::log_directory;
pub use identity::with_sabine_icon;
pub use progress::{ProgressOutcome, ProgressState, finish, run_progress_window, set_progress};
pub use winit::window::WindowAttributes;

/// Gives a window its icon: the app's own when the window speaks for the app,
/// or Sabine's when it speaks for Sabine.
pub type IconHook = fn(WindowAttributes) -> WindowAttributes;
