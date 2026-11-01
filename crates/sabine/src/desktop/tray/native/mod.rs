mod menu;

use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use sabine_platform::{PlatformEvent, TrayActivation, TrayIcon, TrayMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent, menu::MenuEvent};

use super::icon;
use crate::desktop::{EventQueue, platform::UiQueue, sanitize_id};
use menu::MenuEntries;

thread_local! {
    static TRAY: RefCell<Option<tray_icon::TrayIcon>> = const { RefCell::new(None) };
}

/// Changes the app's tray icon while it runs. Changes apply in the order
/// they are made, on the thread that owns the icon; images are decoded
/// before they reach it.
#[derive(Clone)]
pub struct TrayHandle {
    queue: UiQueue,
    entries: MenuEntries,
    icon_requests: Arc<AtomicU64>,
}

impl TrayHandle {
    pub(in crate::desktop) fn spawn(queue: UiQueue, icon: &TrayIcon, events: &EventQueue) -> Self {
        let handle = Self {
            queue,
            entries: MenuEntries::default(),
            icon_requests: Arc::default(),
        };
        forward_events(icon.id.clone(), handle.entries.clone(), events.clone());
        let created = icon.clone();
        let entries = handle.entries.clone();
        handle.queue.run(move || {
            match create(&created, &entries) {
                Ok(tray) => TRAY.with(|current| *current.borrow_mut() = Some(tray)),
                Err(error) => report(format!("could not show the tray icon: {error}")),
            };
        });
        handle.set_icon(icon.icon_path.clone(), icon.template);
        handle
    }

    /// Shows the image at `path`, or a plain dot without one. An image
    /// requested later replaces this one even when it decodes first.
    pub fn set_icon(&self, path: Option<PathBuf>, template: bool) {
        let request = self.icon_requests.fetch_add(1, Ordering::Relaxed) + 1;
        let handle = self.clone();
        sabine_platform::async_runtime().spawn_blocking(move || {
            let image = icon::load(path.as_deref());
            #[cfg(target_os = "macos")]
            let template = template || image.is_black_and_white();
            let requests = Arc::clone(&handle.icon_requests);
            handle.with_tray(move |tray, _| {
                if requests.load(Ordering::Relaxed) != request {
                    return Ok(());
                }
                show_image(tray, image.into_icon()?, template)
            });
        });
    }

    pub fn set_tooltip(&self, tooltip: Option<String>) {
        self.with_tray(move |tray, _| tray.set_tooltip(tooltip).map_err(|error| error.to_string()));
    }

    pub fn set_menu(&self, items: Vec<TrayMenuItem>) {
        self.with_tray(move |tray, entries| {
            tray.set_menu(Some(Box::new(menu::build(&items, entries)?)));
            Ok(())
        });
    }

    /// Removes the icon, on the thread that owns it.
    pub(in crate::desktop) fn remove(&self) {
        self.queue
            .run(|| TRAY.with(|tray| drop(tray.borrow_mut().take())));
    }

    fn with_tray(
        &self,
        change: impl FnOnce(&tray_icon::TrayIcon, &MenuEntries) -> Result<(), String> + Send + 'static,
    ) {
        let entries = self.entries.clone();
        self.queue.run(move || {
            TRAY.with(|tray| {
                if let Some(tray) = tray.borrow().as_ref()
                    && let Err(error) = change(tray, &entries)
                {
                    report(format!("could not update the tray icon: {error}"));
                }
            });
        });
    }
}

/// Creates the icon without its image, which follows once it is decoded.
fn create(icon: &TrayIcon, entries: &MenuEntries) -> Result<tray_icon::TrayIcon, String> {
    TrayIconBuilder::new()
        .with_id(sanitize_id(&icon.id))
        .with_tooltip(icon.tooltip.as_deref().unwrap_or(&icon.title))
        .with_menu(Box::new(menu::build(&icon.menu, entries)?))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn show_image(
    tray: &tray_icon::TrayIcon,
    icon: tray_icon::Icon,
    template: bool,
) -> Result<(), String> {
    if template {
        tray.set_icon_templated(Some(icon))
    } else {
        tray.set_icon(Some(icon))
    }
    .map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn show_image(
    tray: &tray_icon::TrayIcon,
    icon: tray_icon::Icon,
    _template: bool,
) -> Result<(), String> {
    tray.set_icon(Some(icon)).map_err(|error| error.to_string())
}

/// A left click activates the app; the menu opens on a right click on every
/// desktop.
fn forward_events(tray_id: String, entries: MenuEntries, events: EventQueue) {
    let clicks = events.clone();
    let clicked_tray = tray_id.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = clicks.send(PlatformEvent::Tray(TrayActivation::new(
                clicked_tray.clone(),
            )));
        }
    }));
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some(activation) = entries.activate(&tray_id, &event.id) {
            let _ = events.send(PlatformEvent::Tray(activation));
        }
    }));
}

fn report(message: String) {
    sabine_runtime::report_error("desktop", message);
}
