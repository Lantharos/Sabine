use crate::osr::protocol::PermissionAsk;

/// What the prompt asks, such as "meet.example.com wants to use your camera
/// and microphone". The app's own pages ask in the app's name.
pub(super) fn message(ask: &PermissionAsk, app_name: &str) -> String {
    let who = if ask.app {
        app_name.to_string()
    } else {
        site(&ask.origin)
    };
    format!("{who} wants to {}", wants(&ask.permissions))
}

fn site(origin: &str) -> String {
    url::Url::parse(origin)
        .ok()
        .and_then(|url| {
            let host = url.host_str()?.to_string();
            Some(match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host,
            })
        })
        .unwrap_or_else(|| origin.trim_end_matches('/').to_string())
}

fn wants(permissions: &[String]) -> String {
    let has = |name: &str| permissions.iter().any(|permission| permission == name);
    let mut phrases = Vec::new();
    if has("camera") && has("microphone") {
        phrases.push("use your camera and microphone");
    }
    for permission in permissions {
        let phrase = match permission.as_str() {
            "camera" if has("microphone") => continue,
            "microphone" if has("camera") => continue,
            "camera" => "use your camera",
            "microphone" => "use your microphone",
            "screen" => "record your screen",
            "geolocation" => "know your location",
            "notifications" => "show notifications",
            "midi" => "control your MIDI devices",
            "clipboard" => "see what you copy",
            "pointer-lock" => "lock and hide your pointer",
            "keyboard-lock" => "capture your keyboard shortcuts",
            "local-fonts" => "use the fonts on your computer",
            "window-management" => "place windows on your displays",
            "idle-detection" => "know when you're away",
            "storage-access" => "use its data on other sites",
            "protected-media" => "play protected content",
            "file-system" => "edit files on your computer",
            "sensors" => "use your device's motion sensors",
            "local-network" => "reach devices on your network",
            "downloads" => "download several files",
            _ => continue,
        };
        phrases.push(phrase);
    }
    let last = phrases.pop().unwrap_or_default();
    if phrases.is_empty() {
        last.to_string()
    } else {
        format!("{} and {last}", phrases.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(origin: &str, app: bool, permissions: &[&str]) -> PermissionAsk {
        PermissionAsk {
            id: 1,
            origin: origin.to_string(),
            app,
            guest: None,
            permissions: permissions.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn names_the_site_and_what_it_wants() {
        assert_eq!(
            message(
                &ask(
                    "https://meet.example.com/",
                    false,
                    &["microphone", "camera"]
                ),
                "Notes"
            ),
            "meet.example.com wants to use your camera and microphone"
        );
        assert_eq!(
            message(
                &ask("sabine://app/", true, &["geolocation", "midi", "screen"]),
                "Notes"
            ),
            "Notes wants to know your location, control your MIDI devices and record your screen"
        );
    }
}
