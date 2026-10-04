//! Commands Sabine answers itself, which every page may call next to the
//! app's own.

pub(crate) mod activity;
pub mod clipboard;
pub mod media;

pub const POPUP_OPEN_COMMAND: &str = "sabine.popup.open";
pub const POPUP_CLOSE_COMMAND: &str = "sabine.popup.close";
pub const INHIBIT_SHORTCUTS_COMMAND: &str = "sabine.window.inhibitShortcuts";
pub const SET_REGIONS_COMMAND: &str = "sabine.window.setRegions";
pub const CONTROLS_OVERLAY_COMMAND: &str = "sabine.window.controlsOverlay";
/// Sent when the corner the system's window controls cover changes.
pub const CONTROLS_OVERLAY_EVENT: &str = "window.controlsOverlay";
pub const APPEARANCE_COMMAND: &str = "sabine.system.appearance";
/// Sent when the desktop switches between light and dark or its accent
/// color changes.
pub const APPEARANCE_EVENT: &str = "system.appearance";
pub const NOTIFICATION_COMMAND_PREFIX: &str = "sabine.notification.";
pub const NOTIFICATION_SHOW_COMMAND: &str = "sabine.notification.show";
pub const NOTIFICATION_CLOSE_COMMAND: &str = "sabine.notification.close";

const WINDOW_COMMANDS: [&str; 8] = [
    POPUP_OPEN_COMMAND,
    POPUP_CLOSE_COMMAND,
    INHIBIT_SHORTCUTS_COMMAND,
    SET_REGIONS_COMMAND,
    CONTROLS_OVERLAY_COMMAND,
    APPEARANCE_COMMAND,
    NOTIFICATION_SHOW_COMMAND,
    NOTIFICATION_CLOSE_COMMAND,
];

/// Every command a window's pages may call: the app's own, Sabine's, and the
/// native media commands when the window can show native media.
pub fn page_commands(mut commands: Vec<String>, native_media: bool) -> Vec<String> {
    let media: &[&str] = if native_media { &media::COMMANDS } else { &[] };
    let builtin = activity::COMMANDS
        .iter()
        .chain(&WINDOW_COMMANDS)
        .chain(&crate::guest::COMMANDS)
        .chain(media);
    for command in builtin {
        if !commands.iter().any(|existing| existing == command) {
            commands.push(command.to_string());
        }
    }
    commands
}
