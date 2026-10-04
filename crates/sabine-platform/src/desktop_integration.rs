use std::path::PathBuf;

/// A status icon in the system tray or menu bar. `title` names the icon for
/// assistive technology and desktops that list tray items; it is not drawn
/// beside the icon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayIcon {
    pub id: String,
    pub title: String,
    pub icon_path: Option<PathBuf>,
    /// Draw the icon as a macOS template image, which the menu bar tints to
    /// match its appearance. Black-and-white icons are drawn as templates
    /// automatically.
    pub template: bool,
    pub tooltip: Option<String>,
    pub menu: Vec<TrayMenuItem>,
}

impl TrayIcon {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon_path: None,
            template: false,
            tooltip: None,
            menu: Vec::new(),
        }
    }

    pub fn icon(mut self, path: impl Into<PathBuf>) -> Self {
        self.icon_path = Some(path.into());
        self
    }

    pub fn template(mut self, template: bool) -> Self {
        self.template = template;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn menu(mut self, menu: Vec<TrayMenuItem>) -> Self {
        self.menu = menu;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuItem {
    pub id: String,
    pub label: String,
    pub action: Option<String>,
    pub enabled: bool,
    pub kind: TrayMenuItemKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayMenuItemKind {
    Normal,
    Separator,
    Checkbox { checked: bool },
    Submenu(Vec<TrayMenuItem>),
}

impl TrayMenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            action: None,
            enabled: true,
            kind: TrayMenuItemKind::Normal,
        }
    }

    pub fn separator() -> Self {
        Self {
            kind: TrayMenuItemKind::Separator,
            ..Self::new("", "")
        }
    }

    pub fn checkbox(id: impl Into<String>, label: impl Into<String>, checked: bool) -> Self {
        Self {
            kind: TrayMenuItemKind::Checkbox { checked },
            ..Self::new(id, label)
        }
    }

    pub fn submenu(
        id: impl Into<String>,
        label: impl Into<String>,
        items: Vec<TrayMenuItem>,
    ) -> Self {
        Self {
            kind: TrayMenuItemKind::Submenu(items),
            ..Self::new(id, label)
        }
    }

    pub fn action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutostartEntry {
    pub id: String,
    pub name: String,
    pub command: String,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShortcutModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub modifiers: ShortcutModifiers,
    pub key: String,
}

impl Shortcut {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            modifiers: ShortcutModifiers::default(),
            key: key.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalShortcutRegistration {
    pub id: String,
    pub shortcut: Shortcut,
    pub action: String,
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub description: Option<String>,
    pub desktop_command: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeepLinkRegistration {
    pub id: String,
    pub schemes: Vec<String>,
}

impl DeepLinkRegistration {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || matches!(self.id.as_str(), "." | "..")
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
        {
            return Err("URL handler id must be a nonempty desktop identifier".into());
        }
        for scheme in &self.schemes {
            if !scheme
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                || !scheme
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte))
            {
                return Err(format!("invalid URL scheme: {scheme}"));
            }
        }
        Ok(())
    }

    pub fn new(
        id: impl Into<String>,
        schemes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            id: id.into(),
            schemes: schemes.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMessagingHost {
    pub id: String,
    pub name: String,
    pub executable: PathBuf,
    pub allowed_origins: Vec<String>,
    pub allowed_extensions: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SingleInstancePolicy {
    #[default]
    AllowMultiple,
    ReuseExisting,
    FocusExisting,
}

/// A click on the tray icon, or on one of its menu items. `checked` carries a
/// checkbox item's new state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayActivation {
    pub tray_id: String,
    pub item_id: Option<String>,
    pub action: Option<String>,
    pub checked: Option<bool>,
}

impl TrayActivation {
    pub fn new(tray_id: impl Into<String>) -> Self {
        Self {
            tray_id: tray_id.into(),
            item_id: None,
            action: None,
            checked: None,
        }
    }

    pub fn item(
        tray_id: impl Into<String>,
        item_id: impl Into<String>,
        action: Option<String>,
        checked: Option<bool>,
    ) -> Self {
        Self {
            tray_id: tray_id.into(),
            item_id: Some(item_id.into()),
            action,
            checked,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalShortcutActivation {
    pub id: String,
    pub action: String,
    pub activation_token: Option<String>,
}

impl GlobalShortcutActivation {
    pub fn new(id: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            action: action.into(),
            activation_token: None,
        }
    }

    pub fn activation_token(mut self, token: impl Into<String>) -> Self {
        self.activation_token = Some(token.into());
        self
    }
}

/// A global shortcut the desktop did not register, such as one another app
/// already holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalShortcutFailure {
    pub id: String,
    pub action: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SingleInstanceActivation {
    pub policy: SingleInstancePolicy,
    pub arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub activation_token: Option<String>,
}

impl SingleInstanceActivation {
    pub fn new(policy: SingleInstancePolicy, arguments: Vec<String>) -> Self {
        Self {
            policy,
            arguments,
            working_directory: None,
            activation_token: None,
        }
    }

    pub fn working_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.working_directory = Some(directory.into());
        self
    }

    pub fn activation_token(mut self, token: impl Into<String>) -> Self {
        self.activation_token = Some(token.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformEvent {
    OpenUrls(Vec<String>),
    Tray(TrayActivation),
    GlobalShortcut(GlobalShortcutActivation),
    GlobalShortcutFailed(GlobalShortcutFailure),
    SingleInstance(SingleInstanceActivation),
}
