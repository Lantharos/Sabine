use std::ffi::c_void;
use std::ptr;

use crate::wayland_client::{
    DESTROY_FLAG, Globals, WL_SEAT_INTERFACE, WL_SURFACE_INTERFACE, WlDisplay, WlInterface,
    WlMessage, WlProxy, wl_display_flush, wl_proxy_destroy, wl_proxy_marshal_flags,
};

const MANAGER_DESTROY: u32 = 0;
const MANAGER_INHIBIT_SHORTCUTS: u32 = 1;
const INHIBITOR_DESTROY: u32 = 0;

#[repr(C)]
struct InterfaceTypes {
    inhibit_shortcuts: [*const WlInterface; 3],
}

unsafe impl Sync for InterfaceTypes {}

static INTERFACE_TYPES: InterfaceTypes = InterfaceTypes {
    inhibit_shortcuts: [
        &INHIBITOR_INTERFACE,
        unsafe { &WL_SURFACE_INTERFACE },
        unsafe { &WL_SEAT_INTERFACE },
    ],
};

static MANAGER_METHODS: [WlMessage; 2] = [
    WlMessage {
        name: c"destroy".as_ptr(),
        signature: c"".as_ptr(),
        types: ptr::null(),
    },
    WlMessage {
        name: c"inhibit_shortcuts".as_ptr(),
        signature: c"noo".as_ptr(),
        types: INTERFACE_TYPES.inhibit_shortcuts.as_ptr(),
    },
];

static INHIBITOR_METHODS: [WlMessage; 1] = [WlMessage {
    name: c"destroy".as_ptr(),
    signature: c"".as_ptr(),
    types: ptr::null(),
}];

static INHIBITOR_EVENTS: [WlMessage; 2] = [
    WlMessage {
        name: c"active".as_ptr(),
        signature: c"".as_ptr(),
        types: ptr::null(),
    },
    WlMessage {
        name: c"inactive".as_ptr(),
        signature: c"".as_ptr(),
        types: ptr::null(),
    },
];

static MANAGER_INTERFACE: WlInterface = WlInterface {
    name: c"zwp_keyboard_shortcuts_inhibit_manager_v1".as_ptr(),
    version: 1,
    method_count: MANAGER_METHODS.len() as i32,
    methods: MANAGER_METHODS.as_ptr(),
    event_count: 0,
    events: ptr::null(),
};

static INHIBITOR_INTERFACE: WlInterface = WlInterface {
    name: c"zwp_keyboard_shortcuts_inhibitor_v1".as_ptr(),
    version: 1,
    method_count: INHIBITOR_METHODS.len() as i32,
    methods: INHIBITOR_METHODS.as_ptr(),
    event_count: INHIBITOR_EVENTS.len() as i32,
    events: INHIBITOR_EVENTS.as_ptr(),
};

/// A `zwp_keyboard_shortcuts_inhibitor_v1` on the window's surface. The
/// compositor applies it whenever the surface has keyboard focus.
pub(super) struct WaylandInhibitor {
    display: *mut WlDisplay,
    manager: *mut WlProxy,
    seat: *mut WlProxy,
    inhibitor: *mut WlProxy,
}

impl WaylandInhibitor {
    pub(super) unsafe fn new(
        display: *mut WlDisplay,
        surface: *mut WlProxy,
    ) -> Result<Self, String> {
        let globals = unsafe { Globals::discover(display) }
            .ok_or("Could not read the Wayland compositor globals")?;
        let manager = unsafe { globals.bind(&MANAGER_INTERFACE, 1) }
            .ok_or("This desktop does not let apps inhibit its shortcuts")?;
        let Some(seat) = (unsafe { globals.bind(&WL_SEAT_INTERFACE, 1) }) else {
            unsafe {
                wl_proxy_marshal_flags(manager, MANAGER_DESTROY, ptr::null(), 1, DESTROY_FLAG)
            };
            return Err("This desktop has no keyboard seat".to_string());
        };
        let inhibitor = unsafe {
            wl_proxy_marshal_flags(
                manager,
                MANAGER_INHIBIT_SHORTCUTS,
                &INHIBITOR_INTERFACE,
                1,
                0,
                ptr::null::<c_void>(),
                surface,
                seat,
            )
        };
        if inhibitor.is_null() {
            unsafe {
                wl_proxy_marshal_flags(manager, MANAGER_DESTROY, ptr::null(), 1, DESTROY_FLAG);
                wl_proxy_destroy(seat);
            }
            return Err("The compositor refused to inhibit shortcuts".to_string());
        }
        unsafe { wl_display_flush(display) };
        Ok(Self {
            display,
            manager,
            seat,
            inhibitor,
        })
    }
}

impl Drop for WaylandInhibitor {
    fn drop(&mut self) {
        unsafe {
            wl_proxy_marshal_flags(
                self.inhibitor,
                INHIBITOR_DESTROY,
                ptr::null(),
                1,
                DESTROY_FLAG,
            );
            wl_proxy_marshal_flags(self.manager, MANAGER_DESTROY, ptr::null(), 1, DESTROY_FLAG);
            wl_proxy_destroy(self.seat);
            wl_display_flush(self.display);
        }
    }
}
