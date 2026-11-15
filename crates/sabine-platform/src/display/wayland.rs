use std::thread;
use std::time::Duration;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::ZxdgOutputManagerV1;
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::{self, ZxdgOutputV1};
use winit::monitor::MonitorHandle;

use super::DisplayRect;

/// The compositor describes a change to its outputs in a burst of events,
/// which wakes the window once after this long.
const SETTLE: Duration = Duration::from_millis(100);
const OUTPUT_VERSION: u32 = 4;

/// Wakes the window whenever the compositor adds, removes or changes an
/// output, over a connection of its own that ends with the compositor.
pub struct DisplayWatcher;

impl DisplayWatcher {
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Self {
        thread::spawn(move || {
            let _ = watch(&wake);
        });
        Self
    }
}

fn watch(wake: &impl Fn()) -> Result<(), String> {
    let connection = Connection::connect_to_env().map_err(|error| error.to_string())?;
    let (globals, mut queue) =
        registry_queue_init::<Outputs>(&connection).map_err(|error| error.to_string())?;
    let handle = queue.handle();
    let mut outputs = Outputs {
        manager: globals.bind(&handle, 1..=3, ()).ok(),
        changed: false,
    };
    for global in globals.contents().clone_list() {
        if global.interface == WlOutput::interface().name {
            outputs.watch(globals.registry(), global.name, global.version, &handle);
        }
    }
    loop {
        queue
            .blocking_dispatch(&mut outputs)
            .map_err(|error| error.to_string())?;
        if outputs.changed {
            thread::sleep(SETTLE);
            queue
                .roundtrip(&mut outputs)
                .map_err(|error| error.to_string())?;
            outputs.changed = false;
            wake();
        }
    }
}

pub(super) fn rects(monitor: &MonitorHandle) -> Option<(DisplayRect, Option<DisplayRect>)> {
    let position = monitor.position()?;
    let size = monitor.current_video_mode()?.size();
    Some((
        DisplayRect {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        },
        None,
    ))
}

struct Outputs {
    manager: Option<ZxdgOutputManagerV1>,
    changed: bool,
}

impl Outputs {
    fn watch(
        &mut self,
        registry: &WlRegistry,
        name: u32,
        version: u32,
        handle: &QueueHandle<Self>,
    ) {
        let output = registry.bind::<WlOutput, _, _>(name, version.min(OUTPUT_VERSION), handle, ());
        if let Some(manager) = &self.manager {
            manager.get_xdg_output(&output, handle, ());
        }
        self.changed = true;
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Outputs {
    fn event(
        outputs: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == WlOutput::interface().name => {
                outputs.watch(registry, name, version, handle);
            }
            wl_registry::Event::GlobalRemove { .. } => outputs.changed = true,
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, ()> for Outputs {
    fn event(
        outputs: &mut Self,
        _: &WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_output::Event::Done) {
            outputs.changed = true;
        }
    }
}

impl Dispatch<ZxdgOutputV1, ()> for Outputs {
    fn event(
        outputs: &mut Self,
        _: &ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zxdg_output_v1::Event::Done) {
            outputs.changed = true;
        }
    }
}

delegate_noop!(Outputs: ignore ZxdgOutputManagerV1);
