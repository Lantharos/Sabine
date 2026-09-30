use crate::osr::host::native::OsrNativeHost;
#[cfg(target_os = "linux")]
use crate::osr::host::types::{LoadingKind, NativeLoading};

impl OsrNativeHost {
    pub(in crate::osr::host) fn accelerated_paint(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            !self.software_paint
                && self
                    .renderer
                    .as_ref()
                    .is_none_or(crate::render::GpuRenderer::imports_dmabuf)
        }
        #[cfg(windows)]
        {
            self.renderer.is_some()
        }
        #[cfg(target_os = "macos")]
        {
            true
        }
    }

    /// Reopen the page with software painting once accelerated frames turn
    /// out not to work on this machine.
    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn paint_in_software(&mut self, reason: &str) {
        if self.software_paint {
            return;
        }
        self.software_paint = true;
        sabine_runtime::report_error(
            "gpu",
            format!("Painting without shared GPU frames: {reason}"),
        );
        if !self.accelerated_launch || (self.socket.is_none() && !self.awaiting_connection) {
            return;
        }
        self.send_control("close\n");
        self.drop_connection();
        self.awaiting_connection = false;
        self.connection_deadline = None;
        self.main_surface = None;
        self.overlays.clear();
        if let Some(renderer) = &mut self.renderer {
            renderer.clear_images();
        }
        if self.config.visible {
            self.loading = Some(NativeLoading::new(LoadingKind::Opening));
        }
        self.launch_child();
    }
}
