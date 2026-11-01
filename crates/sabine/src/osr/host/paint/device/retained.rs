use crate::render::{ImageId, RendererError};

use crate::osr::host::native::OsrNativeHost;

impl OsrNativeHost {
    /// Keeps the shown surfaces on the CPU before the renderer holding them
    /// goes away, so the window can show them again right away.
    pub(in crate::osr::host) fn retain_frames(&mut self) {
        let Some(renderer) = &self.renderer else {
            return;
        };
        let images = self
            .main_surface
            .map(|_| ImageId::Main)
            .into_iter()
            .chain(self.overlays.ids().cloned().map(ImageId::Overlay));
        self.retained_frames = images
            .filter_map(|id| Some((id.clone(), renderer.read_bgra_image(&id)?)))
            .collect();
        if !self.retained_frames.contains_key(&ImageId::Main) {
            self.main_surface = None;
        }
    }

    pub(in crate::osr::host) fn restore_retained_frames(&mut self) -> Result<(), RendererError> {
        let frames = std::mem::take(&mut self.retained_frames);
        let Some(renderer) = self.renderer.as_mut() else {
            return Ok(());
        };
        for (id, frame) in &frames {
            renderer.write_bgra_rects(id, frame.size(), &[frame.whole()])?;
        }
        Ok(())
    }
}
