use block2::{DynBlock, RcBlock};
use objc2::{
    AllocAnyThread, DefinedClass, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
};
use objc2_foundation::{NSArray, NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationInterruptionLevel, UNNotificationPresentationOptions, UNNotificationRequest,
    UNNotificationResponse, UNNotificationSound, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

use super::{Notification, NotificationEvent, NotificationEvents, Urgency};

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
            let id = response.notification().request().identifier().to_string();
            (self.ivars())(NotificationEvent::Clicked {
                id,
                activation_token: None,
            });
            completion.call(());
        }
    }
);

/// The user notification center, which only serves bundled apps: an app run
/// outside its bundle reports every notification as failed.
pub(super) struct Backend {
    center: Option<(
        Retained<UNUserNotificationCenter>,
        Retained<NotificationDelegate>,
    )>,
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
        Self { center, events }
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
        if !notification.silent {
            content.setSound(Some(&UNNotificationSound::defaultSound()));
        }
        content.setInterruptionLevel(match notification.urgency {
            Urgency::Low => UNNotificationInterruptionLevel::Passive,
            Urgency::Normal => UNNotificationInterruptionLevel::Active,
            Urgency::Critical => UNNotificationInterruptionLevel::TimeSensitive,
        });
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
}

impl Drop for Backend {
    fn drop(&mut self) {
        if let Some((center, _)) = &self.center {
            center.setDelegate(None);
        }
    }
}
