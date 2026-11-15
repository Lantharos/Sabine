//! The app menu bar as the macOS main menu.
//!
//! The menu shows each item's accelerator but leaves the keys to the page
//! first: its key equivalents never fire, and the browser hands the window
//! the accelerators the page leaves unhandled, as on the other desktops.

use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSMenu, NSMenuItem};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
use winit::event_loop::EventLoopProxy;

use crate::menu::Accelerator;
use crate::{MenuItem, MenuRole};

struct Clicks {
    tags: Rc<RefCell<Vec<usize>>>,
    proxy: EventLoopProxy,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SabineMenuTarget"]
    #[ivars = Clicks]
    struct MenuTarget;

    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        #[unsafe(method(chooseItem:))]
        fn choose_item(&self, item: &NSMenuItem) {
            let Ok(tag) = usize::try_from(item.tag()) else {
                return;
            };
            self.ivars().tags.borrow_mut().push(tag);
            self.ivars().proxy.wake_up();
        }
    }
);

define_class!(
    /// The main menu, which leaves every key to the window.
    #[unsafe(super(NSMenu))]
    #[thread_kind = MainThreadOnly]
    #[name = "SabineMainMenu"]
    struct MainMenuBar;

    impl MainMenuBar {
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, _event: &NSEvent) -> bool {
            false
        }
    }
);

pub(super) struct MainMenu {
    _menu: Retained<MainMenuBar>,
    _target: Retained<MenuTarget>,
}

impl MainMenu {
    /// Makes `items` the app's main menu, with the first of them as the
    /// menu macOS names after the app. Returns the ids of its items by the
    /// tag each reports.
    pub(super) fn install(
        items: &[MenuItem],
        app: &str,
        tags: Rc<RefCell<Vec<usize>>>,
        proxy: EventLoopProxy,
    ) -> (Self, Vec<String>) {
        let main_thread = MainThreadMarker::new().expect("the window host runs on the main thread");
        let target: Retained<MenuTarget> = unsafe {
            msg_send![
                super(MenuTarget::alloc(main_thread).set_ivars(Clicks { tags, proxy })),
                init
            ]
        };
        let menu: Retained<MainMenuBar> =
            unsafe { msg_send![MainMenuBar::alloc(main_thread), init] };
        let mut builder = Builder {
            main_thread,
            target: &target,
            app,
            ids: Vec::new(),
        };
        builder.fill(&menu, items);
        let ids = builder.ids;
        NSApplication::sharedApplication(main_thread).setMainMenu(Some(&menu));
        (
            Self {
                _menu: menu,
                _target: target,
            },
            ids,
        )
    }
}

struct Builder<'a> {
    main_thread: MainThreadMarker,
    target: &'a MenuTarget,
    app: &'a str,
    ids: Vec<String>,
}

impl Builder<'_> {
    fn fill(&mut self, menu: &NSMenu, items: &[MenuItem]) {
        menu.setAutoenablesItems(false);
        for item in items {
            if item.separator {
                menu.addItem(&NSMenuItem::separatorItem(self.main_thread));
                continue;
            }
            let title = NSString::from_str(&item.shown_label(self.app));
            let entry = NSMenuItem::new(self.main_thread);
            entry.setTitle(&title);
            entry.setEnabled(item.enabled);
            if !item.items.is_empty() || item.role == Some(MenuRole::Services) {
                let submenu = NSMenu::initWithTitle(NSMenu::alloc(self.main_thread), &title);
                self.fill(&submenu, &item.items);
                let application = NSApplication::sharedApplication(self.main_thread);
                match item.role {
                    Some(MenuRole::Services) => application.setServicesMenu(Some(&submenu)),
                    Some(MenuRole::Window) => application.setWindowsMenu(Some(&submenu)),
                    _ => {}
                }
                entry.setSubmenu(Some(&submenu));
                menu.addItem(&entry);
                continue;
            }
            if let Some(checked) = item.checked {
                entry.setState(isize::from(checked));
            }
            if let Some(accelerator) = item.shown_accelerator() {
                entry.setKeyEquivalent(&NSString::from_str(&key_equivalent(&accelerator)));
                entry.setKeyEquivalentModifierMask(modifier_mask(&accelerator));
            }
            unsafe {
                entry.setTarget(Some(self.target as &AnyObject));
                entry.setAction(Some(choose_item()));
            }
            entry.setTag(self.ids.len() as isize);
            self.ids.push(item.chosen_id().to_string());
            menu.addItem(&entry);
        }
    }
}

fn choose_item() -> Sel {
    sel!(chooseItem:)
}

/// The menu macOS gives apps that set none: the app, Edit, View and Window
/// menus.
pub(super) fn standard_menu() -> Vec<MenuItem> {
    use MenuRole::*;
    let role = MenuItem::role;
    vec![
        MenuItem::submenu(
            "",
            vec![
                role(About),
                MenuItem::separator(),
                role(Services),
                MenuItem::separator(),
                role(Hide),
                role(HideOthers),
                role(ShowAll),
                MenuItem::separator(),
                role(Quit),
            ],
        ),
        MenuItem::submenu(
            "Edit",
            vec![
                role(Undo),
                role(Redo),
                MenuItem::separator(),
                role(Cut),
                role(Copy),
                role(Paste),
                role(SelectAll),
            ],
        ),
        MenuItem::submenu("View", vec![role(Fullscreen)]),
        role(Window).items(vec![
            role(Minimize),
            role(Zoom),
            MenuItem::separator(),
            role(Close),
        ]),
    ]
}

/// Runs a role that belongs to the macOS app menu.
pub(super) fn run_app_role(role: MenuRole) {
    let main_thread = MainThreadMarker::new().expect("the window host runs on the main thread");
    let application = NSApplication::sharedApplication(main_thread);
    match role {
        MenuRole::About => application.orderFrontStandardAboutPanel(None),
        MenuRole::Hide => application.hide(None),
        MenuRole::HideOthers => application.hideOtherApplications(None),
        MenuRole::ShowAll => application.unhideAllApplications(None),
        _ => {}
    }
}

fn key_equivalent(accelerator: &Accelerator) -> String {
    let function = |offset: u32| char::from_u32(0xF700 + offset).map(String::from);
    let named = match accelerator.key.as_str() {
        "Enter" => Some("\r".to_string()),
        "Escape" => Some("\u{1b}".to_string()),
        "Tab" => Some("\t".to_string()),
        "Space" => Some(" ".to_string()),
        "Backspace" => Some("\u{8}".to_string()),
        "Delete" => function(0x28),
        "Home" => function(0x29),
        "End" => function(0x2B),
        "PageUp" => function(0x2C),
        "PageDown" => function(0x2D),
        "ArrowUp" => function(0x00),
        "ArrowDown" => function(0x01),
        "ArrowLeft" => function(0x02),
        "ArrowRight" => function(0x03),
        "Plus" => Some("+".to_string()),
        "Minus" => Some("-".to_string()),
        "Equal" => Some("=".to_string()),
        key => key
            .strip_prefix('F')
            .and_then(|number| number.parse::<u32>().ok())
            .and_then(|number| function(0x03 + number)),
    };
    named.unwrap_or_else(|| accelerator.key.to_lowercase())
}

fn modifier_mask(accelerator: &Accelerator) -> NSEventModifierFlags {
    [
        (accelerator.control, NSEventModifierFlags::Control),
        (accelerator.alt, NSEventModifierFlags::Option),
        (accelerator.shift, NSEventModifierFlags::Shift),
        (accelerator.meta, NSEventModifierFlags::Command),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .fold(NSEventModifierFlags::empty(), |mask, (_, flag)| mask | flag)
}
