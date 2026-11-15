use crate::window::{MenuItemListener, SabineWindow};
use crate::{MenuActivation, MenuItem};

impl SabineWindow {
    /// Adds the app's own items to the window's context menu, after the
    /// edit and spelling items Chromium offers where the page was clicked.
    /// Pages change them with `menu.setContextMenu`.
    pub fn context_menu(mut self, items: Vec<MenuItem>) -> Self {
        self.config.context_menu = items;
        self
    }

    /// Sets the app menu bar, which `[[menu]]` in `Sabine.toml` also sets.
    /// macOS shows it as the app's main menu, with a standard one when the
    /// app sets none. Windows draws it as the window's menu bar when the
    /// window has the system's titlebar. Pages read it with `menu.appMenu`
    /// to draw it themselves, which on Linux is the only way it shows.
    pub fn app_menu(mut self, items: Vec<MenuItem>) -> Self {
        self.config.app_menu = items;
        self
    }

    /// Calls `listener` with each item someone chooses from the window's
    /// context menu, a menu its page showed, or the app menu bar, unless
    /// the item has a role, which does what the role says instead.
    pub fn on_menu_item(
        mut self,
        listener: impl Fn(MenuActivation) + Send + Sync + 'static,
    ) -> Self {
        self.config.listeners.menu_item = Some(MenuItemListener::new(listener));
        self
    }
}
