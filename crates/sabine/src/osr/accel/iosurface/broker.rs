use std::{
    collections::HashMap,
    ffi::CString,
    io,
    ops::Deref,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use mach2::{
    bootstrap::{bootstrap_check_in, bootstrap_port},
    kern_return::KERN_SUCCESS,
    mach_port::{mach_port_deallocate, mach_port_mod_refs},
    message::{
        MACH_MSG_PORT_DESCRIPTOR, MACH_MSG_SUCCESS, MACH_MSG_TIMEOUT_NONE, MACH_MSGH_BITS_COMPLEX,
        MACH_RCV_INVALID_NAME, MACH_RCV_MSG, MACH_RCV_PORT_CHANGED, MACH_RCV_PORT_DIED, mach_msg,
        mach_msg_body_t, mach_msg_destroy, mach_msg_header_t, mach_msg_port_descriptor_t,
    },
    port::{MACH_PORT_NULL, MACH_PORT_RIGHT_RECEIVE, mach_port_t},
    traps::mach_task_self,
};
use objc2_core_foundation::CFRetained;
use objc2_io_surface::IOSurfaceRef;

const SURFACE_MESSAGE_ID: i32 = 0x5ab1;
const SURFACE_ANNOUNCE: u32 = 1;
const SURFACE_RETIRE: u32 = 2;
const SURFACE_TOKEN_BYTES: usize = 64;
const SURFACE_WAIT: Duration = Duration::from_millis(250);

#[repr(C)]
struct SurfaceMessage {
    header: mach_msg_header_t,
    body: mach_msg_body_t,
    surface: mach_msg_port_descriptor_t,
    surface_id: u64,
    kind: u32,
    token: [u8; SURFACE_TOKEN_BYTES],
}

const _: () = assert!(size_of::<SurfaceMessage>() == 120);

#[repr(C)]
struct ReceivedMessage {
    message: SurfaceMessage,
    trailer: [u8; 128],
}

#[derive(Clone)]
pub(crate) struct SharedSurface(CFRetained<IOSurfaceRef>);

// Safety: IOSurface references are thread-safe reference-counted kernel objects.
unsafe impl Send for SharedSurface {}
unsafe impl Sync for SharedSurface {}

impl Deref for SharedSurface {
    type Target = IOSurfaceRef;

    fn deref(&self) -> &IOSurfaceRef {
        &self.0
    }
}

impl std::fmt::Debug for SharedSurface {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedSurface")
    }
}

#[derive(Default)]
pub(crate) struct SurfaceRegistry {
    surfaces: Mutex<HashMap<u64, SharedSurface>>,
    announced: Condvar,
}

impl SurfaceRegistry {
    pub(crate) fn surface(&self, surface_id: u64) -> Option<SharedSurface> {
        let deadline = Instant::now() + SURFACE_WAIT;
        let mut surfaces = self.surfaces.lock().ok()?;
        loop {
            if let Some(surface) = surfaces.get(&surface_id) {
                return Some(surface.clone());
            }
            let remaining = deadline.checked_duration_since(Instant::now())?;
            surfaces = self.announced.wait_timeout(surfaces, remaining).ok()?.0;
        }
    }

    fn insert(&self, surface_id: u64, surface: SharedSurface) {
        if let Ok(mut surfaces) = self.surfaces.lock() {
            surfaces.insert(surface_id, surface);
        }
        self.announced.notify_all();
    }

    fn remove(&self, surface_id: u64) {
        if let Ok(mut surfaces) = self.surfaces.lock() {
            surfaces.remove(&surface_id);
        }
    }
}

/// Mach service through which the browser host hands this window its
/// Sabine-owned IOSurfaces, each once, before frames reference them.
pub(crate) struct SurfaceBroker {
    service_name: String,
    port: mach_port_t,
    registry: Arc<SurfaceRegistry>,
    receiver: Option<JoinHandle<()>>,
}

impl SurfaceBroker {
    pub(crate) fn start(token: &str) -> io::Result<Self> {
        if token.len() > SURFACE_TOKEN_BYTES {
            return Err(io::Error::other("the window token is too long"));
        }
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let service_name = format!("dev.sabine.surfaces.{}.{nanos}", std::process::id());
        let name = CString::new(service_name.clone()).map_err(io::Error::other)?;
        let mut port = MACH_PORT_NULL;
        let result = unsafe { bootstrap_check_in(bootstrap_port, name.as_ptr(), &mut port) };
        if result != KERN_SUCCESS {
            return Err(io::Error::other(format!(
                "could not register the shared surface service ({result})"
            )));
        }
        let registry = Arc::new(SurfaceRegistry::default());
        let token = token.as_bytes().to_vec();
        let receiver = thread::Builder::new()
            .name("sabine-surfaces".into())
            .spawn({
                let registry = Arc::clone(&registry);
                move || receive_surfaces(port, &token, &registry)
            })?;
        Ok(Self {
            service_name,
            port,
            registry,
            receiver: Some(receiver),
        })
    }

    pub(crate) fn service_name(&self) -> &str {
        &self.service_name
    }

    pub(crate) fn registry(&self) -> Arc<SurfaceRegistry> {
        Arc::clone(&self.registry)
    }
}

impl Drop for SurfaceBroker {
    fn drop(&mut self) {
        unsafe {
            mach_port_mod_refs(mach_task_self(), self.port, MACH_PORT_RIGHT_RECEIVE, -1);
        }
        if let Some(receiver) = self.receiver.take() {
            let _ = receiver.join();
        }
    }
}

fn receive_surfaces(port: mach_port_t, token: &[u8], registry: &SurfaceRegistry) {
    loop {
        let mut received: ReceivedMessage = unsafe { std::mem::zeroed() };
        let result = unsafe {
            mach_msg(
                &mut received.message.header,
                MACH_RCV_MSG,
                0,
                size_of::<ReceivedMessage>() as u32,
                port,
                MACH_MSG_TIMEOUT_NONE,
                MACH_PORT_NULL,
            )
        };
        match result {
            MACH_MSG_SUCCESS => handle_message(&mut received.message, token, registry),
            MACH_RCV_INVALID_NAME | MACH_RCV_PORT_DIED | MACH_RCV_PORT_CHANGED => return,
            _ => {}
        }
    }
}

fn handle_message(message: &mut SurfaceMessage, token: &[u8], registry: &SurfaceRegistry) {
    let valid = message.header.msgh_id == SURFACE_MESSAGE_ID
        && message.header.msgh_size as usize == size_of::<SurfaceMessage>()
        && message.header.msgh_bits & MACH_MSGH_BITS_COMPLEX != 0
        && message.body.msgh_descriptor_count == 1
        && u32::from(message.surface.type_) == MACH_MSG_PORT_DESCRIPTOR
        && message.token[..token.len()] == *token
        && message.token[token.len()..].iter().all(|byte| *byte == 0);
    if !valid {
        unsafe { mach_msg_destroy(&mut message.header) };
        return;
    }
    let surface_port = message.surface.name;
    match message.kind {
        SURFACE_ANNOUNCE => {
            if let Some(surface) = IOSurfaceRef::lookup_from_mach_port(surface_port) {
                registry.insert(message.surface_id, SharedSurface(surface));
            }
        }
        SURFACE_RETIRE => registry.remove(message.surface_id),
        _ => {}
    }
    if surface_port != MACH_PORT_NULL {
        unsafe { mach_port_deallocate(mach_task_self(), surface_port) };
    }
}
