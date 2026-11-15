//! Menus and dialogs, which the window host shows and answers.

pub const COMMAND_PREFIX: &str = "sabine.menu.";
/// Replaces the app's items in the window's context menu.
pub const SET_CONTEXT_MENU_COMMAND: &str = "sabine.menu.setContextMenu";
/// Shows a menu at a point of the page, answering with the id chosen.
pub const POPUP_COMMAND: &str = "sabine.menu.popup";
/// Replaces the app menu bar of every window.
pub const SET_APP_MENU_COMMAND: &str = "sabine.menu.setAppMenu";
/// The app menu bar, for pages that draw it.
pub const APP_MENU_COMMAND: &str = "sabine.menu.appMenu";
/// Chooses an item of the app menu bar, as a page that draws it does.
pub const ACTIVATE_COMMAND: &str = "sabine.menu.activate";
/// Shows a message over the page, answering with the button picked.
pub const DIALOG_COMMAND: &str = "sabine.dialog.show";

pub(super) const COMMANDS: [&str; 6] = [
    SET_CONTEXT_MENU_COMMAND,
    POPUP_COMMAND,
    SET_APP_MENU_COMMAND,
    APP_MENU_COMMAND,
    ACTIVATE_COMMAND,
    DIALOG_COMMAND,
];
