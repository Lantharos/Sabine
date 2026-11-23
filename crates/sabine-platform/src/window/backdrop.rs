/// What a window's glass is for. Kestrel gives transient surfaces the
/// material people chose for panels and menus, and windows the one they
/// chose for app windows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackdropRole {
    #[default]
    Window,
    /// A popover, a menu or a palette.
    Transient,
}

impl BackdropRole {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "window" => Some(Self::Window),
            "transient" => Some(Self::Transient),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Transient => "transient",
        }
    }
}

/// The material a window would like its glass to have instead of the one
/// people chose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackdropPreference {
    #[default]
    User,
    Luca,
    Maris,
    Niko,
}

impl BackdropPreference {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "luca" => Some(Self::Luca),
            "maris" => Some(Self::Maris),
            "niko" => Some(Self::Niko),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Luca => "luca",
            Self::Maris => "maris",
            Self::Niko => "niko",
        }
    }
}

/// The material a window's glass has. On Kestrel, Luca is a blur of what is
/// behind the window, Maris the desktop's surface colour over a wide blur of
/// the wallpaper and Niko a blur of what is behind the window in the
/// lightness of the surface colour. Maris and Niko already are the whole
/// surface, so pages draw no tint of their own over them. Everywhere else
/// glass is Luca.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackdropMaterial {
    #[default]
    Luca,
    Maris,
    Niko,
}

impl BackdropMaterial {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Luca => "luca",
            Self::Maris => "maris",
            Self::Niko => "niko",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowBackdrop {
    pub role: BackdropRole,
    pub preference: BackdropPreference,
}
