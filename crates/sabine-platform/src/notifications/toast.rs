// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Toasts belong to an AppUserModelID. An unpackaged app names its own: the
// process takes the app id as its AppUserModelID, and a per-user registry
// entry gives that id the app's display name, which Windows needs before it
// shows a toast from an app without a Start menu shortcut carrying the id.
// Toast events arrive on thread pool threads.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager, ToastNotifier};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::core::HSTRING;

use super::{Notification, NotificationEvent, NotificationEvents};

pub(super) struct Backend {
    notifier: Result<ToastNotifier, String>,
    shown: Arc<Mutex<HashMap<String, ToastNotification>>>,
    events: NotificationEvents,
}

impl Backend {
    pub(super) fn new(app_id: &str, app_name: &str, events: NotificationEvents) -> Self {
        Self {
            notifier: register_app(app_id, app_name).map_err(|error| error.to_string()),
            shown: Arc::default(),
            events,
        }
    }

    pub(super) fn show(&self, notification: Notification) {
        let id = notification.id.clone();
        if let Err(message) = self.try_show(notification) {
            (self.events)(NotificationEvent::Failed { id, message });
        }
    }

    pub(super) fn close(&self, id: &str) {
        let toast = self
            .shown
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(id);
        if let (Ok(notifier), Some(toast)) = (&self.notifier, toast) {
            let _ = notifier.Hide(&toast);
        }
    }

    fn try_show(&self, notification: Notification) -> Result<(), String> {
        let notifier = self.notifier.as_ref().map_err(Clone::clone)?;
        let toast = toast(&notification).map_err(|error| error.to_string())?;
        let id = notification.id;
        let clicked = (Arc::clone(&self.events), id.clone());
        toast
            .Activated(&TypedEventHandler::new(move |_, _| {
                (clicked.0)(NotificationEvent::Clicked(clicked.1.clone()));
                Ok(())
            }))
            .map_err(|error| error.to_string())?;
        let dismissed = (
            Arc::clone(&self.events),
            id.clone(),
            Arc::clone(&self.shown),
        );
        toast
            .Dismissed(&TypedEventHandler::new(move |_, _| {
                dismissed
                    .2
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&dismissed.1);
                (dismissed.0)(NotificationEvent::Closed(dismissed.1.clone()));
                Ok(())
            }))
            .map_err(|error| error.to_string())?;
        if let Some(previous) = self
            .shown
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id, toast.clone())
        {
            let _ = notifier.Hide(&previous);
        }
        notifier.Show(&toast).map_err(|error| error.to_string())
    }
}

fn register_app(app_id: &str, app_name: &str) -> windows::core::Result<ToastNotifier> {
    let app_id = HSTRING::from(app_id);
    let name = app_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(&app_id)?;
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(format!(r"Software\Classes\AppUserModelId\{app_id}")),
            &HSTRING::from("DisplayName"),
            REG_SZ.0,
            Some(name.as_ptr().cast()),
            (name.len() * size_of::<u16>()) as u32,
        )
        .ok()?;
    }
    ToastNotificationManager::CreateToastNotifierWithId(&app_id)
}

fn toast(notification: &Notification) -> windows::core::Result<ToastNotification> {
    let audio = if notification.silent {
        r#"<audio silent="true"/>"#
    } else {
        ""
    };
    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual>{audio}</toast>"#,
        xml_text(&notification.title),
        xml_text(&notification.body)
    )))?;
    ToastNotification::CreateToastNotification(&document)
}

fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
