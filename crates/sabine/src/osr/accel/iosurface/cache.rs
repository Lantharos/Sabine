use std::{collections::HashMap, sync::Arc};

use super::broker::{SharedSurface, SurfaceRegistry};

/// The surfaces the socket reader has resolved, so a frame naming a known
/// surface never waits on the registry. Surfaces the browser host retired
/// are dropped as soon as the registry counts a retirement.
pub(crate) struct SurfaceCache {
    registry: Arc<SurfaceRegistry>,
    surfaces: HashMap<u64, SharedSurface>,
    retirements: u64,
}

impl SurfaceCache {
    pub(crate) fn new(registry: Arc<SurfaceRegistry>) -> Self {
        Self {
            registry,
            surfaces: HashMap::new(),
            retirements: 0,
        }
    }

    pub(crate) fn surface(&mut self, surface_id: u64) -> Option<SharedSurface> {
        let retirements = self.registry.retirements();
        if retirements != self.retirements {
            self.retirements = retirements;
            self.registry.forget_retired(&mut self.surfaces);
        }
        if let Some(surface) = self.surfaces.get(&surface_id) {
            return Some(surface.clone());
        }
        let surface = self.registry.surface(surface_id)?;
        self.surfaces.insert(surface_id, surface.clone());
        Some(surface)
    }
}
