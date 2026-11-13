use crate::osr::host::native::OsrNativeHost;

impl OsrNativeHost {
    pub(in crate::osr::host) fn accelerated_paint(&self) -> bool {
        !self.software_paint && self.renderer_imports_frames()
    }

    #[cfg(target_os = "linux")]
    fn renderer_imports_frames(&self) -> bool {
        self.renderer
            .as_ref()
            .is_none_or(crate::render::GpuRenderer::imports_dmabuf)
    }

    #[cfg(windows)]
    fn renderer_imports_frames(&self) -> bool {
        self.renderer.is_some()
    }

    #[cfg(target_os = "macos")]
    fn renderer_imports_frames(&self) -> bool {
        true
    }

    /// Reopen the page with software painting once accelerated frames turn
    /// out not to work on this machine.
    pub(in crate::osr::host) fn paint_in_software(&mut self, reason: &str) {
        if self.software_paint {
            return;
        }
        self.software_paint = true;
        sabine_runtime::report_error(
            "gpu",
            format!("Painting without shared GPU frames: {reason}"),
        );
        if self.accelerated_launch {
            self.relaunch_browser();
        }
    }
}
