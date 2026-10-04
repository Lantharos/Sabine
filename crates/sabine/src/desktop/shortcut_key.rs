//! Global shortcut keys are named the same way on every desktop: a single
//! character (`K`, `5`, `/`), a common key name (`Space`, `Enter`,
//! `Escape`, `Up`, `PageDown`, `F13`), or a W3C `KeyboardEvent.code` value
//! (`KeyK`, `Digit5`, `NumpadAdd`, `MediaPlayPause`).

/// The `KeyboardEvent.code` name of a shortcut key, or the key as given when
/// it is already a code name or a desktop-specific key name.
pub(super) fn key_code(key: &str) -> String {
    let key = key.trim();
    let mut characters = key.chars();
    if let (Some(character), None) = (characters.next(), characters.next())
        && let Some(code) = character_code(character)
    {
        return code;
    }
    let upper = key.to_ascii_uppercase();
    let named = match upper.as_str() {
        "SPACE" => Some("Space"),
        "ENTER" | "RETURN" => Some("Enter"),
        "ESC" | "ESCAPE" => Some("Escape"),
        "TAB" => Some("Tab"),
        "BACKSPACE" => Some("Backspace"),
        "DEL" | "DELETE" => Some("Delete"),
        "INS" | "INSERT" => Some("Insert"),
        "HOME" => Some("Home"),
        "END" => Some("End"),
        "PAGEUP" | "PGUP" | "PRIOR" => Some("PageUp"),
        "PAGEDOWN" | "PGDN" | "NEXT" => Some("PageDown"),
        "UP" => Some("ArrowUp"),
        "DOWN" => Some("ArrowDown"),
        "LEFT" => Some("ArrowLeft"),
        "RIGHT" => Some("ArrowRight"),
        "PRINT" | "PRINTSCREEN" => Some("PrintScreen"),
        "PAUSE" => Some("Pause"),
        "CAPSLOCK" => Some("CapsLock"),
        "NUMLOCK" => Some("NumLock"),
        "SCROLLLOCK" => Some("ScrollLock"),
        _ => None,
    };
    if let Some(named) = named {
        return named.to_string();
    }
    if let Some(number) = upper.strip_prefix('F')
        && number
            .parse::<u8>()
            .is_ok_and(|number| (1..=24).contains(&number))
    {
        return upper;
    }
    key.to_string()
}

fn character_code(character: char) -> Option<String> {
    let code = match character.to_ascii_uppercase() {
        letter @ 'A'..='Z' => return Some(format!("Key{letter}")),
        digit @ '0'..='9' => return Some(format!("Digit{digit}")),
        '-' => "Minus",
        '=' => "Equal",
        '[' => "BracketLeft",
        ']' => "BracketRight",
        '\\' => "Backslash",
        ';' => "Semicolon",
        '\'' => "Quote",
        ',' => "Comma",
        '.' => "Period",
        '/' => "Slash",
        '`' => "Backquote",
        _ => return None,
    };
    Some(code.to_string())
}

#[cfg(test)]
mod tests {
    use super::key_code;

    #[test]
    fn keys_resolve_to_their_code_names() {
        assert_eq!(key_code("k"), "KeyK");
        assert_eq!(key_code("5"), "Digit5");
        assert_eq!(key_code("/"), "Slash");
        assert_eq!(key_code("return"), "Enter");
        assert_eq!(key_code("PgDn"), "PageDown");
        assert_eq!(key_code("f13"), "F13");
        assert_eq!(key_code("NumpadAdd"), "NumpadAdd");
    }
}
