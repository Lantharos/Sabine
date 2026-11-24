use serde::{Deserialize, Serialize};

use super::config::{SabineWindowChrome, SabineWindowConfig};

const MARGIN: u32 = 24;

/// The edge or corner of the screen an overlay keeps to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SabineOverlayAnchor {
    Top,
    TopLeft,
    #[default]
    TopRight,
    Bottom,
    BottomLeft,
    BottomRight,
    Left,
    Right,
    Center,
}

/// Where an overlay window sits: above every other window, full-screen
/// ones included, taking neither focus nor the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SabineOverlay {
    #[serde(default)]
    pub anchor: SabineOverlayAnchor,
    /// The distance from the edges it keeps to, in logical pixels.
    #[serde(default = "default_margin")]
    pub margin: u32,
}

fn default_margin() -> u32 {
    MARGIN
}

impl Default for SabineOverlay {
    fn default() -> Self {
        Self {
            anchor: SabineOverlayAnchor::default(),
            margin: MARGIN,
        }
    }
}

/// Whether this desktop can show overlay windows. On Linux the compositor
/// has to offer the overlay layer of `wlr-layer-shell`, which Kestrel offers
/// only to apps that may record windows; Windows and macOS always can.
pub fn overlays_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        layer_shell::offered()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

#[cfg(target_os = "linux")]
mod layer_shell {
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::{Connection, Dispatch, QueueHandle};

    const INTERFACE: &str = "zwlr_layer_shell_v1";

    struct Registry;

    impl Dispatch<WlRegistry, GlobalListContents> for Registry {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    pub(super) fn offered() -> bool {
        let Ok(connection) = Connection::connect_to_env() else {
            return false;
        };
        registry_queue_init::<Registry>(&connection).is_ok_and(|(globals, _)| {
            globals
                .contents()
                .with_list(|list| list.iter().any(|global| global.interface == INTERFACE))
        })
    }
}

impl SabineWindowConfig {
    pub(crate) fn show_as_overlay(&mut self, placement: SabineOverlay) {
        self.overlay = Some(placement);
        self.chrome = SabineWindowChrome::Frameless;
        self.transparent = true;
        self.resizable = false;
        self.skip_taskbar = true;
        self.active = false;
    }
}

impl SabineOverlay {
    pub fn new(anchor: SabineOverlayAnchor, margin: u32) -> Self {
        Self { anchor, margin }
    }

    /// Where the overlay's top left corner goes on a screen at `origin` of
    /// `screen` size, in the same units as `size`.
    #[cfg(not(target_os = "linux"))]
    pub(crate) fn origin(
        self,
        origin: (f64, f64),
        screen: (f64, f64),
        size: (f64, f64),
    ) -> (f64, f64) {
        use SabineOverlayAnchor::*;
        let margin = f64::from(self.margin);
        let along = |start: f64, length: f64, extent: f64, at_start: bool, at_end: bool| match (
            at_start, at_end,
        ) {
            (true, false) => start + margin,
            (false, true) => start + length - extent - margin,
            _ => start + (length - extent) / 2.0,
        };
        let anchor = self.anchor;
        let left = matches!(anchor, TopLeft | BottomLeft | Left);
        let right = matches!(anchor, TopRight | BottomRight | Right);
        let top = matches!(anchor, Top | TopLeft | TopRight);
        let bottom = matches!(anchor, Bottom | BottomLeft | BottomRight);
        (
            along(origin.0, screen.0, size.0, left, right),
            along(origin.1, screen.1, size.1, top, bottom),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{SabineOverlay, SabineOverlayAnchor};

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn overlays_keep_to_their_anchor() {
        let screen = ((100.0, 0.0), (1920.0, 1080.0));
        let size = (200.0, 80.0);
        let at = |anchor| SabineOverlay::new(anchor, 24).origin(screen.0, screen.1, size);
        assert_eq!(at(SabineOverlayAnchor::TopRight), (1796.0, 24.0));
        assert_eq!(at(SabineOverlayAnchor::BottomLeft), (124.0, 976.0));
        assert_eq!(at(SabineOverlayAnchor::Center), (960.0, 500.0));
    }

    #[test]
    fn pages_name_overlays_in_kebab_case() {
        let overlay: SabineOverlay =
            serde_json::from_str(r#"{"anchor":"bottom-right","margin":8}"#).unwrap();
        assert_eq!(
            overlay,
            SabineOverlay::new(SabineOverlayAnchor::BottomRight, 8)
        );
        let defaults: SabineOverlay = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults, SabineOverlay::default());
    }
}
