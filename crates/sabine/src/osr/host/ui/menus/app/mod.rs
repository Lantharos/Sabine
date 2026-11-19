#[cfg(target_os = "macos")]
mod macos;
mod page;
mod roles;
#[cfg(windows)]
mod windows;

use std::borrow::Cow;
#[cfg(not(target_os = "linux"))]
use std::cell::RefCell;
use std::io::Write;
#[cfg(not(target_os = "linux"))]
use std::rc::Rc;

use winit::event_loop::ActiveEventLoop;

use crate::menu::{find, shortcuts};
use crate::osr::host::native::OsrNativeHost;
use crate::window::menu_item_line;
use crate::{MenuItem, MenuKind};

/// Sent to pages when the app menu bar changes, so pages that draw it can.
const APP_MENU_EVENT: &str = "menu.appMenu";
/// Sent to pages when someone chooses an item of the app menu bar.
const SELECT_EVENT: &str = "menu.select";

/// The app menu bar in the window.
#[derive(Default)]
pub(in crate::osr::host) struct Menus {
    /// The menu bar as the app set it, empty while it set none.
    bar: Vec<MenuItem>,
    /// The ids of the native menu bar's items, by the tag each carries.
    #[cfg(not(target_os = "linux"))]
    tags: Vec<String>,
    /// The tags of the native menu bar's items chosen since the window last
    /// ran them, which the desktop reports while the window can't act.
    #[cfg(not(target_os = "linux"))]
    clicked: Rc<RefCell<Vec<usize>>>,
    close_requested: bool,
    #[cfg(target_os = "macos")]
    native: Option<macos::MainMenu>,
    #[cfg(windows)]
    native: Option<windows::MenuBar>,
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn set_app_menu(&mut self, items: Vec<MenuItem>) {
        self.menus.bar = items;
        #[cfg(not(target_os = "linux"))]
        self.install_app_menu();
        self.send_menu_state();
        self.send_control(format!(
            "SABINE_BRIDGE_EVENT\t\"{APP_MENU_EVENT}\"\t{}\n",
            serde_json::json!({ "items": crate::menu::shown(&self.menus.bar, &self.config.title) })
        ));
    }

    /// The menu bar the window shows: the app's, or on macOS the standard
    /// one while the app sets none.
    fn shown_bar(&self) -> Cow<'_, [MenuItem]> {
        #[cfg(target_os = "macos")]
        if self.menus.bar.is_empty() {
            return Cow::Owned(macos::standard_menu());
        }
        Cow::Borrowed(&self.menus.bar)
    }

    /// Tells the browser which keys the app menu bar takes when the page
    /// leaves them unhandled.
    pub(in crate::osr::host) fn send_menu_state(&self) {
        let accelerators = shortcuts(&self.shown_bar())
            .into_iter()
            .map(|(accelerator, _)| accelerator.canonical())
            .collect::<Vec<_>>();
        self.send_control(format!(
            "menu_accelerators\t{}\n",
            serde_json::json!({ "accelerators": accelerators })
        ));
    }

    pub(in crate::osr::host) fn run_menu_accelerator(&mut self, accelerator: &str) {
        let bar = self.shown_bar().into_owned();
        let chosen = shortcuts(&bar)
            .into_iter()
            .find(|(shortcut, _)| shortcut.canonical() == accelerator)
            .map(|(_, item)| item.chosen_id().to_string());
        if let Some(id) = chosen {
            self.choose_menu_item(&bar, &id, MenuKind::App);
        }
    }

    /// Runs the item of `items` that reports `id`: what its role does, or
    /// else telling the app and its pages.
    pub(in crate::osr::host) fn choose_menu_item(
        &mut self,
        items: &[MenuItem],
        id: &str,
        kind: MenuKind,
    ) {
        let Some(item) = find(items, id) else {
            return;
        };
        if !item.role.is_some_and(|role| self.run_menu_role(role)) {
            self.report_menu_item(id, kind);
        }
    }

    fn report_menu_item(&self, id: &str, kind: MenuKind) {
        let mut output = std::io::stdout().lock();
        let _ = writeln!(output, "{}", menu_item_line(id, kind));
        let _ = output.flush();
        if kind == MenuKind::App {
            self.send_control(format!(
                "SABINE_BRIDGE_EVENT\t\"{SELECT_EVENT}\"\t{}\n",
                serde_json::json!({ "id": id })
            ));
        }
    }

    /// Runs what was chosen from the native menu bar, and the close a menu
    /// asked for, once the window can act on them.
    pub(in crate::osr::host) fn run_menu_requests(&mut self, event_loop: &dyn ActiveEventLoop) {
        #[cfg(not(target_os = "linux"))]
        {
            let clicked = std::mem::take(&mut *self.menus.clicked.borrow_mut());
            let bar = self.shown_bar().into_owned();
            for tag in clicked {
                if let Some(id) = self.menus.tags.get(tag).cloned() {
                    self.choose_menu_item(&bar, &id, MenuKind::App);
                }
            }
        }
        if std::mem::take(&mut self.menus.close_requested) {
            self.close_requested(event_loop);
        }
    }

    /// Shows the menu bar as the app's main menu.
    #[cfg(target_os = "macos")]
    pub(in crate::osr::host) fn install_app_menu(&mut self) {
        let bar = self.shown_bar().into_owned();
        let (menu, tags) = macos::MainMenu::install(
            &bar,
            &self.config.title,
            self.menus.clicked.clone(),
            self.proxy.clone(),
        );
        self.menus.native = Some(menu);
        self.menus.tags = tags;
    }

    /// Shows the menu bar under the window's own titlebar, which windows
    /// whose page draws the titlebar leave to the page.
    #[cfg(windows)]
    pub(in crate::osr::host) fn install_app_menu(&mut self) {
        self.menus.native = None;
        self.menus.tags.clear();
        let Some(window) = self.window.clone() else {
            return;
        };
        if self.menus.bar.is_empty() || !self.config.chrome.uses_native_decorations() {
            return;
        }
        let (menu, tags) = windows::MenuBar::attach(
            &window,
            &self.menus.bar,
            &self.config.title,
            self.menus.clicked.clone(),
            self.proxy.clone(),
        );
        self.menus.native = menu;
        self.menus.tags = tags;
    }
}
