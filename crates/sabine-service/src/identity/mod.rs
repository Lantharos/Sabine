//! Sabine's own name and icon, which its service, shared runtime and setup
//! windows show wherever the desktop lists them. Apps keep their own icons.

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub(crate) use linux::{install_for_system, install_for_user, remove_for_user};

pub const ICON_NAME: &str = "com.lantharos.sabine";

#[cfg(target_os = "linux")]
const ICON_SVG: &[u8] =
    include_bytes!("../../icons/hicolor/scalable/apps/com.lantharos.sabine.svg");

macro_rules! hicolor_png {
    ($size:literal) => {
        (
            $size,
            include_bytes!(concat!(
                "../../icons/hicolor/",
                $size,
                "x",
                $size,
                "/apps/com.lantharos.sabine.png"
            )) as &[u8],
        )
    };
}

const ICON_PNGS: [(u32, &[u8]); 8] = [
    hicolor_png!(16),
    hicolor_png!(24),
    hicolor_png!(32),
    hicolor_png!(48),
    hicolor_png!(64),
    hicolor_png!(128),
    hicolor_png!(256),
    hicolor_png!(512),
];

/// Sabine's icon as a PNG at least `size` pixels square, or its largest.
pub fn icon_png(size: u32) -> &'static [u8] {
    ICON_PNGS
        .iter()
        .find(|(available, _)| *available >= size)
        .unwrap_or(&ICON_PNGS[ICON_PNGS.len() - 1])
        .1
}
