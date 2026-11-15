use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use block2::{DynBlock, RcBlock};
use objc2::{
    AllocAnyThread, DefinedClass, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
};
use objc2_foundation::{
    NSArray, NSBundle, NSError, NSObject, NSObjectProtocol, NSSet, NSString, NSURL,
};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationAttachment, UNNotificationCategory,
    UNNotificationCategoryOptions, UNNotificationDefaultActionIdentifier,
    UNNotificationDismissActionIdentifier, UNNotificationInterruptionLevel,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSound, UNTextInputNotificationAction, UNTextInputNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

use super::{Notification, NotificationAction, NotificationEvent, NotificationEvents, Urgency};

/// The category of notifications without buttons, which asks macOS to
/// report dismissals as well as clicks. Notifications with buttons get a
/// category for each set of buttons, named after this one.
const CATEGORY: &str = "dev.sabine.notification";

static ATTACHMENTS: AtomicU64 = AtomicU64::new(0);

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SabineNotificationDelegate"]
    #[ivars = NotificationEvents]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        // Without this, macOS shows nothing while the app is in front.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion: &DynBlock<dyn Fn()>,
        ) {
            (self.ivars())(response_event(response));
            completion.call(());
        }
    }
);

fn response_event(response: &UNNotificationResponse) -> NotificationEvent {
    let id = response.notification().request().identifier().to_string();
    let action = response.actionIdentifier();
    if &*action == unsafe { UNNotificationDismissActionIdentifier } {
        return NotificationEvent::Closed(id);
    }
    if &*action == unsafe { UNNotificationDefaultActionIdentifier } {
        return NotificationEvent::Clicked {
            id,
            activation_token: None,
        };
    }
    NotificationEvent::Action {
        id,
        action: action.to_string(),
        reply: response
            .downcast_ref::<UNTextInputNotificationResponse>()
            .map(|reply| reply.userText().to_string()),
        activation_token: None,
    }
}

/// The user notification center, which only serves bundled apps: an app run
/// outside its bundle reports every notification as failed.
pub(super) struct Backend {
    center: Option<(
        Retained<UNUserNotificationCenter>,
        Retained<NotificationDelegate>,
    )>,
    /// The buttons of each category registered so far. macOS keeps one set
    /// of categories per app, so registering one sets them all again.
    categories: Mutex<BTreeMap<String, Vec<NotificationAction>>>,
    events: NotificationEvents,
}

impl Backend {
    pub(super) fn new(_app_id: &str, _app_name: &str, events: NotificationEvents) -> Self {
        let center = NSBundle::mainBundle().bundleIdentifier().map(|_| {
            let center = UNUserNotificationCenter::currentNotificationCenter();
            let delegate: Retained<NotificationDelegate> = unsafe {
                msg_send![
                    super(NotificationDelegate::alloc().set_ivars(events.clone())),
                    init
                ]
            };
            center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            center.requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
                &RcBlock::new(|_, _| {}),
            );
            (center, delegate)
        });
        let backend = Self {
            center,
            categories: Mutex::new(BTreeMap::from([(CATEGORY.to_string(), Vec::new())])),
            events,
        };
        backend.register_categories(&backend.categories());
        backend
    }

    pub(super) fn show(&self, notification: Notification) {
        let Some((center, _)) = &self.center else {
            (self.events)(NotificationEvent::Failed {
                id: notification.id,
                message: "macOS only shows notifications from bundled apps".to_string(),
            });
            return;
        };
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&notification.title));
        content.setBody(&NSString::from_str(&notification.body));
        content.setCategoryIdentifier(&NSString::from_str(&self.category_of(&notification)));
        if !notification.silent {
            content.setSound(Some(&UNNotificationSound::defaultSound()));
        }
        content.setInterruptionLevel(match notification.urgency {
            Urgency::Low => UNNotificationInterruptionLevel::Passive,
            Urgency::Normal => UNNotificationInterruptionLevel::Active,
            Urgency::Critical => UNNotificationInterruptionLevel::TimeSensitive,
        });
        if let Some(attachment) = notification.image.as_deref().and_then(attachment) {
            content.setAttachments(&NSArray::from_retained_slice(&[attachment]));
        }
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&notification.id),
            &content,
            None,
        );
        let events = self.events.clone();
        let id = notification.id;
        center.addNotificationRequest_withCompletionHandler(
            &request,
            Some(&RcBlock::new(move |error: *mut NSError| {
                if let Some(error) = unsafe { error.as_ref() } {
                    events(NotificationEvent::Failed {
                        id: id.clone(),
                        message: error.localizedDescription().to_string(),
                    });
                }
            })),
        );
    }

    pub(super) fn close(&self, id: &str) {
        let Some((center, _)) = &self.center else {
            return;
        };
        let ids = NSArray::from_retained_slice(&[NSString::from_str(id)]);
        center.removePendingNotificationRequestsWithIdentifiers(&ids);
        center.removeDeliveredNotificationsWithIdentifiers(&ids);
        (self.events)(NotificationEvent::Closed(id.to_string()));
    }

    /// The category of the notification's buttons, registering it the
    /// first time a notification shows them.
    fn category_of(&self, notification: &Notification) -> String {
        let actions = notification.shown_actions().cloned().collect::<Vec<_>>();
        if actions.is_empty() {
            return CATEGORY.to_string();
        }
        let mut hasher = DefaultHasher::new();
        actions.hash(&mut hasher);
        let category = format!("{CATEGORY}.{:016x}", hasher.finish());
        let mut categories = self.categories();
        if !categories.contains_key(&category) {
            categories.insert(category.clone(), actions);
            self.register_categories(&categories);
        }
        category
    }

    fn register_categories(&self, categories: &BTreeMap<String, Vec<NotificationAction>>) {
        let Some((center, _)) = &self.center else {
            return;
        };
        let categories = categories
            .iter()
            .map(|(category, actions)| {
                let actions = actions.iter().map(action).collect::<Vec<_>>();
                UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
                    &NSString::from_str(category),
                    &NSArray::from_retained_slice(&actions),
                    &NSArray::new(),
                    UNNotificationCategoryOptions::CustomDismissAction,
                )
            })
            .collect::<Vec<_>>();
        center.setNotificationCategories(&NSSet::from_retained_slice(&categories));
    }

    fn categories(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Vec<NotificationAction>>> {
        self.categories
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        if let Some((center, _)) = &self.center {
            center.setDelegate(None);
        }
    }
}

fn action(action: &NotificationAction) -> Retained<UNNotificationAction> {
    let id = NSString::from_str(&action.id);
    let label = NSString::from_str(&action.label);
    match &action.reply {
        Some(placeholder) => Retained::into_super(
            UNTextInputNotificationAction::actionWithIdentifier_title_options_textInputButtonTitle_textInputPlaceholder(
                &id,
                &label,
                UNNotificationActionOptions::empty(),
                &label,
                &NSString::from_str(placeholder),
            ),
        ),
        None => UNNotificationAction::actionWithIdentifier_title_options(
            &id,
            &label,
            UNNotificationActionOptions::Foreground,
        ),
    }
}

/// macOS moves an attachment's file into its own store, so it gets a copy.
fn attachment(image: &Path) -> Option<Retained<UNNotificationAttachment>> {
    let copy = attachment_copy(image);
    std::fs::copy(image, &copy).ok()?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(&copy.to_string_lossy()));
    let attachment = unsafe {
        UNNotificationAttachment::attachmentWithIdentifier_URL_options_error(
            &NSString::from_str("image"),
            &url,
            None,
        )
    };
    if attachment.is_err() {
        let _ = std::fs::remove_file(&copy);
    }
    attachment.ok()
}

fn attachment_copy(image: &Path) -> PathBuf {
    let mut name = format!(
        "sabine-notification-{}-{}",
        std::process::id(),
        ATTACHMENTS.fetch_add(1, Ordering::Relaxed)
    );
    if let Some(extension) = image.extension() {
        name = format!("{name}.{}", extension.to_string_lossy());
    }
    std::env::temp_dir().join(name)
}
