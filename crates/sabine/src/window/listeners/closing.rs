use std::{fmt, sync::Arc};

use crate::WindowId;

/// Someone asked the window to close, and the app decides whether it does.
pub(crate) const CLOSE_REQUESTED_LINE: &str = "SABINE_CLOSE_REQUESTED";

/// The app's answer to someone asking a window to close.
#[derive(Clone)]
pub(crate) struct CloseRequestListener(Arc<dyn Fn(WindowId) -> bool + Send + Sync>);

impl CloseRequestListener {
    pub(crate) fn new(listener: impl Fn(WindowId) -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(listener))
    }

    pub(crate) fn allows(&self, window: WindowId) -> bool {
        (self.0)(window)
    }
}

impl fmt::Debug for CloseRequestListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CloseRequestListener")
    }
}

/// The app's listener for a window having closed.
#[derive(Clone)]
pub(crate) struct ClosedListener(Arc<dyn Fn(WindowId) + Send + Sync>);

impl ClosedListener {
    pub(crate) fn new(listener: impl Fn(WindowId) + Send + Sync + 'static) -> Self {
        Self(Arc::new(listener))
    }

    pub(crate) fn deliver(&self, window: WindowId) {
        (self.0)(window);
    }
}

impl fmt::Debug for ClosedListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClosedListener")
    }
}
