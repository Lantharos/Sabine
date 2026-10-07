use std::ffi::c_void;
use std::ptr;

use winit::platform::wayland::WindowExtWayland;
use winit::window::Window;

use crate::wayland_client::{
    DESTROY_FLAG, Globals, WlDisplay, WlInterface, WlMessage, WlProxy, wayland_display,
    wl_display_flush, wl_proxy_marshal_flags,
};

const MANAGER_DESTROY: u32 = 0;
const MANAGER_GET_WINDOW_STATE: u32 = 1;
const STATE_DESTROY: u32 = 0;
const STATE_SET_SKIP_WINDOW_LIST: u32 = 1;
const STATE_SET_KEEP_ABOVE: u32 = 3;

#[repr(C)]
struct InterfaceTypes {
    get_window_state: [*const WlInterface; 2],
}

unsafe impl Sync for InterfaceTypes {}

static INTERFACE_TYPES: InterfaceTypes = InterfaceTypes {
    get_window_state: [&STATE_INTERFACE, ptr::null()],
};

const fn request(name: &'static std::ffi::CStr) -> WlMessage {
    WlMessage {
        name: name.as_ptr(),
        signature: c"".as_ptr(),
        types: ptr::null(),
    }
}

static MANAGER_METHODS: [WlMessage; 2] = [
    request(c"destroy"),
    WlMessage {
        name: c"get_window_state".as_ptr(),
        signature: c"no".as_ptr(),
        types: INTERFACE_TYPES.get_window_state.as_ptr(),
    },
];

static STATE_METHODS: [WlMessage; 5] = [
    request(c"destroy"),
    request(c"set_skip_window_list"),
    request(c"unset_skip_window_list"),
    request(c"set_keep_above"),
    request(c"unset_keep_above"),
];

static MANAGER_INTERFACE: WlInterface = WlInterface {
    name: c"kestrel_window_manager_v1".as_ptr(),
    version: 1,
    method_count: MANAGER_METHODS.len() as i32,
    methods: MANAGER_METHODS.as_ptr(),
    event_count: 0,
    events: ptr::null(),
};

static STATE_INTERFACE: WlInterface = WlInterface {
    name: c"kestrel_window_state_v1".as_ptr(),
    version: 1,
    method_count: STATE_METHODS.len() as i32,
    methods: STATE_METHODS.as_ptr(),
    event_count: 0,
    events: ptr::null(),
};

/// Keeps a Wayland window out of the taskbar and window switcher, or above
/// other windows, through `kestrel_window_v1`. Dropping it returns the window
/// to its default placement.
pub struct WindowPlacement {
    display: *mut WlDisplay,
    manager: *mut WlProxy,
    state: *mut WlProxy,
}

impl WindowPlacement {
    pub fn new(
        window: &dyn Window,
        skip_window_list: bool,
        keep_above: bool,
    ) -> Result<Self, String> {
        let display = wayland_display(window).ok_or("The window has no Wayland display")?;
        let toplevel = window
            .xdg_toplevel()
            .ok_or("The window has no Wayland toplevel")?
            .as_ptr();
        let globals = unsafe { Globals::discover(display) }
            .ok_or("Could not read the Wayland compositor globals")?;
        let manager = unsafe { globals.bind(&MANAGER_INTERFACE, 1) }
            .ok_or("This desktop cannot keep windows out of its taskbar or above other windows")?;
        let state = unsafe {
            wl_proxy_marshal_flags(
                manager,
                MANAGER_GET_WINDOW_STATE,
                &STATE_INTERFACE,
                1,
                0,
                ptr::null::<c_void>(),
                toplevel,
            )
        };
        let placement = Self {
            display,
            manager,
            state,
        };
        if skip_window_list {
            placement.send(STATE_SET_SKIP_WINDOW_LIST);
        }
        if keep_above {
            placement.send(STATE_SET_KEEP_ABOVE);
        }
        unsafe { wl_display_flush(display) };
        Ok(placement)
    }

    fn send(&self, opcode: u32) {
        unsafe { wl_proxy_marshal_flags(self.state, opcode, ptr::null(), 1, 0) };
    }
}

impl Drop for WindowPlacement {
    fn drop(&mut self) {
        unsafe {
            wl_proxy_marshal_flags(self.state, STATE_DESTROY, ptr::null(), 1, DESTROY_FLAG);
            wl_proxy_marshal_flags(self.manager, MANAGER_DESTROY, ptr::null(), 1, DESTROY_FLAG);
            wl_display_flush(self.display);
        }
    }
}
