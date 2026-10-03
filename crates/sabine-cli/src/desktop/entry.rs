use serde::Deserialize;

const MAIN_CATEGORIES: [&str; 13] = [
    "AudioVideo",
    "Audio",
    "Video",
    "Development",
    "Education",
    "Game",
    "Graphics",
    "Network",
    "Office",
    "Science",
    "Settings",
    "System",
    "Utility",
];

/// How an app is listed and found in launchers, from `[app]` in `Sabine.toml`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(crate) struct Listing {
    pub generic_name: Option<String>,
    #[serde(default = "default_categories")]
    pub categories: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default = "default_listed")]
    pub listed: bool,
}

impl Default for Listing {
    fn default() -> Self {
        Self {
            generic_name: None,
            categories: default_categories(),
            keywords: Vec::new(),
            listed: default_listed(),
        }
    }
}

fn default_categories() -> Vec<String> {
    vec!["Utility".to_string()]
}

fn default_listed() -> bool {
    true
}

impl Listing {
    /// The closest macOS application category to the first main category.
    pub(crate) fn apple_category(&self) -> Option<&'static str> {
        self.categories
            .iter()
            .find_map(|category| match category.as_str() {
                "AudioVideo" | "Video" => Some("video"),
                "Audio" => Some("music"),
                "Development" => Some("developer-tools"),
                "Education" | "Science" => Some("education"),
                "Game" => Some("games"),
                "Graphics" => Some("graphics-design"),
                "Office" => Some("productivity"),
                "Network" => Some("social-networking"),
                "Settings" | "System" | "Utility" => Some("utilities"),
                _ => None,
            })
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self
            .generic_name
            .iter()
            .chain(&self.keywords)
            .any(|text| text.trim().is_empty() || text.chars().any(char::is_control))
        {
            return Err(
                "app generic_name and keywords must be nonempty and contain no control characters"
                    .to_string(),
            );
        }
        if let Some(category) = self.categories.iter().find(|category| {
            category.is_empty()
                || !category
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        }) {
            return Err(format!("invalid app category: {category:?}"));
        }
        if !self
            .categories
            .iter()
            .any(|category| MAIN_CATEGORIES.contains(&category.as_str()))
        {
            return Err(format!(
                "app categories need one of the main categories: {}",
                MAIN_CATEGORIES.join(", ")
            ));
        }
        Ok(())
    }
}

/// A freedesktop application entry.
pub(crate) struct Entry<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub exec: &'a str,
    pub icon: Option<&'a str>,
    pub mime_types: &'a [String],
    pub listing: &'a Listing,
}

impl Entry<'_> {
    pub(crate) fn render(&self) -> String {
        let mut entry = format!(
            "[Desktop Entry]\nType=Application\nName={}\n",
            string(self.name)
        );
        if let Some(generic_name) = &self.listing.generic_name {
            entry.push_str(&format!("GenericName={}\n", string(generic_name)));
        }
        entry.push_str(&format!("Exec={} %U\n", exec(self.exec)));
        if let Some(icon) = self.icon {
            entry.push_str(&format!("Icon={}\n", string(icon)));
        }
        if !self.mime_types.is_empty() {
            entry.push_str(&format!("MimeType={}\n", list(self.mime_types)));
        }
        if !self.listing.keywords.is_empty() {
            entry.push_str(&format!("Keywords={}\n", list(&self.listing.keywords)));
        }
        if !self.listing.listed {
            entry.push_str("NoDisplay=true\n");
        }
        entry.push_str(&format!(
            "Terminal=false\nCategories={}\nStartupNotify=true\nStartupWMClass={}\n",
            list(&self.listing.categories),
            string(self.id)
        ));
        entry
    }
}

fn string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

fn list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("{};", string(value.trim()).replace(';', "\\;")))
        .collect()
}

fn exec(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    format!("\"{}\"", escaped.replace('\\', "\\\\"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_listing_fields_escaped() {
        let listing = Listing {
            generic_name: Some("Terminal".to_string()),
            categories: vec!["System".to_string(), "TerminalEmulator".to_string()],
            keywords: vec!["console".to_string(), "a;b".to_string()],
            listed: true,
        };
        let entry = Entry {
            id: "com.example.tern",
            name: "Tern",
            exec: "/opt/tern/tern",
            icon: None,
            mime_types: &[],
            listing: &listing,
        }
        .render();
        assert!(entry.contains("GenericName=Terminal\n"));
        assert!(entry.contains("Categories=System;TerminalEmulator;\n"));
        assert!(entry.contains("Keywords=console;a\\;b;\n"));
    }

    #[test]
    fn unlisted_apps_keep_their_handlers() {
        let listing = Listing {
            listed: false,
            ..Listing::default()
        };
        let entry = Entry {
            id: "com.example.signin",
            name: "Sign-In",
            exec: "/opt/signin/signin",
            icon: None,
            mime_types: &["x-scheme-handler/example-signin".to_string()],
            listing: &listing,
        }
        .render();
        assert!(entry.contains("NoDisplay=true\n"));
        assert!(entry.contains("MimeType=x-scheme-handler/example-signin;\n"));
    }

    #[test]
    fn requires_a_main_category() {
        let listing = Listing {
            categories: vec!["TerminalEmulator".to_string()],
            ..Listing::default()
        };
        assert!(listing.validate().is_err());
        assert!(Listing::default().validate().is_ok());
    }
}
