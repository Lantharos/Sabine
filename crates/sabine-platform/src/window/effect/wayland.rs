use std::sync::Arc;

use wayland_client::protocol::{
    wl_compositor::WlCompositor, wl_region::WlRegion, wl_surface::WlSurface,
};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use winit::window::Window;

use crate::linux::wayland::Client;
use crate::{WindowOptions, WindowRegion, WindowRegionRect};

/// Surface regions through `wl_compositor` and blur through
/// `ext_background_effect_v1`. Globals are bound the first time the window
/// asks for an effect or a region and kept for the window's life.
pub(super) struct Backend {
    window: Arc<dyn Window>,
    binding: Binding,
}

enum Binding {
    Unbound,
    Bound(Box<Bound>),
    Unavailable,
}

struct Bound {
    surface: WlSurface,
    compositor: WlCompositor,
    blur: Option<Blur>,
    client: Client,
}

struct Blur {
    manager: ExtBackgroundEffectManagerV1,
    effect: ExtBackgroundEffectSurfaceV1,
}

impl Backend {
    pub(super) fn new(window: &Arc<dyn Window>, _options: &WindowOptions) -> Option<Self> {
        Some(Self {
            window: Arc::clone(window),
            binding: Binding::Unbound,
        })
    }

    pub(super) fn update(
        &mut self,
        options: &WindowOptions,
        width: i32,
        height: i32,
        transparent_holes: &[WindowRegionRect],
    ) {
        if matches!(self.binding, Binding::Unbound) {
            if !options.wants_background_effect() && options.regions.is_empty() {
                return;
            }
            self.binding = Bound::bind(self.window.as_ref(), options)
                .map_or(Binding::Unavailable, |bound| {
                    Binding::Bound(Box::new(bound))
                });
        }
        if let Binding::Bound(bound) = &self.binding {
            bound.apply(options, width, height, transparent_holes);
        }
    }
}

impl Bound {
    fn bind(window: &dyn Window, options: &WindowOptions) -> Option<Self> {
        let mut client = Client::connect(window).ok()?;
        let surface = client.surface(window).ok()?;
        let compositor = client
            .globals
            .bind::<WlCompositor, _, _>(&client.handle(), 1..=1, ())
            .ok()?;
        let blur = options
            .wants_background_effect()
            .then(|| Blur::bind(&mut client, &surface))
            .flatten();
        Some(Self {
            surface,
            compositor,
            blur,
            client,
        })
    }

    fn apply(
        &self,
        options: &WindowOptions,
        width: i32,
        height: i32,
        transparent_holes: &[WindowRegionRect],
    ) {
        if let Some(blur) = &self.blur {
            let region = options
                .regions
                .blur
                .clone()
                .unwrap_or_else(WindowRegion::full);
            let region = self.create_region(&region, width, height, transparent_holes);
            blur.effect.set_blur_region(Some(&region));
            region.destroy();
        }
        let opaque = options
            .regions
            .opaque
            .as_ref()
            .map(|opaque| self.create_region(opaque, width, height, transparent_holes));
        self.surface.set_opaque_region(opaque.as_ref());
        let input = options
            .regions
            .input
            .as_ref()
            .map(|input| self.create_region(input, width, height, &[]));
        self.surface.set_input_region(input.as_ref());
        for region in opaque.into_iter().chain(input) {
            region.destroy();
        }
        self.flush_for_next_frame();
    }

    fn create_region(
        &self,
        region: &WindowRegion,
        width: i32,
        height: i32,
        holes: &[WindowRegionRect],
    ) -> WlRegion {
        let created = self.compositor.create_region(&self.client.handle(), ());
        for rect in region.resolved_rects(width, height) {
            created.add(rect.x, rect.y, rect.width, rect.height);
        }
        for hole in holes {
            created.subtract(hole.x, hole.y, hole.width, hole.height);
        }
        created
    }

    /// Region changes are double-buffered surface state. The renderer owns the
    /// surface's commits, and committing here without a buffer would race its
    /// presentation and flash transparent windows, so they apply with its next
    /// frame.
    fn flush_for_next_frame(&self) {
        self.client.flush();
    }
}

impl Drop for Bound {
    fn drop(&mut self) {
        if let Some(blur) = self.blur.take() {
            blur.effect.destroy();
            blur.manager.destroy();
        }
        self.client.flush();
    }
}

impl Blur {
    /// Binds the manager and waits for it to say whether it can blur.
    fn bind(client: &mut Client, surface: &WlSurface) -> Option<Self> {
        let manager = client
            .globals
            .bind::<ExtBackgroundEffectManagerV1, _, _>(&client.handle(), 1..=1, ())
            .ok()?;
        if client.roundtrip().is_err() || !client.replies.blur_supported {
            manager.destroy();
            return None;
        }
        let effect = manager.get_background_effect(surface, &client.handle(), ());
        Some(Self { manager, effect })
    }
}
