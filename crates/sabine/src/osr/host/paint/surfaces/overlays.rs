use std::{collections::HashMap, sync::Arc};

use crate::osr::protocol::{OsrSurface, POPUP_OVERLAY_ID};
use crate::render::ImageId;

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::SurfaceGeometry;

/// The browser surfaces drawn over the page: guests, the popups they open,
/// and the page's own popup, kept in the order they are drawn in. Guests
/// stack as the browser host stacks them, each popup right above its guest,
/// and the page's popup above everything.
#[derive(Default)]
pub(in crate::osr::host) struct Overlays {
    surfaces: HashMap<Arc<str>, SurfaceGeometry>,
    guest_order: Vec<Arc<str>>,
    drawing_order: Vec<Arc<str>>,
}

impl Overlays {
    pub(in crate::osr::host) fn get_key_value(
        &self,
        id: &str,
    ) -> Option<(&Arc<str>, &SurfaceGeometry)> {
        self.surfaces.get_key_value(id)
    }

    pub(in crate::osr::host) fn ids(&self) -> impl Iterator<Item = &Arc<str>> {
        self.surfaces.keys()
    }

    /// The overlays bottom first, with where each sits.
    pub(in crate::osr::host) fn in_drawing_order(
        &self,
    ) -> impl Iterator<Item = (&Arc<str>, SurfaceGeometry)> {
        self.drawing_order
            .iter()
            .filter_map(|id| Some((id, *self.surfaces.get(id)?)))
    }

    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn is_empty(&self) -> bool {
        self.surfaces.is_empty()
    }

    pub(in crate::osr::host) fn clear(&mut self) {
        self.surfaces.clear();
        self.drawing_order.clear();
    }

    pub(in crate::osr::host) fn stack(&mut self, guest_order: Vec<Arc<str>>) {
        self.guest_order = guest_order;
        self.restack();
    }

    fn place(&mut self, id: &Arc<str>, geometry: SurfaceGeometry) {
        if self.surfaces.insert(Arc::clone(id), geometry).is_none() {
            self.restack();
        }
    }

    fn remove_entry(&mut self, id: &str) -> Option<Arc<str>> {
        let (id, _) = self.surfaces.remove_entry(id)?;
        self.drawing_order.retain(|drawn| *drawn != id);
        Some(id)
    }

    fn restack(&mut self) {
        self.drawing_order.clear();
        for guest in &self.guest_order {
            let popup = format!("{guest}/popup");
            for id in [&**guest, popup.as_str()] {
                if let Some((id, _)) = self.surfaces.get_key_value(id) {
                    self.drawing_order.push(Arc::clone(id));
                }
            }
        }
        let mut unstacked = self
            .surfaces
            .keys()
            .filter(|id| &***id != POPUP_OVERLAY_ID && !self.drawing_order.contains(id))
            .cloned()
            .collect::<Vec<_>>();
        unstacked.sort();
        self.drawing_order.extend(unstacked);
        if let Some((popup, _)) = self.surfaces.get_key_value(POPUP_OVERLAY_ID) {
            self.drawing_order.push(Arc::clone(popup));
        }
    }
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn clear_overlay(&mut self, overlay_id: &str) {
        let Some(id) = self.overlays.remove_entry(overlay_id) else {
            return;
        };
        if let Some(renderer) = &mut self.renderer {
            renderer.remove_image(&ImageId::Overlay(id));
        }
    }

    /// The image a surface paints into, sharing the overlay's existing id.
    pub(in crate::osr::host) fn image_id(&self, surface: &OsrSurface) -> ImageId {
        let Some(overlay_id) = surface.overlay_id() else {
            return ImageId::Main;
        };
        ImageId::Overlay(
            self.overlays
                .get_key_value(overlay_id)
                .map_or_else(|| Arc::from(overlay_id), |(id, _)| Arc::clone(id)),
        )
    }

    pub(in crate::osr::host) fn place_overlay(
        &mut self,
        image: &ImageId,
        geometry: SurfaceGeometry,
    ) {
        if let ImageId::Overlay(id) = image {
            self.overlays.place(id, geometry);
        }
    }
}
