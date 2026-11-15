use sabine_platform::{PlatformEvent, SingleInstancePolicy};

use super::SabineProcess;
use crate::bridge::platform_event_payload;
use crate::desktop::open_urls::Openable;
use crate::desktop::start_desktop_event_forwarder;
use crate::window::DesktopEventListener;

impl SabineProcess {
    /// Hands the desktop's events to the open queue, the app's listener and
    /// its pages.
    pub(crate) fn start_desktop_event_forwarder(
        &mut self,
        openable: Openable,
        listener: Option<DesktopEventListener>,
    ) {
        let Some(services) = self.desktop_services.as_ref() else {
            return;
        };
        let emitter = self.bridge_emitter.clone();
        let handle = self.handle.clone();
        let (stop, stopped) = crossbeam_channel::bounded(1);
        let open_urls = self.open_urls.clone();
        self.desktop_event_stop = Some(stop);
        self.desktop_event_thread = Some(start_desktop_event_forwarder(
            services,
            stopped,
            move |event| {
                let opened = match &event {
                    PlatformEvent::OpenUrls(urls) => open_urls.receive(urls.clone()),
                    PlatformEvent::SingleInstance(activation) => open_urls.receive_arguments(
                        activation.arguments.get(1..).unwrap_or_default(),
                        activation.working_directory.as_deref(),
                        &openable,
                    ),
                    _ => false,
                };
                if opened && !matches!(event, PlatformEvent::OpenUrls(_)) {
                    let _ = emitter.emit("app.openUrlsAvailable", serde_json::Value::Null);
                }
                if matches!(event, PlatformEvent::OpenUrls(_)) {
                    handle.present(None);
                }
                if let PlatformEvent::SingleInstance(activation) = &event
                    && activation.policy == SingleInstancePolicy::FocusExisting
                {
                    handle.present(activation.activation_token.clone());
                }
                if let Some(listener) = &listener {
                    listener.deliver(&handle, &event);
                }
                let (name, payload) = platform_event_payload(event);
                let _ = emitter.emit(name, payload);
            },
        ));
    }

    pub(super) fn stop_desktop_event_forwarder(&mut self) {
        if let Some(stop) = self.desktop_event_stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.desktop_event_thread.take() {
            let _ = thread.join();
        }
    }
}
