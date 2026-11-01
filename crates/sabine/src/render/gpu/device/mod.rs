pub(in crate::render::gpu) mod health;
pub(in crate::render::gpu) mod instance;
#[cfg(target_os = "linux")]
mod producer_linux;
pub(in crate::render::gpu) mod retirement;

#[cfg(target_os = "linux")]
pub(in crate::render::gpu) use producer_linux::{ProducerWaits, open_device};
