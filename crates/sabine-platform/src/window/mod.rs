#[cfg(target_os = "macos")]
mod controls_overlay;
mod effect;
mod options;
#[cfg(target_os = "linux")]
mod placement;
mod regions;

#[cfg(target_os = "macos")]
pub use controls_overlay::{ControlsOverlay, controls_overlay};
pub use effect::WindowEffect;
pub use options::{WindowBackgroundEffect, WindowChrome, WindowOptions};
#[cfg(target_os = "linux")]
pub use placement::{CompositorSupport, WindowPlacement, compositor_support};
pub use regions::{WindowRegion, WindowRegionAdaptive, WindowRegionRect, WindowRegions};
