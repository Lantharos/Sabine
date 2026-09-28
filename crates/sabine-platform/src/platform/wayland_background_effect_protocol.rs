use std::ffi::c_int;
use std::ptr;

use crate::wayland_client::{WL_REGION_INTERFACE, WL_SURFACE_INTERFACE, WlInterface, WlMessage};

unsafe impl Sync for InterfaceTypes {}

pub(super) const COMPOSITOR_CREATE_REGION: u32 = 1;
pub(super) const SURFACE_SET_OPAQUE_REGION: u32 = 4;
pub(super) const SURFACE_SET_INPUT_REGION: u32 = 5;
pub(super) const REGION_DESTROY: u32 = 0;
pub(super) const REGION_ADD: u32 = 1;
pub(super) const REGION_SUBTRACT: u32 = 2;
pub(super) const MANAGER_DESTROY: u32 = 0;
pub(super) const MANAGER_GET_BACKGROUND_EFFECT: u32 = 1;
pub(super) const MANAGER_CAPABILITY_BLUR: u32 = 1;
pub(super) const EFFECT_DESTROY: u32 = 0;
pub(super) const EFFECT_SET_BLUR_REGION: u32 = 1;

#[repr(C)]
struct InterfaceTypes {
    manager_get_background_effect: [*const WlInterface; 2],
    effect_set_blur_region: [*const WlInterface; 1],
}

static INTERFACE_TYPES: InterfaceTypes = InterfaceTypes {
    manager_get_background_effect: [&EXT_BACKGROUND_EFFECT_SURFACE_V1_INTERFACE, unsafe {
        &WL_SURFACE_INTERFACE
    }],
    effect_set_blur_region: [unsafe { &WL_REGION_INTERFACE }],
};

static MANAGER_METHODS: [WlMessage; 2] = [
    WlMessage {
        name: c"destroy".as_ptr(),
        signature: c"".as_ptr(),
        types: ptr::null(),
    },
    WlMessage {
        name: c"get_background_effect".as_ptr(),
        signature: c"no".as_ptr(),
        types: INTERFACE_TYPES.manager_get_background_effect.as_ptr(),
    },
];

static MANAGER_EVENTS: [WlMessage; 1] = [WlMessage {
    name: c"capabilities".as_ptr(),
    signature: c"u".as_ptr(),
    types: ptr::null(),
}];

static EFFECT_METHODS: [WlMessage; 2] = [
    WlMessage {
        name: c"destroy".as_ptr(),
        signature: c"".as_ptr(),
        types: ptr::null(),
    },
    WlMessage {
        name: c"set_blur_region".as_ptr(),
        signature: c"?o".as_ptr(),
        types: INTERFACE_TYPES.effect_set_blur_region.as_ptr(),
    },
];

pub(super) static EXT_BACKGROUND_EFFECT_MANAGER_V1_INTERFACE: WlInterface = WlInterface {
    name: c"ext_background_effect_manager_v1".as_ptr(),
    version: 1,
    method_count: MANAGER_METHODS.len() as c_int,
    methods: MANAGER_METHODS.as_ptr(),
    event_count: MANAGER_EVENTS.len() as c_int,
    events: MANAGER_EVENTS.as_ptr(),
};

pub(super) static EXT_BACKGROUND_EFFECT_SURFACE_V1_INTERFACE: WlInterface = WlInterface {
    name: c"ext_background_effect_surface_v1".as_ptr(),
    version: 1,
    method_count: EFFECT_METHODS.len() as c_int,
    methods: EFFECT_METHODS.as_ptr(),
    event_count: 0,
    events: ptr::null(),
};
