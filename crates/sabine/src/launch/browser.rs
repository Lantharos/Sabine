use std::{ffi::OsString, path::PathBuf, process::Command};

pub(crate) const HOST_CONTROL_PREFIX: &str = "SABINE_HOST_CONTROL";

/// CEF features Sabine does not use. Printing stays in the browser process,
/// since Chromium's out-of-process print drivers expect Chromium's own print
/// dialog and crash the browser when Sabine's dialog prints.
const DISABLED_CEF_FEATURES: &str = concat!(
    "EnableOopPrintDrivers,",
    "OptimizationGuideOnDeviceModel,",
    "AutofillServerCommunication,",
    "MediaRouter,",
    "Translate,",
    "InterestFeedContentSuggestions,",
    "SpareRendererForSitePerProcess"
);

const ON_DEVICE_MODEL_GPU_BLOCKED_PERFORMANCE_CLASS: u8 = 8;

/// With Wayland's fractional-scale protocol, Chromium takes each view's scale from its native
/// window. Off-screen views have none, so pages would always render at 1x and never match the
/// window; without it Chromium uses the scale Sabine reports for the view.
#[cfg(target_os = "linux")]
const LINUX_DISABLED_CEF_FEATURES: &str = "WaylandFractionalScaleV1";

/// Chromium's GTK 4 integration crashes the browser process when the desktop's settings portal
/// is unavailable; GTK 3 reads the desktop settings itself.
#[cfg(target_os = "linux")]
const LINUX_GTK_VERSION: &str = "--gtk-version=3";

/// Mesa's radeonsi gives the buffers Chromium hands each frame over in a layout with delta color
/// compression, which Chromium can't use, so on AMD graphics no frame ever reaches the window.
#[cfg(target_os = "linux")]
const LINUX_AMD_DEBUG_NO_DCC: &str = "nodcc";

/// The folder Chromium serves its own Wayland display in: the app's runtime
/// folder, or the shared runtime's.
#[cfg(target_os = "linux")]
pub(crate) const DISPLAY_DIR_ENV: &str = "SABINE_DISPLAY_DIR";

const DEFAULT_REMOTE_DEVTOOLS_PORT: u16 = 9222;
const DEVTOOLS_PORT_ENV: &str = "SABINE_DEVTOOLS_PORT";
/// A file for the browser process's log; sandboxed Chromium processes keep logging to the window
/// host's log.
const CHROMIUM_LOG_ENV: &str = "SABINE_CHROMIUM_LOG";

/// Browser-process launch options (devtools, hardware decode, and related flags).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BrowserOptions {
    pub remote_devtools_port: Option<u16>,
    /// Where the browser sends crash reports, which it otherwise only keeps.
    pub crash_upload_url: Option<String>,
    #[cfg(target_os = "linux")]
    pub vaapi_hardware_decode: bool,
    /// Runs the app's pages in a Chromium of its own instead of the runtime
    /// Sabine apps share.
    #[cfg(target_os = "linux")]
    pub separate_runtime: bool,
    #[cfg(target_os = "linux")]
    pub password_store: PasswordStore,
}

/// Where Chromium keeps the key it encrypts cookies and saved passwords with
/// on Linux.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PasswordStore {
    /// The person's keyring: KWallet on KDE, which Chromium picks there
    /// itself, and the Secret Service everywhere else, where Chromium would
    /// otherwise fall back to a fixed key.
    #[default]
    Keyring,
    /// Chromium's fixed key, for an app confined without the Secret Service.
    Basic,
}

#[cfg(target_os = "linux")]
impl PasswordStore {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "keyring" => Some(Self::Keyring),
            "basic" => Some(Self::Basic),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Keyring => "keyring",
            Self::Basic => "basic",
        }
    }

    fn switch(self) -> Option<&'static str> {
        match self {
            Self::Basic => Some("--password-store=basic"),
            Self::Keyring if desktop_is_kde() => None,
            Self::Keyring => Some("--password-store=gnome-libsecret"),
        }
    }
}

#[cfg(target_os = "linux")]
fn desktop_is_kde() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktops| {
        desktops
            .split(':')
            .any(|desktop| desktop.eq_ignore_ascii_case("KDE"))
    })
}

impl BrowserOptions {
    /// Development runs take their port from `SABINE_DEVTOOLS_PORT` first, so
    /// several apps can be debugged at once; `0` lets Chromium pick a free one.
    pub fn effective_remote_devtools_port(&self, dev_mode: bool) -> Option<u16> {
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
    #[cfg(target_os = "linux")]
    command.arg(format!(
        "--disable-features={DISABLED_CEF_FEATURES},{LINUX_DISABLED_CEF_FEATURES}"
    ));
    #[cfg(not(target_os = "linux"))]
    command.arg(format!("--disable-features={DISABLED_CEF_FEATURES}"));
    command
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
        .arg("--metrics-recording-only")
        .arg("--no-default-browser-check")
        .arg("--no-first-run")
        .arg(format!(
            "--sabine-accept-languages={}",
            super::languages::accept_languages(sys_locale::get_locales())
        ));
    #[cfg(target_os = "linux")]
    command.args(options.password_store.switch());
    #[cfg(target_os = "macos")]
    command.arg("--use-mock-keychain");
    if let Some(port) = options.effective_remote_devtools_port(dev_mode) {
        command
            .arg(format!("--remote-debugging-port={port}"))
            .arg("--remote-allow-origins=devtools://devtools");
    }
    if let Some(path) = std::env::var_os(CHROMIUM_LOG_ENV).filter(|path| !path.is_empty()) {
        let mut log_file = std::ffi::OsString::from("--log-file=");
        log_file.push(path);
        command
            .arg("--enable-logging")
            .arg(log_file)
            .arg("--log-severity=info");
    }
}

/// The environment that has a browser keep its crash reports in `folder`,
/// within Sabine's limits, and send them where the app asks.
pub(crate) fn crash_environment(
    folder: PathBuf,
    upload_url: Option<&str>,
) -> Vec<(&'static str, OsString)> {
    let reports = crate::diagnostics::CrashReports::at(folder);
    if let Err(error) = std::fs::create_dir_all(reports.folder()).and_then(|()| reports.prune()) {
        sabine_runtime::report_error(
            "crash",
            format!(
                "could not tidy the crash reports in {}: {error}",
                reports.folder().display()
            ),
        );
    }
    let mut environment = vec![(
        sabine_host::CRASH_FOLDER_ENV,
        reports.folder().as_os_str().to_owned(),
    )];
    if let Some(url) = upload_url {
        environment.push((sabine_host::CRASH_UPLOAD_ENV, url.into()));
    }
    environment
}

/// `AMD_DEBUG` for Chromium, keeping whatever the app was started with.
#[cfg(target_os = "linux")]
pub(crate) fn chromium_amd_debug() -> String {
    amd_debug_without_dcc(std::env::var("AMD_DEBUG").ok().as_deref())
}

#[cfg(target_os = "linux")]
fn amd_debug_without_dcc(existing: Option<&str>) -> String {
    match existing
        .map(str::trim)
        .filter(|existing| !existing.is_empty())
    {
        Some(existing)
            if existing
                .split(',')
                .any(|option| option.trim() == LINUX_AMD_DEBUG_NO_DCC) =>
        {
            existing.to_string()
        }
        Some(existing) => format!("{existing},{LINUX_AMD_DEBUG_NO_DCC}"),
        None => LINUX_AMD_DEBUG_NO_DCC.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    #[cfg(target_os = "linux")]
    use super::amd_debug_without_dcc;
    use super::{BrowserOptions, apply_browser_launch_args};

    fn switches(options: &BrowserOptions) -> Vec<String> {
        let mut command = Command::new("sabine-host");
        apply_browser_launch_args(&mut command, options, false);
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn production_opens_remote_debugging_only_when_the_app_asks() {
        assert!(
            !switches(&BrowserOptions::default())
                .iter()
                .any(|switch| switch.starts_with("--remote-"))
        );
        let debugged = switches(&BrowserOptions {
            remote_devtools_port: Some(9333),
            crash_upload_url: None,
            #[cfg(target_os = "linux")]
            vaapi_hardware_decode: false,
            #[cfg(target_os = "linux")]
            separate_runtime: false,
            #[cfg(target_os = "linux")]
            password_store: super::PasswordStore::Keyring,
        });
        assert!(debugged.contains(&"--remote-debugging-port=9333".to_string()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn turns_off_dcc_for_chromium() {
        assert_eq!(amd_debug_without_dcc(None), "nodcc");
        assert_eq!(amd_debug_without_dcc(Some(" ")), "nodcc");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn keeps_the_app_s_amd_debug_options() {
        assert_eq!(amd_debug_without_dcc(Some("nohyperz")), "nohyperz,nodcc");
        assert_eq!(
            amd_debug_without_dcc(Some("nodcc,nohyperz")),
            "nodcc,nohyperz"
        );
    }
}
