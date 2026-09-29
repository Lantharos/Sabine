use std::{
    ffi::{c_uint, c_void},
    ptr,
};

use super::WaylandEffect;
use super::wayland_background_effect_protocol::*;
use crate::wayland_client::{
    DESTROY_FLAG, Globals, WL_COMPOSITOR_INTERFACE, WL_REGION_INTERFACE, WlDisplay, WlProxy,
    wl_display_flush, wl_display_roundtrip, wl_proxy_add_listener, wl_proxy_destroy,
    wl_proxy_get_version, wl_proxy_marshal_flags,
};
use crate::{WindowBackgroundEffect, WindowOptions, WindowRegion, WindowRegionRect};

#[cfg(target_os = "linux")]
pub(super) struct ExtBackgroundEffect;

#[cfg(target_os = "linux")]
impl ExtBackgroundEffect {
    pub(super) unsafe fn bind(
        display: *mut WlDisplay,
        surface: *mut WlProxy,
        _effect: WindowBackgroundEffect,
        width: i32,
        height: i32,
        wants_blur: bool,
        options: &WindowOptions,
    ) -> Option<WaylandEffect> {
        if display.is_null() || surface.is_null() {
            return None;
        }

        let globals = unsafe { Globals::discover(display) }?;
        let Some(compositor) = (unsafe { globals.bind(&WL_COMPOSITOR_INTERFACE, 1) }) else {
            debug("failed to bind wl_compositor");
            return None;
        };

        let mut manager_state = Box::<ManagerState>::default();
        let mut manager = ptr::null_mut();
        let mut effect_proxy = ptr::null_mut();
        if wants_blur {
            if !globals.contains(&EXT_BACKGROUND_EFFECT_MANAGER_V1_INTERFACE) {
                debug("ext_background_effect_manager_v1 was not advertised");
                unsafe { wl_proxy_destroy(compositor) };
                return None;
            }
            let Some(bound) =
                (unsafe { globals.bind(&EXT_BACKGROUND_EFFECT_MANAGER_V1_INTERFACE, 1) })
            else {
                debug("failed to bind ext_background_effect_manager_v1");
                unsafe { wl_proxy_destroy(compositor) };
                return None;
            };
            manager = bound;
            debug("bound ext_background_effect_manager_v1");

            let add_manager_listener = unsafe {
                wl_proxy_add_listener(
                    manager,
                    &MANAGER_LISTENER as *const ManagerListener as *mut _,
                    manager_state.as_mut() as *mut ManagerState as *mut c_void,
                )
            };
            if add_manager_listener != 0 {
                debug("failed to add ext_background_effect_manager_v1 listener");
                unsafe { wl_proxy_destroy(compositor) };
                unsafe { wl_proxy_destroy(manager) };
                return None;
            }
            unsafe {
                wl_display_roundtrip(display);
            }
            if !manager_state.supports_blur() {
                debug("ext_background_effect_manager_v1 does not advertise blur capability");
                unsafe { wl_proxy_destroy(compositor) };
                unsafe { wl_proxy_destroy(manager) };
                return None;
            }

            effect_proxy = unsafe {
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
            if effect_proxy.is_null() {
                debug("failed to create ext_background_effect_surface_v1");
                unsafe { wl_proxy_destroy(compositor) };
                unsafe { wl_proxy_destroy(manager) };
                return None;
            }
            debug("created ext_background_effect_surface_v1");
        }
        drop(globals);

        let effect = WaylandEffect {
            display,
            surface,
            effect: effect_proxy,
            manager: manager.cast(),
            compositor: compositor.cast(),
            _manager_state: manager_state,
        };
        unsafe { effect.apply_surface_regions(options, width, height, &[]) };
        debug("applied ext_background_effect_surface_v1 regions");
        Some(effect)
    }
}

#[cfg(target_os = "linux")]
impl WaylandEffect {
    pub(super) unsafe fn apply_surface_regions(
        &self,
        options: &WindowOptions,
        width: i32,
        height: i32,
        transparent_holes: &[WindowRegionRect],
    ) -> bool {
        let (display, surface, compositor, effect_proxy) =
            (self.display, self.surface, self.compositor, self.effect);
        if !effect_proxy.is_null() {
            let blur = options
                .regions
                .blur
                .clone()
                .unwrap_or_else(WindowRegion::adaptive_full);
            let Some(region) =
                (unsafe { create_region(compositor, &blur, width, height, transparent_holes) })
            else {
                debug("failed to create blur wl_region");
                return false;
            };
            unsafe {
                wl_proxy_marshal_flags(
                    effect_proxy,
                    EFFECT_SET_BLUR_REGION,
                    ptr::null(),
                    1,
                    0,
                    region,
                );
                wl_proxy_marshal_flags(region, REGION_DESTROY, ptr::null(), 1, DESTROY_FLAG);
            }
        }

        let opaque = options.regions.opaque.as_ref().and_then(|opaque| unsafe {
            create_region(compositor, opaque, width, height, transparent_holes)
        });
        unsafe { set_surface_region(surface, SURFACE_SET_OPAQUE_REGION, opaque) };
        let input = options
            .regions
            .input
            .as_ref()
            .and_then(|input| unsafe { create_region(compositor, input, width, height, &[]) });
        unsafe { set_surface_region(surface, SURFACE_SET_INPUT_REGION, input) };

        // Do not wl_surface_commit here. This surface is owned by wgpu; a commit
        // without attaching a buffer races presentation and flashes transparent
        // windows (especially after interactive move / focus regain).
        unsafe {
            wl_display_flush(display);
        }
        true
    }
}

/// Sets or, without a region, clears one of the surface's regions.
#[cfg(target_os = "linux")]
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
            wl_proxy_marshal_flags(region, REGION_DESTROY, ptr::null(), 1, DESTROY_FLAG);
        }
    }
}

#[cfg(target_os = "linux")]
unsafe fn create_region(
    compositor: *mut WlProxy,
    region: &WindowRegion,
    width: i32,
    height: i32,
    holes: &[WindowRegionRect],
) -> Option<*mut WlProxy> {
    let proxy = unsafe {
        wl_proxy_marshal_flags(
            compositor,
            COMPOSITOR_CREATE_REGION,
            &WL_REGION_INTERFACE,
            1,
            0,
            ptr::null::<c_void>(),
        )
    };
    if proxy.is_null() {
        return None;
    }

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
    Some(proxy)
}

#[cfg(target_os = "linux")]
fn debug(message: &str) {
    if std::env::var_os("SABINE_WAYLAND_EFFECT_DEBUG").is_some() {
        eprintln!("sabine wayland background effect: {message}");
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
pub struct ManagerState {
    capabilities: u32,
}

#[cfg(target_os = "linux")]
impl ManagerState {
    fn supports_blur(&self) -> bool {
        self.capabilities & MANAGER_CAPABILITY_BLUR != 0
    }
}

#[cfg(target_os = "linux")]
unsafe extern "C" fn manager_capabilities(
    data: *mut c_void,
    _manager: *mut WlProxy,
    flags: c_uint,
) {
    if data.is_null() {
        return;
    }
    let state = unsafe { &mut *(data.cast::<ManagerState>()) };
    state.capabilities = flags;
}

#[cfg(target_os = "linux")]
#[repr(C)]
struct ManagerListener {
    capabilities: unsafe extern "C" fn(*mut c_void, *mut WlProxy, c_uint),
}

#[cfg(target_os = "linux")]
static MANAGER_LISTENER: ManagerListener = ManagerListener {
    capabilities: manager_capabilities,
};

#[cfg(target_os = "linux")]
unsafe impl Sync for ManagerListener {}
