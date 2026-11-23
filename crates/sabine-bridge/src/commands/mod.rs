//! Commands Sabine answers itself, which every page may call next to the
//! app's own.

pub(crate) mod activity;
pub mod clipboard;
pub mod media;
pub mod menu;
pub(crate) mod window;

pub const POPUP_OPEN_COMMAND: &str = "sabine.popup.open";
pub const POPUP_CLOSE_COMMAND: &str = "sabine.popup.close";
pub const INHIBIT_SHORTCUTS_COMMAND: &str = "sabine.window.inhibitShortcuts";
pub const SET_REGIONS_COMMAND: &str = "sabine.window.setRegions";
pub const SET_CONTROL_REGIONS_COMMAND: &str = "sabine.window.setControlRegions";
pub const CONTROLS_OVERLAY_COMMAND: &str = "sabine.window.controlsOverlay";
/// Changes the window as a [`WindowControl`](crate::WindowControl) describes.
pub const WINDOW_CONTROL_COMMAND: &str = "sabine.window.control";
/// The zoom factor of the window's page, sent by the native host itself
/// whenever it changes and never by pages.
pub const PAGE_ZOOM_COMMAND: &str = "sabine.window.pageZoom";
/// A token another app's window can present to take focus, made for this
/// window while the person uses it.
pub const ACTIVATION_TOKEN_COMMAND: &str = "sabine.window.activationToken";
/// The window as another window names its parent to open as its dialog,
/// `wayland:<handle>` on Linux and `null` on other desktops.
pub const IDENTIFIER_COMMAND: &str = "sabine.window.identifier";
/// Holds back the window's first frame until the page sends
/// [`REVEAL_COMMAND`], so the window appears with the page's first view.
pub const HOLD_REVEAL_COMMAND: &str = "sabine.window.holdReveal";
pub const REVEAL_COMMAND: &str = "sabine.window.reveal";
/// What the window showed when the session it was open in ended, for a
/// window the desktop opens again, and `null` for every other window.
pub const RESTORED_STATE_COMMAND: &str = "sabine.window.restoredState";
/// Keeps what the window shows, for the desktop to open it again with after
/// the session ends while the window is open.
pub const KEEP_STATE_COMMAND: &str = "sabine.window.keepState";
/// Sent when the corner the system's window controls cover changes.
pub const CONTROLS_OVERLAY_EVENT: &str = "window.controlsOverlay";
pub const APPEARANCE_COMMAND: &str = "sabine.system.appearance";
/// Sent when the desktop switches between light and dark or its accent
/// color changes.
pub const APPEARANCE_EVENT: &str = "system.appearance";
/// The displays connected to the computer.
pub const DISPLAYS_COMMAND: &str = "sabine.system.displays";
/// Sent when a display is connected or disconnected, or one changes its
/// bounds, work area, scale or refresh rate.
pub const DISPLAYS_EVENT: &str = "system.displays";
pub const NOTIFICATION_COMMAND_PREFIX: &str = "sabine.notification.";
pub const NOTIFICATION_SHOW_COMMAND: &str = "sabine.notification.show";
pub const NOTIFICATION_CLOSE_COMMAND: &str = "sabine.notification.close";
/// The files a page's drag carries to other apps, from the file URIs it put
/// in the drag's `text/uri-list`.
pub const DRAG_FILES_COMMAND: &str = "sabine.drag.files";
/// Prints PDF, image and text files through the desktop's print dialog.
pub const PRINT_FILES_COMMAND: &str = "sabine.print.files";
/// Opens a web or mail link in the person's default app.
pub const OPEN_LINK_COMMAND: &str = "sabine.shell.openLink";
/// Opens a folder in the person's file manager.
pub const SHOW_FOLDER_COMMAND: &str = "sabine.shell.showFolder";
/// The power source and screen lock as the desktop last reported them.
pub const POWER_STATE_COMMAND: &str = "sabine.power.state";
/// Watches for the person going idle for a number of seconds, until the
/// page that asked goes away or sends [`IDLE_UNWATCH_COMMAND`].
pub const IDLE_WATCH_COMMAND: &str = "sabine.idle.watch";
pub const IDLE_UNWATCH_COMMAND: &str = "sabine.idle.unwatch";
/// Sent to a window when a watch of its pages sees the person go idle or
/// come back.
pub const IDLE_EVENT: &str = "system.idle";

/// Pauses, resumes and cancels downloads the window's page started, which
/// the browser host answers itself.
pub const DOWNLOAD_COMMANDS: [&str; 3] = [
    "sabine.download.pause",
    "sabine.download.resume",
    "sabine.download.cancel",
];
/// Reads and changes spellchecking in the window's profile, which the
/// browser host answers itself.
pub const SPELLCHECK_COMMANDS: [&str; 4] = [
    "sabine.spellcheck.get",
    "sabine.spellcheck.set",
    "sabine.spellcheck.addWord",
    "sabine.spellcheck.replace",
];

const WINDOW_COMMANDS: [&str; 23] = [
    POPUP_OPEN_COMMAND,
    POPUP_CLOSE_COMMAND,
    INHIBIT_SHORTCUTS_COMMAND,
    SET_REGIONS_COMMAND,
    SET_CONTROL_REGIONS_COMMAND,
    CONTROLS_OVERLAY_COMMAND,
    WINDOW_CONTROL_COMMAND,
    ACTIVATION_TOKEN_COMMAND,
    IDENTIFIER_COMMAND,
    HOLD_REVEAL_COMMAND,
    REVEAL_COMMAND,
    RESTORED_STATE_COMMAND,
    KEEP_STATE_COMMAND,
    APPEARANCE_COMMAND,
    DISPLAYS_COMMAND,
    NOTIFICATION_SHOW_COMMAND,
    NOTIFICATION_CLOSE_COMMAND,
    DRAG_FILES_COMMAND,
    PRINT_FILES_COMMAND,
    OPEN_LINK_COMMAND,
    SHOW_FOLDER_COMMAND,
    IDLE_WATCH_COMMAND,
    IDLE_UNWATCH_COMMAND,
];

/// Every command a window's pages may call: the app's own, Sabine's, and the
/// native media commands when the window can show native media.
pub fn page_commands(mut commands: Vec<String>, native_media: bool) -> Vec<String> {
    let media: &[&str] = if native_media { &media::COMMANDS } else { &[] };
    let builtin = activity::COMMANDS
        .iter()
        .chain(&WINDOW_COMMANDS)
        .chain(&DOWNLOAD_COMMANDS)
        .chain(&SPELLCHECK_COMMANDS)
        .chain(&menu::COMMANDS)
        .chain(&crate::guest::COMMANDS)
        .chain(media);
    for command in builtin {
        if !commands.iter().any(|existing| existing == command) {
            commands.push(command.to_string());
        }
    }
    commands
}
