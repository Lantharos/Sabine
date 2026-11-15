use std::{collections::HashMap, sync::Arc};

use super::broker::{SharedSurface, SurfaceRegistry};

/// The surfaces the socket reader has resolved, so a frame naming a known
/// surface never waits on the registry. Surfaces the browser host retired
/// are dropped as soon as the registry counts a retirement, and their ids
/// kept until the window takes them.
pub(crate) struct SurfaceCache {
    registry: Arc<SurfaceRegistry>,
    surfaces: HashMap<u64, SharedSurface>,
    retirements: u64,
    retired: Vec<u64>,
}

impl SurfaceCache {
    pub(crate) fn new(registry: Arc<SurfaceRegistry>) -> Self {
        Self {
            registry,
            surfaces: HashMap::new(),
            retirements: 0,
            retired: Vec::new(),
        }
    }

    pub(crate) fn surface(&mut self, surface_id: u64) -> Option<SharedSurface> {
        self.forget_retired();
        if let Some(surface) = self.surfaces.get(&surface_id) {
            return Some(surface.clone());
        }
        let surface = self.registry.surface(surface_id)?;
        self.surfaces.insert(surface_id, surface.clone());
        Some(surface)
    }

    /// The ids of the surfaces retired since the last call.
    pub(crate) fn take_retired(&mut self) -> Vec<u64> {
        self.forget_retired();
        std::mem::take(&mut self.retired)
    }

    fn forget_retired(&mut self) {
        let retirements = self.registry.retirements();
        if retirements != self.retirements {
            self.retirements = retirements;
            self.retired
                .extend(self.registry.forget_retired(&mut self.surfaces));
        }
    }
}
