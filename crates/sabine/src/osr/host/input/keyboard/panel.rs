use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use wayland_backend::client::Backend;
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry::WlRegistry, wl_seat::WlSeat},
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3, zwp_text_input_v3::ZwpTextInputV3,
};
use winit::window::Window;

use crate::osr::host::native::OsrNativeHost;

const SHOW_INPUT_PANEL_SINCE: u32 = 2;

/// Asks the desktop to show its on-screen keyboard.
pub(in crate::osr::host) struct InputPanel {
    queue: EventQueue<Ignored>,
    manager: ZwpTextInputManagerV3,
    seat: WlSeat,
}

struct Ignored;

impl InputPanel {
    fn connect(window: &dyn Window) -> Result<Self, String> {
        let RawDisplayHandle::Wayland(display) = window
            .display_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("the window has no Wayland display".to_string());
        };
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) =
            registry_queue_init::<Ignored>(&connection).map_err(|error| error.to_string())?;
        let handle = queue.handle();
        let manager = globals
            .bind(&handle, SHOW_INPUT_PANEL_SINCE..=SHOW_INPUT_PANEL_SINCE, ())
            .map_err(|_| "the desktop can't show an on-screen keyboard for apps".to_string())?;
        let seat = globals
            .bind(&handle, 1..=1, ())
            .map_err(|_| "the desktop has no seat".to_string())?;
        Ok(Self {
            queue,
            manager,
            seat,
        })
    }

    fn show(&mut self) -> Result<(), String> {
        self.queue
            .dispatch_pending(&mut Ignored)
            .map_err(|error| error.to_string())?;
        let text_input = self
            .manager
            .get_text_input(&self.seat, &self.queue.handle(), ());
        text_input.show_input_panel();
        text_input.destroy();
        self.queue.flush().map_err(|error| error.to_string())
    }
}

impl Drop for InputPanel {
    fn drop(&mut self) {
        self.manager.destroy();
        let _ = self.queue.flush();
    }
}

#[derive(Default)]
pub(in crate::osr::host) enum InputPanelState {
    #[default]
    Unconnected,
    Connected(InputPanel),
    Unavailable,
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn show_input_panel(&mut self) {
        if matches!(self.input_panel, InputPanelState::Unconnected) {
            let Some(window) = self.window.clone() else {
                return;
            };
            self.input_panel = match InputPanel::connect(window.as_ref()) {
                Ok(panel) => InputPanelState::Connected(panel),
                Err(error) => {
                    sabine_runtime::report_error(
                        "input",
                        format!("the on-screen keyboard is unavailable: {error}"),
                    );
                    InputPanelState::Unavailable
                }
            };
        }
        if let InputPanelState::Connected(panel) = &mut self.input_panel
            && let Err(error) = panel.show()
        {
            sabine_runtime::report_error(
                "input",
                format!("could not show the on-screen keyboard: {error}"),
            );
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Ignored {
    fn event(
        _state: &mut Self,
        _registry: &WlRegistry,
        _event: <WlRegistry as wayland_client::Proxy>::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Ignored: ignore WlSeat);
delegate_noop!(Ignored: ZwpTextInputManagerV3);
delegate_noop!(Ignored: ignore ZwpTextInputV3);
