/// A badge on the app's Dock or taskbar icon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppBadge {
    /// A number, such as unread messages. Zero shows no badge.
    Count(u64),
    /// A few characters. Linux docks show only counts.
    #[cfg(not(target_os = "linux"))]
    Text(String),
}

impl AppBadge {
    /// What the badge shows, or `None` when it shows nothing.
    pub fn label(&self) -> Option<String> {
        match self {
            Self::Count(0) => None,
            Self::Count(count) => Some(count.to_string()),
            #[cfg(not(target_os = "linux"))]
            Self::Text(text) => Some(text.trim().to_string()).filter(|text| !text.is_empty()),
        }
    }
}

/// A progress bar on the app's Dock or taskbar icon, for work such as a
/// download. Values run from 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AppProgress {
    #[default]
    Hidden,
    /// Work whose end can't be told yet. Linux docks show no such bar.
    #[cfg(not(target_os = "linux"))]
    Indeterminate,
    Normal(f64),
    /// Linux docks draw paused and failed work as they draw any other.
    Paused(f64),
    Error(f64),
}

impl AppProgress {
    /// How far the bar is filled, for the states that have a value.
    pub fn value(self) -> Option<f64> {
        match self {
            Self::Normal(value) | Self::Paused(value) | Self::Error(value) => {
                Some(if value.is_nan() {
                    0.0
                } else {
                    value.clamp(0.0, 1.0)
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_shows_no_badge() {
        assert_eq!(AppBadge::Count(0).label(), None);
        assert_eq!(AppBadge::Count(12).label().as_deref(), Some("12"));
    }

    #[test]
    fn progress_stays_between_empty_and_full() {
        assert_eq!(AppProgress::Normal(1.5).value(), Some(1.0));
        assert_eq!(AppProgress::Error(f64::NAN).value(), Some(0.0));
        assert_eq!(AppProgress::Hidden.value(), None);
    }
}
