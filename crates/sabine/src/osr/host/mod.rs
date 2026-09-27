mod config;
mod events;
mod input;
mod lifecycle;
mod native;
mod paint;
mod recovery;
mod socket;
pub(in crate::osr) mod types;
mod ui;
mod visibility;

use std::path::PathBuf;
use std::sync::mpsc;

use winit::event_loop::{EventLoop, run_on_demand::EventLoopExtRunOnDemand};
#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};

pub(crate) use config::OsrHostConfig;

use native::OsrNativeHost;

pub(crate) fn run(config_path: PathBuf) -> Result<(), String> {
    let config = OsrHostConfig::read(config_path)?;
    let mut event_loop_builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    if config.skip_taskbar {
        event_loop_builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    let mut event_loop = event_loop_builder
        .build()
        .map_err(|error| error.to_string())?;
    let proxy = event_loop.create_proxy();
    let (sender, receiver) = mpsc::sync_channel(8);
    let mut host = OsrNativeHost::new(config, sender, receiver, proxy);
    event_loop
        .run_app_on_demand(&mut host)
        .map_err(|error| error.to_string())?;
    host.failure.take().map_or(Ok(()), Err)
}

fn trace_host(config: &OsrHostConfig, stage: impl AsRef<str>) {
    let enabled = std::env::var(sabine_bridge::SABINE_TRACE_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "trace"
        )
    });
    if !enabled {
        return;
    }
    let label = config.app_id.as_deref().unwrap_or(&config.title);
    eprintln!(
        "sabine trace [{label}] osr-host pid={} {}",
        std::process::id(),
        stage.as_ref()
    );
}
