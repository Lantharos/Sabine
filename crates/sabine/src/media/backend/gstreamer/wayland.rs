use std::ffi::c_void;

use wayland_backend::client::{Backend, ObjectId};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_compositor::WlCompositor, wl_region::WlRegion, wl_registry::WlRegistry,
        wl_subcompositor::WlSubcompositor, wl_subsurface::WlSubsurface, wl_surface::WlSurface,
    },
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

pub(super) struct State;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: WlCompositor);
delegate_noop!(State: WlSubcompositor);
delegate_noop!(State: WlSubsurface);
delegate_noop!(State: WlRegion);
delegate_noop!(State: WpViewporter);
delegate_noop!(State: WpViewport);
delegate_noop!(State: ignore WlSurface);

/// A second view of the window's Wayland connection with its own event queue,
/// so video surfaces never touch the queue the window toolkit dispatches.
pub(super) struct MediaWayland {
    connection: Connection,
    queue: EventQueue<State>,
    compositor: WlCompositor,
    subcompositor: WlSubcompositor,
    viewporter: WpViewporter,
}

/// The half of a video surface the window thread positions.
pub(super) struct Placement {
    pub(super) subsurface: WlSubsurface,
}

/// The half of a video surface its player renders into.
pub(super) struct Target {
    pub(super) surface: WlSurface,
    pub(super) viewport: WpViewport,
}

impl MediaWayland {
    /// # Safety
    /// `display` must be the live `wl_display` of the window's connection.
    pub(super) unsafe fn connect(display: *mut c_void) -> Result<Self, String> {
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) =
            registry_queue_init::<State>(&connection).map_err(|error| error.to_string())?;
        let handle = queue.handle();
        let missing = |name: &str| format!("the compositor does not support {name}");
        Ok(Self {
            compositor: globals
                .bind(&handle, 4..=6, ())
                .map_err(|_| missing("wl_compositor"))?,
            subcompositor: globals
                .bind(&handle, 1..=1, ())
                .map_err(|_| missing("subsurfaces"))?,
            viewporter: globals
                .bind(&handle, 1..=1, ())
                .map_err(|_| missing("wp_viewporter"))?,
            connection,
            queue,
        })
    }

    /// Creates a video surface stacked directly beneath the window surface.
    ///
    /// # Safety
    /// `parent` must be the live `wl_surface` of the window on this connection.
    pub(super) unsafe fn create_surface(
        &self,
        parent: *mut c_void,
    ) -> Result<(Placement, Target), String> {
        let parent = unsafe { ObjectId::from_ptr(WlSurface::interface(), parent.cast()) }
            .and_then(|id| WlSurface::from_id(&self.connection, id))
            .map_err(|error| error.to_string())?;
        let handle = self.queue.handle();
        let surface = self.compositor.create_surface(&handle, ());
        let input = self.compositor.create_region(&handle, ());
        surface.set_input_region(Some(&input));
        input.destroy();
        let subsurface = self
            .subcompositor
            .get_subsurface(&surface, &parent, &handle, ());
        subsurface.place_below(&parent);
        subsurface.set_desync();
        let viewport = self.viewporter.get_viewport(&surface, &handle, ());
        Ok((Placement { subsurface }, Target { surface, viewport }))
    }

    pub(super) fn flush(&mut self) {
        let _ = self.queue.dispatch_pending(&mut State);
        let _ = self.connection.flush();
    }
}

impl Drop for Placement {
    fn drop(&mut self) {
        self.subsurface.destroy();
    }
}

impl Target {
    pub(super) fn surface_pointer(&self) -> *mut c_void {
        self.surface.id().as_ptr().cast()
    }

    pub(super) fn hide(&self) {
        self.surface.attach(None, 0, 0);
        self.surface.commit();
        self.flush();
    }

    fn flush(&self) {
        if let Some(backend) = self.surface.backend().upgrade() {
            let _ = backend.flush();
        }
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        self.viewport.destroy();
        self.surface.destroy();
        self.flush();
    }
}
