// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Toasts belong to an AppUserModelID. An unpackaged app names its own: the
// process takes the app id as its AppUserModelID, and a per-user registry
// entry gives that id the app's display name, which Windows needs before it
// shows a toast from an app without a Start menu shortcut carrying the id.
// Toast events arrive on thread pool threads. Clicks on a toast's buttons
// reach the app through the same Activated event as clicks on the toast,
// told apart by the arguments each button carries.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::{IReference, TypedEventHandler};
use windows::UI::Notifications::{
    ToastActivatedEventArgs, ToastNotification, ToastNotificationManager,
    ToastNotificationPriority, ToastNotifier,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::core::{HSTRING, IInspectable, Interface, Ref};

use super::{Notification, NotificationEvent, NotificationEvents, Urgency};

pub(super) struct Backend {
    notifier: Result<ToastNotifier, String>,
    shown: Shown,
    events: NotificationEvents,
}

/// The toasts on screen by id. A toast leaves it when it is clicked or
/// dismissed, unless one shown later with the same id replaced it.
#[derive(Clone, Default)]
struct Shown(Arc<Mutex<HashMap<String, ToastNotification>>>);

impl Shown {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, ToastNotification>> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn forget(&self, id: &str, toast: Option<&ToastNotification>) -> bool {
        let mut shown = self.lock();
        let current = toast.is_some() && shown.get(id) == toast;
        if current {
            shown.remove(id);
        }
        current
    }
}

impl Backend {
    pub(super) fn new(app_id: &str, app_name: &str, events: NotificationEvents) -> Self {
        Self {
            notifier: register_app(app_id, app_name).map_err(|error| error.to_string()),
            shown: Shown::default(),
            events,
        }
    }

    pub(super) fn show(&self, notification: Notification) {
        let id = notification.id.clone();
        if let Err(message) = self.try_show(notification) {
            (self.events)(NotificationEvent::Failed { id, message });
        }
    }

    /// Hides the toast, which reports it closed once Windows dismissed it.
    pub(super) fn close(&self, id: &str) {
        let toast = self.shown.lock().get(id).cloned();
        if let (Ok(notifier), Some(toast)) = (&self.notifier, toast) {
            let _ = notifier.Hide(&toast);
        }
    }

    fn try_show(&self, notification: Notification) -> Result<(), String> {
        let notifier = self.notifier.as_ref().map_err(Clone::clone)?;
        let toast = toast(&notification).map_err(|error| error.to_string())?;
        let actions = notification
            .shown_actions()
            .map(|action| action.id.clone())
            .collect::<Vec<_>>();
        let id = notification.id;
        let activated = (Arc::clone(&self.events), id.clone(), self.shown.clone());
        toast
            .Activated(&TypedEventHandler::new(
                move |sender: Ref<ToastNotification>, arguments: Ref<IInspectable>| {
                    activated.2.forget(&activated.1, sender.as_ref());
                    let arguments = arguments
                        .as_ref()
                        .and_then(|arguments| arguments.cast::<ToastActivatedEventArgs>().ok());
                    (activated.0)(activation(&activated.1, &actions, arguments.as_ref()));
                    Ok(())
                },
            ))
            .map_err(|error| error.to_string())?;
        let dismissed = (Arc::clone(&self.events), id.clone(), self.shown.clone());
        toast
            .Dismissed(&TypedEventHandler::new(
                move |sender: Ref<ToastNotification>, _| {
                    if dismissed.2.forget(&dismissed.1, sender.as_ref()) {
                        (dismissed.0)(NotificationEvent::Closed(dismissed.1.clone()));
                    }
                    Ok(())
                },
            ))
            .map_err(|error| error.to_string())?;
        if let Some(previous) = self.shown.lock().insert(id, toast.clone()) {
            let _ = notifier.Hide(&previous);
        }
        notifier.Show(&toast).map_err(|error| error.to_string())
    }
}

/// A click on the toast carries no arguments, and a click on a button the
/// button's place, with the text of its field for a reply.
fn activation(
    id: &str,
    actions: &[String],
    arguments: Option<&ToastActivatedEventArgs>,
) -> NotificationEvent {
    let button = arguments
        .and_then(|arguments| arguments.Arguments().ok())
        .and_then(|place| place.to_string().parse::<usize>().ok())
        .and_then(|place| Some((place, actions.get(place)?)));
    match button {
        Some((place, action)) => NotificationEvent::Action {
            id: id.to_string(),
            action: action.clone(),
            reply: arguments.and_then(|arguments| typed_reply(arguments, place)),
            activation_token: None,
        },
        None => NotificationEvent::Clicked {
            id: id.to_string(),
            activation_token: None,
        },
    }
}

fn typed_reply(arguments: &ToastActivatedEventArgs, place: usize) -> Option<String> {
    let text = arguments
        .UserInput()
        .ok()?
        .Lookup(&HSTRING::from(input_id(place)))
        .ok()?;
    Some(
        text.cast::<IReference<HSTRING>>()
            .ok()?
            .Value()
            .ok()?
            .to_string(),
    )
}

fn input_id(place: usize) -> String {
    format!("reply-{place}")
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
    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(toast_xml(notification)))?;
    let toast = ToastNotification::CreateToastNotification(&document)?;
    match notification.urgency {
        Urgency::Low => toast.SetSuppressPopup(true)?,
        Urgency::Normal => {}
        Urgency::Critical => toast.SetPriority(ToastNotificationPriority::High)?,
    }
    Ok(toast)
}

fn toast_xml(notification: &Notification) -> String {
    let mut visual = format!(
        "<text>{}</text><text>{}</text>",
        xml_text(&notification.title),
        xml_text(&notification.body)
    );
    if let Some(icon) = &notification.icon {
        visual += &format!(
            r#"<image placement="appLogoOverride" src="{}"/>"#,
            file_uri(icon)
        );
    }
    if let Some(image) = &notification.image {
        visual += &format!(r#"<image src="{}"/>"#, file_uri(image));
    }
    let mut inputs = String::new();
    let mut buttons = String::new();
    for (place, action) in notification.shown_actions().enumerate() {
        let label = xml_text(&action.label);
        match &action.reply {
            Some(placeholder) => {
                inputs += &format!(
                    r#"<input id="{}" type="text" placeHolderContent="{}"/>"#,
                    input_id(place),
                    xml_text(placeholder)
                );
                buttons += &format!(
                    r#"<action content="{label}" arguments="{place}" hint-inputId="{}"/>"#,
                    input_id(place)
                );
            }
            None => buttons += &format!(r#"<action content="{label}" arguments="{place}"/>"#),
        }
    }
    let actions = if buttons.is_empty() {
        String::new()
    } else {
        format!("<actions>{inputs}{buttons}</actions>")
    };
    let audio = if notification.silent {
        r#"<audio silent="true"/>"#
    } else {
        ""
    };
    format!(
        r#"<toast><visual><binding template="ToastGeneric">{visual}</binding></visual>{actions}{audio}</toast>"#
    )
}

fn file_uri(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    xml_text(&format!(
        "file:///{}",
        path.trim_start_matches('/')
            .replace('%', "%25")
            .replace(' ', "%20")
            .replace('#', "%23")
    ))
}

fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
