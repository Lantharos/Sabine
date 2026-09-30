use std::{fmt, sync::Arc};

use crate::WindowId;

pub(crate) const VISIBILITY_LINE: &str = "SABINE_WINDOW_VISIBILITY";

/// A window's visibility, reported to [`SabineWindow::on_visibility_changed`].
///
/// [`SabineWindow::on_visibility_changed`]: crate::SabineWindow::on_visibility_changed
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowVisibility {
    pub window: WindowId,
    /// People can see the window: it is shown and the desktop has not reported
    /// it out of sight, as it does for minimized or fully covered windows
    /// where it can.
    pub visible: bool,
    /// The page runs at the window's background frame rate.
    pub suspended: bool,
}

#[derive(Clone)]
pub(crate) struct VisibilityListener(Arc<dyn Fn(WindowVisibility) + Send + Sync>);

impl VisibilityListener {
    pub(crate) fn new(listener: impl Fn(WindowVisibility) + Send + Sync + 'static) -> Self {
        Self(Arc::new(listener))
    }

    /// Delivers a visibility line written by the window host.
    pub(crate) fn deliver(&self, window: WindowId, line: &str) {
        let mut parts = line.split('\t').skip(1);
        let (Some(visible), Some(suspended)) = (parts.next(), parts.next()) else {
            return;
        };
        (self.0)(WindowVisibility {
            window,
            visible: visible == "1",
            suspended: suspended == "1",
        });
    }
}

impl fmt::Debug for VisibilityListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VisibilityListener")
    }
}
