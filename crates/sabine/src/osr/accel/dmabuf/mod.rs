mod import;

pub(super) use import::import_dmabuf;

use std::{collections::HashMap, os::fd::OwnedFd, sync::Arc};

/// A Sabine-owned dma-buf the browser host copies frames into. The host hands
/// each one over once, with its layout, before frames reference it by id.
#[derive(Debug)]
pub(crate) struct Dmabuf {
    pub(crate) fd: OwnedFd,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) modifier: u64,
    pub(crate) stride: u32,
    pub(crate) offset: u32,
}

/// Dma-bufs announced by the browser host, keyed by resource id.
#[derive(Default)]
pub(crate) struct Dmabufs(HashMap<u64, Arc<Dmabuf>>);

impl Dmabufs {
    pub(crate) fn announce(&mut self, resource_id: u64, dmabuf: Dmabuf) {
        self.0.insert(resource_id, Arc::new(dmabuf));
    }

    pub(crate) fn resolve(&self, resource_id: u64) -> Option<Arc<Dmabuf>> {
        self.0.get(&resource_id).cloned()
    }

    pub(crate) fn retire(&mut self, resource_ids: impl IntoIterator<Item = u64>) {
        for resource_id in resource_ids {
            self.0.remove(&resource_id);
        }
    }
}
