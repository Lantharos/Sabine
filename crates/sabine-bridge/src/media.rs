//! Native media surface bridge command names.
//!
//! Pages play media Chromium cannot decode through a native surface placed
//! beneath the page. The window host answers `sabine.media.*` commands itself.

pub const COMMAND_PREFIX: &str = "sabine.media.";

pub const CREATE_COMMAND: &str = "sabine.media.create";
pub const DESTROY_COMMAND: &str = "sabine.media.destroy";
pub const SET_RECT_COMMAND: &str = "sabine.media.setRect";
pub const PLAY_COMMAND: &str = "sabine.media.play";
pub const PAUSE_COMMAND: &str = "sabine.media.pause";
pub const SEEK_COMMAND: &str = "sabine.media.seek";
pub const SET_RATE_COMMAND: &str = "sabine.media.setRate";
pub const SET_VOLUME_COMMAND: &str = "sabine.media.setVolume";
pub const SET_MUTED_COMMAND: &str = "sabine.media.setMuted";
pub const SET_LOOP_COMMAND: &str = "sabine.media.setLoop";
pub const SELECT_TRACKS_COMMAND: &str = "sabine.media.selectTracks";

/// Event every media surface reports through, tagged with its `id` and `type`.
pub const EVENT: &str = "sabine.media";

const INTERNAL_COMMANDS: &[&str] = if cfg!(target_os = "linux") {
    &[
        CREATE_COMMAND,
        DESTROY_COMMAND,
        SET_RECT_COMMAND,
        PLAY_COMMAND,
        PAUSE_COMMAND,
        SEEK_COMMAND,
        SET_RATE_COMMAND,
        SET_VOLUME_COMMAND,
        SET_MUTED_COMMAND,
        SET_LOOP_COMMAND,
        SELECT_TRACKS_COMMAND,
    ]
} else {
    &[]
};

/// Append the media commands this platform supports to an allow-list.
pub fn bridge_commands_with_media(mut commands: Vec<String>) -> Vec<String> {
    for command in INTERNAL_COMMANDS {
        if !commands.iter().any(|existing| existing == command) {
            commands.push(command.to_string());
        }
    }
    commands
}
