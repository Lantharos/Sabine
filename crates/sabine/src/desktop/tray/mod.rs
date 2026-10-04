mod bridge;
mod icon;
mod menu;

use std::{cell::RefCell, path::PathBuf};

use sabine_platform::{PlatformEvent, TrayActivation, TrayIcon, TrayMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent, menu::MenuEvent};

use super::{EventQueue, platform::UiQueue, sanitize_id};
pub(crate) use bridge::UPDATE_COMMAND;
use menu::MenuEntries;

thread_local! {
    static TRAY: RefCell<Option<tray_icon::TrayIcon>> = const { RefCell::new(None) };
}

/// Changes the app's tray icon while it runs. Changes apply in the order
/// they are made, on the thread that owns the icon.
#[derive(Clone)]
pub struct TrayHandle {
    queue: UiQueue,
    entries: MenuEntries,
}

impl TrayHandle {
    pub(super) fn spawn(queue: UiQueue, icon: &TrayIcon, events: &EventQueue) -> Self {
        let handle = Self {
            queue,
            entries: MenuEntries::default(),
        };
        forward_events(icon.id.clone(), handle.entries.clone(), events.clone());
        let icon = icon.clone();
        let entries = handle.entries.clone();
        handle.queue.run(move || {
            match create(&icon, &entries) {
                Ok(tray) => TRAY.with(|current| *current.borrow_mut() = Some(tray)),
                Err(error) => report(format!("could not show the tray icon: {error}")),
            };
        });
        handle
    }

    /// Shows the image at `path`, or a plain dot without one.
    pub fn set_icon(&self, path: Option<PathBuf>, template: bool) {
        self.with_tray(move |tray, _| {
            let image = icon::load(path.as_deref());
            let template = template || image.is_black_and_white();
            show_image(tray, image.into_icon()?, template)
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

/// Removes the icon. Runs on the thread that owns it.
pub(super) fn remove() {
    TRAY.with(|tray| tray.borrow_mut().take());
}

fn create(icon: &TrayIcon, entries: &MenuEntries) -> Result<tray_icon::TrayIcon, String> {
    let image = icon::load(icon.icon_path.as_deref());
    let template = icon.template || image.is_black_and_white();
    let builder = TrayIconBuilder::new()
        .with_id(sanitize_id(&icon.id))
        .with_tooltip(icon.tooltip.as_deref().unwrap_or(&icon.title))
        .with_menu(Box::new(menu::build(&icon.menu, entries)?))
        .with_menu_on_left_click(false);
    #[cfg(target_os = "linux")]
    let builder = builder.with_title(&icon.title);
    with_image(builder, image.into_icon()?, template)
        .build()
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn with_image(builder: TrayIconBuilder, icon: tray_icon::Icon, template: bool) -> TrayIconBuilder {
    if template {
        builder.with_icon_templated(icon)
    } else {
        builder.with_icon(icon)
    }
}

#[cfg(not(target_os = "macos"))]
fn with_image(builder: TrayIconBuilder, icon: tray_icon::Icon, _template: bool) -> TrayIconBuilder {
    builder.with_icon(icon)
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

#[cfg(not(target_os = "macos"))]
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
