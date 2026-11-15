/// The keys of a menu item's accelerator, written as modifiers and a key
/// joined by `+`, such as `Primary+Shift+S`. `Primary` is Command on macOS
/// and Control elsewhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Accelerator {
    pub(crate) control: bool,
    pub(crate) alt: bool,
    pub(crate) shift: bool,
    pub(crate) meta: bool,
    /// A single character, uppercase for letters, or a key name such as
    /// `Enter` or `F11`.
    pub(crate) key: String,
}

const NAMED_KEYS: [&str; 18] = [
    "Enter",
    "Escape",
    "Tab",
    "Space",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "Plus",
    "Minus",
    "Equal",
];

impl Accelerator {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let invalid = || format!("{text} is not an accelerator");
        let mut parts = text.split('+').map(str::trim).collect::<Vec<_>>();
        let key = parts
            .pop()
            .filter(|key| !key.is_empty())
            .ok_or_else(invalid)?;
        let mut accelerator = Self {
            control: false,
            alt: false,
            shift: false,
            meta: false,
            key: key_name(key).ok_or_else(invalid)?,
        };
        for modifier in parts {
            match modifier.to_ascii_lowercase().as_str() {
                "primary" if cfg!(target_os = "macos") => accelerator.meta = true,
                "primary" | "control" | "ctrl" => accelerator.control = true,
                "meta" | "command" | "cmd" => accelerator.meta = true,
                "alt" | "option" => accelerator.alt = true,
                "shift" => accelerator.shift = true,
                _ => return Err(invalid()),
            }
        }
        Ok(accelerator)
    }

    /// The accelerator in the form the browser host matches keys against.
    pub(crate) fn canonical(&self) -> String {
        let mut text = String::new();
        for (held, name) in [
            (self.control, "Control+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
            (self.meta, "Meta+"),
        ] {
            if held {
                text.push_str(name);
            }
        }
        text.push_str(&self.key);
        text
    }

    /// How menus on this desktop show the accelerator.
    pub(crate) fn display(&self) -> String {
        if cfg!(target_os = "macos") {
            let mut text = String::new();
            for (held, symbol) in [
                (self.control, '⌃'),
                (self.alt, '⌥'),
                (self.shift, '⇧'),
                (self.meta, '⌘'),
            ] {
                if held {
                    text.push(symbol);
                }
            }
            text.push_str(macos_key_symbol(&self.key).unwrap_or(&self.key));
            return text;
        }
        let meta = if cfg!(windows) { "Win+" } else { "Super+" };
        let mut text = String::new();
        for (held, name) in [
            (self.control, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
            (self.meta, meta),
        ] {
            if held {
                text.push_str(name);
            }
        }
        text.push_str(match self.key.as_str() {
            "ArrowUp" => "Up",
            "ArrowDown" => "Down",
            "ArrowLeft" => "Left",
            "ArrowRight" => "Right",
            "Equal" => "=",
            "Minus" => "-",
            "Plus" => "+",
            key => key,
        });
        text
    }
}

fn key_name(key: &str) -> Option<String> {
    let mut characters = key.chars();
    if let (Some(character), None) = (characters.next(), characters.next()) {
        return (!character.is_control()).then(|| character.to_uppercase().collect());
    }
    if let Some(named) = NAMED_KEYS
        .iter()
        .find(|named| named.eq_ignore_ascii_case(key))
    {
        return Some(named.to_string());
    }
    let number = key.strip_prefix(['F', 'f'])?.parse::<u8>().ok()?;
    (1..=24).contains(&number).then(|| format!("F{number}"))
}

fn macos_key_symbol(key: &str) -> Option<&'static str> {
    Some(match key {
        "Enter" => "↩",
        "Escape" => "⎋",
        "Tab" => "⇥",
        "Space" => "Space",
        "Backspace" => "⌫",
        "Delete" => "⌦",
        "Home" => "↖",
        "End" => "↘",
        "PageUp" => "⇞",
        "PageDown" => "⇟",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "Equal" => "=",
        "Minus" => "-",
        "Plus" => "+",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::Accelerator;

    #[test]
    fn accelerators_read_their_modifiers_in_any_case() {
        let accelerator = Accelerator::parse("shift+ctrl+s").unwrap();
        assert!(accelerator.shift && accelerator.control);
        assert_eq!(accelerator.canonical(), "Control+Shift+S");
        assert_eq!(Accelerator::parse("Alt+f11").unwrap().key, "F11");
    }

    #[test]
    fn unknown_modifiers_and_keys_are_refused() {
        assert!(Accelerator::parse("Hyper+S").is_err());
        assert!(Accelerator::parse("Primary+").is_err());
        assert!(Accelerator::parse("Primary+Banana").is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn menus_show_accelerators_in_the_desktops_notation() {
        let accelerator = Accelerator::parse("Primary+Shift+ArrowUp").unwrap();
        assert_eq!(accelerator.display(), "Ctrl+Shift+Up");
    }
}
