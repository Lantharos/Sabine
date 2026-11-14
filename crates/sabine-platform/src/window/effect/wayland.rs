use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread::JoinHandle;

use wayland_client::protocol::{
    wl_callback::WlCallback, wl_compositor::WlCompositor, wl_region::WlRegion,
    wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use winit::window::Window;

use crate::linux::kestrel_glass::{
    kestrel_glass_manager_v1::KestrelGlassManagerV1,
    kestrel_glass_v1::{self, KestrelGlassV1},
};
use crate::linux::wayland::Client;
use crate::{
    BackdropMaterial, BackdropPreference, BackdropRole, WindowBackdrop, WindowOptions,
    WindowRegion, WindowRegionRect,
};

/// Surface regions through `wl_compositor` and blur through
/// `ext_background_effect_v1`. Globals are bound the first time the window
/// asks for an effect or a region and kept for the window's life.
pub(super) struct Backend {
    window: Arc<dyn Window>,
    wake: Arc<dyn Fn() + Send + Sync>,
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
    glass: Option<Glass>,
}

/// Kestrel's word on the material of the window's glass, heard on a queue
/// of its own that a thread dispatches, since it changes whenever people
/// change their choice and nothing else wakes the window then.
struct Glass {
    manager: KestrelGlassManagerV1,
    glass: KestrelGlassV1,
    backdrop: WindowBackdrop,
    material: Arc<AtomicU8>,
    stop: Arc<AtomicBool>,
    connection: Connection,
    handle: QueueHandle<Material>,
    thread: Option<JoinHandle<()>>,
}

struct Material {
    material: Arc<AtomicU8>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Backend {
    pub(super) fn new(
        window: &Arc<dyn Window>,
        _options: &WindowOptions,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Self> {
        Some(Self {
            window: Arc::clone(window),
            wake,
            binding: Binding::Unbound,
        })
    }

    pub(super) fn material(&self) -> BackdropMaterial {
        match &self.binding {
            Binding::Bound(bound) => bound
                .blur
                .as_ref()
                .and_then(|blur| blur.glass.as_ref())
                .map_or(BackdropMaterial::Luca, Glass::material),
            _ => BackdropMaterial::Luca,
        }
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
            self.binding = Bound::bind(self.window.as_ref(), options, &self.wake)
                .map_or(Binding::Unavailable, |bound| {
                    Binding::Bound(Box::new(bound))
                });
        }
        if let Binding::Bound(bound) = &mut self.binding {
            bound.apply(options, width, height, transparent_holes);
        }
    }
}

impl Bound {
    fn bind(
        window: &dyn Window,
        options: &WindowOptions,
        wake: &Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Self> {
        let mut client = Client::connect(window).ok()?;
        let surface = client.surface(window).ok()?;
        let compositor = client
            .globals
            .bind::<WlCompositor, _, _>(&client.handle(), 1..=1, ())
            .ok()?;
        let blur = options
            .wants_background_effect()
            .then(|| Blur::bind(&mut client, &surface, options.backdrop, wake))
            .flatten();
        Some(Self {
            surface,
            compositor,
            blur,
            client,
        })
    }

    fn apply(
        &mut self,
        options: &WindowOptions,
        width: i32,
        height: i32,
        transparent_holes: &[WindowRegionRect],
    ) {
        if let Some(glass) = self.blur.as_mut().and_then(|blur| blur.glass.as_mut()) {
            glass.ask(options.backdrop);
        }
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
        if let Some(mut blur) = self.blur.take() {
            if let Some(glass) = blur.glass.take() {
                glass.close();
            }
            blur.effect.destroy();
            blur.manager.destroy();
        }
        self.client.flush();
    }
}

impl Blur {
    /// Binds the manager and waits for it to say whether it can blur.
    fn bind(
        client: &mut Client,
        surface: &WlSurface,
        backdrop: WindowBackdrop,
        wake: &Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Self> {
        let manager = client
            .globals
            .bind::<ExtBackgroundEffectManagerV1, _, _>(&client.handle(), 1..=1, ())
            .ok()?;
        if client.roundtrip().is_err() || !client.replies.blur_supported {
            manager.destroy();
            return None;
        }
        let effect = manager.get_background_effect(surface, &client.handle(), ());
        let glass = Glass::bind(client, surface, backdrop, wake);
        Some(Self {
            manager,
            effect,
            glass,
        })
    }
}

impl Glass {
    fn bind(
        client: &mut Client,
        surface: &WlSurface,
        backdrop: WindowBackdrop,
        wake: &Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Self> {
        let manager = client
            .globals
            .bind::<KestrelGlassManagerV1, _, _>(&client.handle(), 1..=1, ())
            .ok()?;
        let mut queue = client.connection.new_event_queue::<Material>();
        let handle = queue.handle();
        let material = Arc::new(AtomicU8::new(BackdropMaterial::Luca as u8));
        let mut state = Material {
            material: Arc::clone(&material),
            wake: Arc::clone(wake),
        };
        let glass = manager.get_glass(surface, &handle, ());
        send_backdrop(&glass, backdrop);
        let _ = queue.roundtrip(&mut state);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("sabine-glass".into())
            .spawn(move || {
                while !stopped.load(Ordering::Acquire)
                    && queue.blocking_dispatch(&mut state).is_ok()
                {}
            })
            .ok()?;
        Some(Self {
            manager,
            glass,
            backdrop,
            material,
            stop,
            connection: client.connection.clone(),
            handle,
            thread: Some(thread),
        })
    }

    fn material(&self) -> BackdropMaterial {
        match self.material.load(Ordering::Acquire) {
            value if value == BackdropMaterial::Maris as u8 => BackdropMaterial::Maris,
            _ => BackdropMaterial::Luca,
        }
    }

    fn ask(&mut self, backdrop: WindowBackdrop) {
        if backdrop != self.backdrop {
            self.backdrop = backdrop;
            send_backdrop(&self.glass, backdrop);
        }
    }

    /// Wakes the thread with a sync of its own queue so it sees it should
    /// stop.
    fn close(mut self) {
        self.stop.store(true, Ordering::Release);
        self.glass.destroy();
        self.manager.destroy();
        self.connection.display().sync(&self.handle, ());
        let _ = self.connection.flush();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn send_backdrop(glass: &KestrelGlassV1, backdrop: WindowBackdrop) {
    glass.set_role(match backdrop.role {
        BackdropRole::Window => kestrel_glass_v1::Role::Window,
        BackdropRole::Transient => kestrel_glass_v1::Role::Transient,
    });
    glass.set_preference(match backdrop.preference {
        BackdropPreference::User => kestrel_glass_v1::Preference::User,
        BackdropPreference::Luca => kestrel_glass_v1::Preference::Luca,
        BackdropPreference::Maris => kestrel_glass_v1::Preference::Maris,
    });
}

impl Dispatch<KestrelGlassV1, ()> for Material {
    fn event(
        state: &mut Self,
        _: &KestrelGlassV1,
        event: kestrel_glass_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let kestrel_glass_v1::Event::Material { material } = event;
        let material = match material.into_result() {
            Ok(kestrel_glass_v1::Material::Maris) => BackdropMaterial::Maris,
            _ => BackdropMaterial::Luca,
        };
        if state.material.swap(material as u8, Ordering::AcqRel) != material as u8 {
            (state.wake)();
        }
    }
}

delegate_noop!(Material: ignore WlCallback);
