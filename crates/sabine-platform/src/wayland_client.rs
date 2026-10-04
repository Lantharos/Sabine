use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

pub(crate) type WlDisplay = c_void;
pub(crate) type WlProxy = c_void;

#[repr(C)]
pub(crate) struct WlInterface {
    pub(crate) name: *const c_char,
    pub(crate) version: c_int,
    pub(crate) method_count: c_int,
    pub(crate) methods: *const WlMessage,
    pub(crate) event_count: c_int,
    pub(crate) events: *const WlMessage,
}

#[repr(C)]
pub(crate) struct WlMessage {
    pub(crate) name: *const c_char,
    pub(crate) signature: *const c_char,
    pub(crate) types: *const *const WlInterface,
}

unsafe impl Sync for WlInterface {}
unsafe impl Sync for WlMessage {}

pub(crate) const DESTROY_FLAG: u32 = 1;
const DISPLAY_GET_REGISTRY: u32 = 1;
const REGISTRY_BIND: u32 = 0;

#[link(name = "wayland-client")]
unsafe extern "C" {
    #[link_name = "wl_surface_interface"]
    pub(crate) static WL_SURFACE_INTERFACE: WlInterface;
    #[link_name = "wl_compositor_interface"]
    pub(crate) static WL_COMPOSITOR_INTERFACE: WlInterface;
    #[link_name = "wl_registry_interface"]
    static WL_REGISTRY_INTERFACE: WlInterface;
    #[link_name = "wl_region_interface"]
    pub(crate) static WL_REGION_INTERFACE: WlInterface;
    #[link_name = "wl_seat_interface"]
    pub(crate) static WL_SEAT_INTERFACE: WlInterface;

    pub(crate) fn wl_display_roundtrip(display: *mut WlDisplay) -> c_int;
    pub(crate) fn wl_display_flush(display: *mut WlDisplay) -> c_int;
    pub(crate) fn wl_proxy_add_listener(
        proxy: *mut WlProxy,
        implementation: *mut c_void,
        data: *mut c_void,
    ) -> c_int;
    pub(crate) fn wl_proxy_get_version(proxy: *mut WlProxy) -> u32;
    pub(crate) fn wl_proxy_destroy(proxy: *mut WlProxy);
    pub(crate) fn wl_proxy_marshal_flags(
        proxy: *mut WlProxy,
        opcode: u32,
        interface: *const WlInterface,
        version: u32,
        flags: u32,
        ...
    ) -> *mut WlProxy;
}

#[derive(Default)]
struct Entries(Vec<Global>);

struct Global {
    name: u32,
    interface: Vec<u8>,
    version: u32,
}

/// The globals a compositor advertised, collected with one roundtrip on the
/// window's own display connection.
pub(crate) struct Globals {
    registry: *mut WlProxy,
    entries: Box<Entries>,
}

impl Globals {
    pub(crate) unsafe fn discover(display: *mut WlDisplay) -> Option<Self> {
        let registry = unsafe {
            wl_proxy_marshal_flags(
                display,
                DISPLAY_GET_REGISTRY,
                &WL_REGISTRY_INTERFACE,
                wl_proxy_get_version(display),
                0,
                ptr::null::<c_void>(),
            )
        };
        if registry.is_null() {
            return None;
        }
        let mut globals = Self {
            registry,
            entries: Box::default(),
        };
        let listening = unsafe {
            wl_proxy_add_listener(
                registry,
                &REGISTRY_LISTENER as *const RegistryListener as *mut _,
                globals.entries.as_mut() as *mut Entries as *mut c_void,
            )
        };
        if listening != 0 || unsafe { wl_display_roundtrip(display) } < 0 {
            return None;
        }
        Some(globals)
    }

    pub(crate) unsafe fn bind(
        &self,
        interface: &WlInterface,
        version: u32,
    ) -> Option<*mut WlProxy> {
        let global = self.find(interface)?;
        let version = version.min(global.version);
        let proxy = unsafe {
            wl_proxy_marshal_flags(
                self.registry,
                REGISTRY_BIND,
                interface,
                version,
                0,
                global.name,
                interface.name,
                version,
                ptr::null::<c_void>(),
            )
        };
        (!proxy.is_null()).then_some(proxy)
    }

    fn find(&self, interface: &WlInterface) -> Option<&Global> {
        let name = unsafe { CStr::from_ptr(interface.name) }.to_bytes();
        self.entries
            .0
            .iter()
            .find(|global| global.interface == name)
    }
}

impl Drop for Globals {
    fn drop(&mut self) {
        unsafe { wl_proxy_destroy(self.registry) };
    }
}

unsafe extern "C" fn registry_global(
    data: *mut c_void,
    _registry: *mut WlProxy,
    name: u32,
    interface: *const c_char,
    version: u32,
) {
    let entries = unsafe { &mut *data.cast::<Entries>() };
    entries.0.push(Global {
        name,
        interface: unsafe { CStr::from_ptr(interface) }.to_bytes().to_vec(),
        version,
    });
}

unsafe extern "C" fn registry_global_remove(
    _data: *mut c_void,
    _registry: *mut WlProxy,
    _name: u32,
) {
}

#[repr(C)]
struct RegistryListener {
    global: unsafe extern "C" fn(*mut c_void, *mut WlProxy, u32, *const c_char, u32),
    global_remove: unsafe extern "C" fn(*mut c_void, *mut WlProxy, u32),
}

unsafe impl Sync for RegistryListener {}

static REGISTRY_LISTENER: RegistryListener = RegistryListener {
    global: registry_global,
    global_remove: registry_global_remove,
};

pub(crate) fn wayland_display<W>(window: &W) -> Option<*mut WlDisplay>
where
    W: HasDisplayHandle + ?Sized,
{
    match window.display_handle().ok()?.as_raw() {
        RawDisplayHandle::Wayland(display) => Some(display.display.as_ptr().cast()),
        _ => None,
    }
}

pub(crate) fn wayland_surface<W>(window: &W) -> Option<*mut WlProxy>
where
    W: HasWindowHandle + ?Sized,
{
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Wayland(surface) => Some(surface.surface.as_ptr().cast()),
        _ => None,
    }
}
