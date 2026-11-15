//! What the sound server lists, and remembers volume and mute for, an app's
//! streams as. Chromium opens every stream from its audio process under its
//! own name, and libpulse, and PipeWire for streams that reach it through ALSA
//! or GStreamer, take the properties of each stream from the environment of
//! the process that opens it, so the app's window host and the Chromium it
//! starts carry the app's identity there.

const IDENTITY: &str = "PULSE_PROP_OVERRIDE";
const DEFAULTS: &str = "PULSE_PROP";
const PIPEWIRE_IDENTITY: &str = "PIPEWIRE_PROPS";

/// The variables that name an app to the sound server, which a runtime started
/// outside the window host is handed.
pub(crate) const VARIABLES: [&str; 3] = [IDENTITY, DEFAULTS, PIPEWIRE_IDENTITY];

const SABINE_ID: &str = "com.lantharos.sabine";
const SABINE_NAME: &str = "Sabine";

/// The app's name, id and the icon its desktop entry uses, which is its id, in
/// place of Chromium's, and the role of its sound where its streams name none.
pub(crate) fn app_environment(
    app_id: &str,
    name: &str,
    role: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut environment = identities(app_id, name);
    if let Some(role) = role {
        environment.push((DEFAULTS, property("media.role", role)));
    }
    environment
}

/// The runtime apps installed for every user share plays all of their sound
/// from one audio process, so it names itself instead of whichever app
/// started it.
pub(crate) fn shared_runtime_environment() -> Vec<(&'static str, String)> {
    identities(SABINE_ID, SABINE_NAME)
}

/// The sound server's role for the first `Sabine.toml` category that says
/// what the app's sound is.
pub(crate) fn role(categories: &[String]) -> Option<&'static str> {
    categories
        .iter()
        .find_map(|category| match category.as_str() {
            "Audio" | "Music" => Some("music"),
            "Video" | "TV" => Some("video"),
            "Game" => Some("game"),
            "Telephony" | "VideoConference" => Some("phone"),
            "AudioVideoEditing" | "Recorder" => Some("production"),
            _ => None,
        })
}

fn identities(app_id: &str, name: &str) -> Vec<(&'static str, String)> {
    let pulse = [
        property("application.name", name),
        property("application.id", app_id),
        property("application.icon_name", app_id),
    ];
    let pipewire = [
        ("application.name", name),
        ("application.id", app_id),
        ("application.icon-name", app_id),
    ]
    .map(|(key, value)| format!("{key} = {}", pipewire_string(value)));
    vec![
        (IDENTITY, pulse.join(" ")),
        (PIPEWIRE_IDENTITY, format!("{{ {} }}", pipewire.join(" "))),
    ]
}

fn property(key: &str, value: &str) -> String {
    format!(
        "{key}='{}'",
        value.replace('\\', "\\\\").replace('\'', "\\'")
    )
}

fn pipewire_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::app_environment;

    #[test]
    fn names_the_app_with_quotes_kept() {
        assert_eq!(
            app_environment("com.example.notes", "Ana's \"Notes\" \\", Some("music")),
            vec![
                (
                    "PULSE_PROP_OVERRIDE",
                    "application.name='Ana\\'s \"Notes\" \\\\' application.id='com.example.notes' application.icon_name='com.example.notes'"
                        .to_string()
                ),
                (
                    "PIPEWIRE_PROPS",
                    "{ application.name = \"Ana's \\\"Notes\\\" \\\\\" application.id = \"com.example.notes\" application.icon-name = \"com.example.notes\" }"
                        .to_string()
                ),
                ("PULSE_PROP", "media.role='music'".to_string()),
            ]
        );
    }
}
