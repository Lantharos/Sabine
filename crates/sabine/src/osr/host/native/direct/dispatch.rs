use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop, event_created_child,
    globals::GlobalListContents,
    protocol::{
        wl_buffer::{self, WlBuffer},
        wl_compositor::WlCompositor,
        wl_region::WlRegion,
        wl_registry::WlRegistry,
        wl_shm::WlShm,
        wl_shm_pool::WlShmPool,
        wl_subcompositor::WlSubcompositor,
        wl_subsurface::WlSubsurface,
        wl_surface::WlSurface,
    },
};
use wayland_protocols::wp::{
    linux_dmabuf::zv1::client::{
        zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
        zwp_linux_dmabuf_v1::{self, ZwpLinuxDmabufV1},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

use super::worker::State;

delegate_noop!(State: WlCompositor);
delegate_noop!(State: WlSubcompositor);
delegate_noop!(State: WlSubsurface);
delegate_noop!(State: WlRegion);
delegate_noop!(State: ignore WlShm);
delegate_noop!(State: WlShmPool);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: WpViewporter);
delegate_noop!(State: WpViewport);

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

impl Dispatch<ZwpLinuxDmabufV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwpLinuxDmabufV1,
        event: zwp_linux_dmabuf_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_linux_dmabuf_v1::Event::Modifier {
            format,
            modifier_hi,
            modifier_lo,
        } = event
        {
            let modifier = u64::from(modifier_hi) << 32 | u64::from(modifier_lo);
            state.modifiers.insert((format, modifier));
        }
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, u64> for State {
    fn event(
        state: &mut Self,
        params: &ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        resource_id: &u64,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_linux_buffer_params_v1::Event::Created { buffer } => {
                state.buffer_created(*resource_id, buffer);
            }
            zwp_linux_buffer_params_v1::Event::Failed => state.buffer_failed(*resource_id),
            _ => return,
        }
        params.destroy();
    }

    event_created_child!(State, ZwpLinuxBufferParamsV1, [
        zwp_linux_buffer_params_v1::EVT_CREATED_OPCODE => (WlBuffer, ()),
    ]);
}

impl Dispatch<WlBuffer, ()> for State {
    fn event(
        state: &mut Self,
        buffer: &WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            state.buffer_released(buffer);
        }
    }
}
