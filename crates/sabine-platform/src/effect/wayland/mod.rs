mod protocol;

use std::{
    ffi::{c_uint, c_void},
    ptr,
    sync::Arc,
};

use winit::window::Window;

use crate::wayland_client::{
    DESTROY_FLAG, Globals, WL_COMPOSITOR_INTERFACE, WL_REGION_INTERFACE, WlDisplay, WlProxy,
    wayland_display, wayland_surface, wl_display_flush, wl_display_roundtrip,
    wl_proxy_add_listener, wl_proxy_destroy, wl_proxy_get_version, wl_proxy_marshal_flags,
};
use crate::{WindowOptions, WindowRegion, WindowRegionRect};
use protocol::*;

/// Surface regions through `wl_compositor` and blur through
/// `ext_background_effect_v1`. Globals are bound the first time the window
/// asks for an effect or a region and kept for the window's life.
pub(super) struct Backend {
    display: *mut WlDisplay,
    surface: *mut WlProxy,
    binding: Binding,
}

enum Binding {
    Unbound,
    Bound(Bound),
    Unavailable,
}

struct Bound {
    compositor: *mut WlProxy,
    blur: Option<Blur>,
}

struct Blur {
    manager: *mut WlProxy,
    effect: *mut WlProxy,
    _capabilities: Box<ManagerState>,
}

impl Backend {
    pub(super) fn new(window: &Arc<dyn Window>, _options: &WindowOptions) -> Option<Self> {
        Some(Self {
            display: wayland_display(window.as_ref())?,
            surface: wayland_surface(window.as_ref())?,
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
            self.binding = unsafe { Bound::bind(self.display, self.surface, options) }
                .map_or(Binding::Unavailable, Binding::Bound);
        }
        if let Binding::Bound(bound) = &self.binding {
            unsafe {
                bound.apply(
                    self.display,
                    self.surface,
                    options,
                    width,
                    height,
                    transparent_holes,
                )
            };
        }
    }
}

impl Bound {
    unsafe fn bind(
        display: *mut WlDisplay,
        surface: *mut WlProxy,
        options: &WindowOptions,
    ) -> Option<Self> {
        let globals = unsafe { Globals::discover(display) }?;
        let compositor = unsafe { globals.bind(&WL_COMPOSITOR_INTERFACE, 1) }?;
        let blur = if options.wants_background_effect() {
            unsafe { Blur::bind(display, surface, &globals) }
        } else {
            None
        };
        Some(Self { compositor, blur })
    }

    unsafe fn apply(
        &self,
        display: *mut WlDisplay,
        surface: *mut WlProxy,
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
                .unwrap_or_else(WindowRegion::adaptive_full);
            let region = unsafe { self.create_region(&region, width, height, transparent_holes) };
            unsafe {
                wl_proxy_marshal_flags(
                    blur.effect,
                    EFFECT_SET_BLUR_REGION,
                    ptr::null(),
                    1,
                    0,
                    region,
                );
                destroy_region(region);
            }
        }
        let opaque =
            options.regions.opaque.as_ref().map(|opaque| unsafe {
                self.create_region(opaque, width, height, transparent_holes)
            });
        unsafe { set_surface_region(surface, SURFACE_SET_OPAQUE_REGION, opaque) };
        let input = options
            .regions
            .input
            .as_ref()
            .map(|input| unsafe { self.create_region(input, width, height, &[]) });
        unsafe { set_surface_region(surface, SURFACE_SET_INPUT_REGION, input) };
        unsafe { flush_for_next_frame(display) };
    }

    unsafe fn create_region(
        &self,
        region: &WindowRegion,
        width: i32,
        height: i32,
        holes: &[WindowRegionRect],
    ) -> *mut WlProxy {
        let proxy = unsafe {
            wl_proxy_marshal_flags(
                self.compositor,
                COMPOSITOR_CREATE_REGION,
                &WL_REGION_INTERFACE,
                1,
                0,
                ptr::null::<c_void>(),
            )
        };
        let rects = region.resolved_rects(width, height);
        for (opcode, rect) in rects
            .iter()
            .map(|rect| (REGION_ADD, rect))
            .chain(holes.iter().map(|hole| (REGION_SUBTRACT, hole)))
        {
            unsafe {
                wl_proxy_marshal_flags(
                    proxy,
                    opcode,
                    ptr::null(),
                    1,
                    0,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                );
            }
        }
        proxy
    }
}

impl Drop for Bound {
    fn drop(&mut self) {
        unsafe { wl_proxy_destroy(self.compositor) };
    }
}

impl Blur {
    unsafe fn bind(
        display: *mut WlDisplay,
        surface: *mut WlProxy,
        globals: &Globals,
    ) -> Option<Self> {
        let manager = unsafe { globals.bind(&EXT_BACKGROUND_EFFECT_MANAGER_V1_INTERFACE, 1) }?;
        let mut capabilities = Box::<ManagerState>::default();
        let listening = unsafe {
            wl_proxy_add_listener(
                manager,
                &MANAGER_LISTENER as *const ManagerListener as *mut _,
                capabilities.as_mut() as *mut ManagerState as *mut c_void,
            )
        };
        if listening != 0
            || unsafe { wl_display_roundtrip(display) } < 0
            || !capabilities.supports_blur()
        {
            unsafe { destroy_manager(manager) };
            return None;
        }
        let effect = unsafe {
            wl_proxy_marshal_flags(
                manager,
                MANAGER_GET_BACKGROUND_EFFECT,
                &EXT_BACKGROUND_EFFECT_SURFACE_V1_INTERFACE,
                1,
                0,
                ptr::null::<c_void>(),
                surface,
            )
        };
        Some(Self {
            manager,
            effect,
            _capabilities: capabilities,
        })
    }
}

impl Drop for Blur {
    fn drop(&mut self) {
        unsafe {
            wl_proxy_marshal_flags(self.effect, EFFECT_DESTROY, ptr::null(), 1, DESTROY_FLAG);
            destroy_manager(self.manager);
        }
    }
}

unsafe fn destroy_manager(manager: *mut WlProxy) {
    unsafe { wl_proxy_marshal_flags(manager, MANAGER_DESTROY, ptr::null(), 1, DESTROY_FLAG) };
}

unsafe fn destroy_region(region: *mut WlProxy) {
    unsafe { wl_proxy_marshal_flags(region, REGION_DESTROY, ptr::null(), 1, DESTROY_FLAG) };
}

/// Sets or, without a region, clears one of the surface's regions.
unsafe fn set_surface_region(surface: *mut WlProxy, opcode: u32, region: Option<*mut WlProxy>) {
    unsafe {
        wl_proxy_marshal_flags(
            surface,
            opcode,
            ptr::null(),
            wl_proxy_get_version(surface),
            0,
            region.unwrap_or(ptr::null_mut()),
        );
        if let Some(region) = region {
            destroy_region(region);
        }
    }
}

/// Region changes are double-buffered surface state. The renderer owns the
/// surface's commits, and committing here without a buffer would race its
/// presentation and flash transparent windows, so they apply with its next
/// frame.
unsafe fn flush_for_next_frame(display: *mut WlDisplay) {
    unsafe { wl_display_flush(display) };
}

#[derive(Debug, Default)]
struct ManagerState {
    capabilities: u32,
}

impl ManagerState {
    fn supports_blur(&self) -> bool {
        self.capabilities & MANAGER_CAPABILITY_BLUR != 0
    }
}

unsafe extern "C" fn manager_capabilities(
    data: *mut c_void,
    _manager: *mut WlProxy,
    flags: c_uint,
) {
    let state = unsafe { &mut *(data.cast::<ManagerState>()) };
    state.capabilities = flags;
}

#[repr(C)]
struct ManagerListener {
    capabilities: unsafe extern "C" fn(*mut c_void, *mut WlProxy, c_uint),
}

static MANAGER_LISTENER: ManagerListener = ManagerListener {
    capabilities: manager_capabilities,
};

unsafe impl Sync for ManagerListener {}
