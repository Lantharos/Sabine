use std::io::BufRead;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sabine_platform::{WindowBackgroundEffect, WindowRegionRect, WindowRegions};

use crate::osr::protocol::{
    control_regions_from_json, lifecycle_from_json, rects_from_json, regions_from_json,
};
use crate::window::style::Color;
use crate::{SabineLifecyclePolicy, SabineWindowChrome, SabineWindowControlRegion};

#[derive(Clone, Debug)]
pub(crate) struct OsrHostConfig {
    pub runtime_dir: PathBuf,
    pub host_binary: PathBuf,
    pub url: String,
    pub web_root: Option<PathBuf>,
    pub local_files: bool,
    pub app_id: Option<String>,
    pub title: Arc<str>,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub maximized: bool,
    pub remember_size: bool,
    pub resizable: bool,
    pub visible: bool,
    pub active: bool,
    pub hide_on_blur: bool,
    pub hide_on_close: bool,
    /// Someone asking the window to close is the app's to answer.
    pub asks_before_closing: bool,
    pub skip_taskbar: bool,
    pub always_on_top: bool,
    pub transparent: bool,
    pub background_color: Color,
    pub splash: Option<Duration>,
    pub background_effect: WindowBackgroundEffect,
    pub chrome: SabineWindowChrome,
    #[cfg(target_os = "macos")]
    pub titlebar_overlay: bool,
    pub bridge_policy: serde_json::Value,
    pub regions: WindowRegions,
    pub drag_regions: Vec<WindowRegionRect>,
    pub drag_exclusion_regions: Vec<WindowRegionRect>,
    pub control_regions: Vec<SabineWindowControlRegion>,
    pub lifecycle: SabineLifecyclePolicy,
    pub dev_mode: bool,
    pub remote_devtools_port: Option<u16>,
    #[cfg(target_os = "linux")]
    pub vaapi_hardware_decode: bool,
    #[cfg(target_os = "linux")]
    pub activation_token: Option<String>,
    #[cfg(target_os = "linux")]
    pub shared_runtime: bool,
    #[cfg(target_os = "linux")]
    pub parent_window: Option<String>,
    #[cfg(target_os = "linux")]
    pub modal: bool,
    /// The key of the window the desktop opens again, for the window that
    /// opens as it.
    #[cfg(target_os = "linux")]
    pub restoring: Option<String>,
}

impl OsrHostConfig {
    pub(crate) fn browser_options(&self) -> crate::launch::browser::BrowserOptions {
        crate::launch::browser::BrowserOptions {
            remote_devtools_port: self.remote_devtools_port,
            #[cfg(target_os = "linux")]
            vaapi_hardware_decode: self.vaapi_hardware_decode,
            #[cfg(target_os = "linux")]
            separate_runtime: !self.shared_runtime,
        }
    }

    /// Reads the configuration the app sends first on the window host's input,
    /// ahead of the bridge messages that follow it on the same pipe.
    pub(super) fn read(input: &mut impl BufRead) -> Result<Self, String> {
        let mut line = String::new();
        input
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        let value: serde_json::Value =
            serde_json::from_str(&line).map_err(|error| error.to_string())?;
        Ok(Self {
            runtime_dir: path_value(&value, "runtime_dir")?,
            host_binary: path_value(&value, "host_binary")?,
            url: string_value(&value, "url")?,
            web_root: value
                .get("web_root")
                .and_then(serde_json::Value::as_str)
                .map(PathBuf::from),
            local_files: value
                .get("local_files")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            app_id: value
                .get("app_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            title: value
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Sabine")
                .into(),
            width: value
                .get("width")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(900) as u32,
            height: value
                .get("height")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(640) as u32,
            min_width: value
                .get("min_width")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(420) as u32,
            min_height: value
                .get("min_height")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(280) as u32,
            maximized: value
                .get("maximized")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            remember_size: value
                .get("remember_size")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            resizable: value
                .get("resizable")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            visible: value
                .get("visible")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            active: value
                .get("active")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            hide_on_blur: value
                .get("hide_on_blur")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            hide_on_close: value
                .get("hide_on_close")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            asks_before_closing: value
                .get("asks_before_closing")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            skip_taskbar: value
                .get("skip_taskbar")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            always_on_top: value
                .get("always_on_top")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            transparent: value
                .get("transparent")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            background_color: color_value(&value, "background_color")?,
            splash: value
                .get("splash_delay_ms")
                .and_then(serde_json::Value::as_u64)
                .map(Duration::from_millis),
            background_effect: value
                .get("background_effect")
                .and_then(serde_json::Value::as_str)
                .and_then(WindowBackgroundEffect::parse)
                .unwrap_or(WindowBackgroundEffect::None),
            chrome: value
                .get("chrome")
                .and_then(serde_json::Value::as_str)
                .and_then(SabineWindowChrome::parse)
                .unwrap_or(SabineWindowChrome::System),
            #[cfg(target_os = "macos")]
            titlebar_overlay: value
                .get("titlebar_overlay")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            bridge_policy: value
                .get("bridge_policy")
                .cloned()
                .ok_or("OSR host config missing bridge policy")?,
            regions: regions_from_json(value.get("regions")),
            drag_regions: rects_from_json(value.get("drag_regions")),
            drag_exclusion_regions: rects_from_json(value.get("drag_exclusion_regions")),
            control_regions: control_regions_from_json(value.get("control_regions")),
            lifecycle: lifecycle_from_json(value.get("lifecycle")),
            dev_mode: value
                .get("dev_mode")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            remote_devtools_port: value
                .get("remote_devtools_port")
                .and_then(serde_json::Value::as_u64)
                .and_then(|port| u16::try_from(port).ok()),
            #[cfg(target_os = "linux")]
            vaapi_hardware_decode: value
                .get("vaapi_hardware_decode")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            #[cfg(target_os = "linux")]
            activation_token: value
                .get("activation_token")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            #[cfg(target_os = "linux")]
            shared_runtime: value
                .get("shared_runtime")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            #[cfg(target_os = "linux")]
            parent_window: value
                .get("parent_window")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            #[cfg(target_os = "linux")]
            modal: value
                .get("modal")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            #[cfg(target_os = "linux")]
            restoring: value
                .get("restoring")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        })
    }
}

fn color_value(value: &serde_json::Value, key: &str) -> Result<Color, String> {
    let components = value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("OSR host config missing {key}"))?;
    let mut rgba = [0_u8; 4];
    if components.len() != rgba.len() {
        return Err(format!("OSR host config {key} must contain four bytes"));
    }
    for (target, component) in rgba.iter_mut().zip(components) {
        let component = component
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| format!("OSR host config {key} contains an invalid byte"))?;
        *target = component;
    }
    Ok(Color::rgba8(rgba[0], rgba[1], rgba[2], rgba[3]))
}

pub(super) fn path_value(value: &serde_json::Value, key: &str) -> Result<PathBuf, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| format!("OSR host config missing {key}"))
}

pub(super) fn string_value(value: &serde_json::Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("OSR host config missing {key}"))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::OsrHostConfig;
    use crate::bridge::frame::Frame;

    #[test]
    fn reads_its_configuration_ahead_of_the_bridge_messages() {
        let config = serde_json::json!({
            "runtime_dir": "/runtime",
            "host_binary": "/runtime/sabine-host",
            "url": "sabine://app/index.html",
            "app_id": "com.example.notes",
            "background_color": [0, 0, 0, 255],
            "bridge_policy": { "enabled": true },
            "dev_mode": false,
        });
        let mut input = Cursor::new(format!("{config}\napp.ready\n"));

        let host = OsrHostConfig::read(&mut input).unwrap();

        assert_eq!(host.url, "sabine://app/index.html");
        assert_eq!(host.app_id.as_deref(), Some("com.example.notes"));
        assert!(!host.dev_mode);
        assert_eq!(host.remote_devtools_port, None);
        assert_eq!(
            Frame::read(&mut input).unwrap(),
            Some(Frame::line("app.ready"))
        );
    }
}
