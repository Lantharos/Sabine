//! The textures the browser host shares with this window, which accelerated
//! frames name by resource id.

use crate::osr::protocol::OsrAccelFrame;

#[cfg(windows)]
#[derive(Default)]
pub(super) struct SharedResources(crate::osr::accel::SharedHandles);

#[cfg(windows)]
impl SharedResources {
    pub(super) fn attach(&mut self, frame: &mut OsrAccelFrame, meta: &[u8]) {
        frame.resource = self
            .0
            .resolve(frame.resource_id, super::accel::announced_texture(meta));
    }

    pub(super) fn retire(&mut self, resource_ids: Vec<u64>) {
        self.0.retire(resource_ids);
    }
}

#[cfg(target_os = "linux")]
#[derive(Default)]
pub(super) struct SharedResources(crate::osr::accel::Dmabufs);

#[cfg(target_os = "linux")]
impl SharedResources {
    pub(super) fn attach(&mut self, frame: &mut OsrAccelFrame, _meta: &[u8]) {
        frame.resource = self.0.resolve(frame.resource_id);
    }

    pub(super) fn retire(&mut self, resource_ids: Vec<u64>) {
        self.0.retire(resource_ids);
    }

    pub(super) fn announce(&mut self, resource_id: u64, dmabuf: crate::osr::accel::Dmabuf) {
        self.0.announce(resource_id, dmabuf);
    }
}

#[cfg(target_os = "macos")]
pub(super) struct SharedResources(crate::osr::accel::SurfaceCache);

#[cfg(target_os = "macos")]
impl SharedResources {
    pub(super) fn new(registry: std::sync::Arc<crate::osr::accel::SurfaceRegistry>) -> Self {
        Self(crate::osr::accel::SurfaceCache::new(registry))
    }

    pub(super) fn attach(&mut self, frame: &mut OsrAccelFrame, _meta: &[u8]) {
        frame.resource = self.0.surface(frame.resource_id);
    }
}
