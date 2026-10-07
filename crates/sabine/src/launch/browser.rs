use std::process::Command;

pub(crate) const HOST_CONTROL_PREFIX: &str = "SABINE_HOST_CONTROL";

/// CEF features Sabine does not use.
const DISABLED_CEF_FEATURES: &str = concat!(
    "OptimizationGuideOnDeviceModel,",
    "AutofillServerCommunication,",
    "MediaRouter,",
    "Translate,",
    "InterestFeedContentSuggestions,",
    "SpareRendererForSitePerProcess"
);

const ON_DEVICE_MODEL_GPU_BLOCKED_PERFORMANCE_CLASS: u8 = 8;

/// Chromium's GTK 4 integration crashes the browser process when the desktop's settings portal
/// is unavailable; GTK 3 reads the desktop settings itself.
#[cfg(target_os = "linux")]
const LINUX_GTK_VERSION: &str = "--gtk-version=3";

const DEFAULT_REMOTE_DEVTOOLS_PORT: u16 = 9222;
const DEVTOOLS_PORT_ENV: &str = "SABINE_DEVTOOLS_PORT";

/// Browser-process launch options (devtools, hardware decode, and related flags).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BrowserOptions {
    pub remote_devtools_port: Option<u16>,
    pub remote_devtools_disabled: bool,
    #[cfg(target_os = "linux")]
    pub vaapi_hardware_decode: bool,
}

impl BrowserOptions {
    /// Development runs take their port from `SABINE_DEVTOOLS_PORT` first, so
    /// several apps can be debugged at once; `0` lets Chromium pick a free one.
    pub fn effective_remote_devtools_port(&self, dev_mode: bool) -> Option<u16> {
        if self.remote_devtools_disabled {
            return None;
        }
        let from_environment = dev_mode
            .then(|| std::env::var(DEVTOOLS_PORT_ENV).ok()?.trim().parse().ok())
            .flatten();
        from_environment
            .or(self.remote_devtools_port)
            .or(dev_mode.then_some(DEFAULT_REMOTE_DEVTOOLS_PORT))
    }
}

pub(crate) fn apply_browser_launch_args(
    command: &mut Command,
    options: &BrowserOptions,
    dev_mode: bool,
) {
    let enabled_features: Vec<&str> = {
        #[cfg(target_os = "linux")]
        {
            let mut features = vec!["UseOzonePlatform"];
            command
                .arg("--ozone-platform=wayland")
                .arg(LINUX_GTK_VERSION);
            if options.vaapi_hardware_decode {
                features.push("VaapiVideoDecoder");
            }
            features
        }
        #[cfg(not(target_os = "linux"))]
        {
            Vec::new()
        }
    };
    if !enabled_features.is_empty() {
        command.arg(format!("--enable-features={}", enabled_features.join(",")));
    }
    command
        .arg(format!("--disable-features={DISABLED_CEF_FEATURES}"))
        .arg(format!(
            "--optimization-guide-performance-class={ON_DEVICE_MODEL_GPU_BLOCKED_PERFORMANCE_CLASS}"
        ))
        .arg("--disable-background-networking")
        .arg("--disable-component-update")
        .arg("--disable-component-extensions-with-background-pages")
        .arg("--disable-default-apps")
        .arg("--disable-domain-reliability")
        .arg("--disable-extensions")
        .arg("--disable-sync")
        .arg("--disable-breakpad")
        .arg("--metrics-recording-only")
        .arg("--no-default-browser-check")
        .arg("--no-first-run");
    #[cfg(target_os = "macos")]
    command.arg("--use-mock-keychain");
    if let Some(port) = options.effective_remote_devtools_port(dev_mode) {
        command.arg(format!("--remote-debugging-port={port}"));
    }
}
