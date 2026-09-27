#[cfg(any(windows, target_os = "macos"))]
mod accel;
mod gpu_recovery;
pub(in crate::osr::host) mod guest_preview;
mod ingest;
mod present;
mod resize;
