use std::{
    fs,
    path::{Path, PathBuf},
};

const DRM_CLASS: &str = "/sys/class/drm";
const DRIVERS_WITHOUT_IMPLICIT_SYNC: &[&str] = &["nvidia"];

/// Whether the GPU Chromium draws on fences its work on the dma-bufs it hands
/// over. CEF passes no fence with an accelerated frame, so the browser host
/// can only wait for the drawing through the dma-buf itself. NVIDIA's driver
/// attaches nothing there, and frames copied straight away can be half drawn.
pub(crate) fn primary_gpu_fences_dmabufs() -> bool {
    primary_render_driver()
        .is_none_or(|driver| !DRIVERS_WITHOUT_IMPLICIT_SYNC.contains(&driver.as_str()))
}

/// The kernel driver of the render node compositors and Chromium pick by
/// default: the boot display device, or the first render node without one.
fn primary_render_driver() -> Option<String> {
    let mut nodes: Vec<PathBuf> = fs::read_dir(DRM_CLASS)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("renderD"))
        })
        .collect();
    nodes.sort();
    let primary = nodes
        .iter()
        .find(|node| is_boot_display(node))
        .or_else(|| nodes.first())?;
    let driver = fs::read_link(primary.join("device/driver")).ok()?;
    Some(driver.file_name()?.to_string_lossy().into_owned())
}

fn is_boot_display(node: &Path) -> bool {
    fs::read_to_string(node.join("device/boot_vga")).is_ok_and(|value| value.trim() == "1")
}
