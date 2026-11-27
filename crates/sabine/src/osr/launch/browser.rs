use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::host::browser_profile_dir;
#[cfg(target_os = "linux")]
use crate::host::ld_library_path;
use crate::osr::transport::IpcEndpoint;

pub(crate) struct CefViewport {
    pub(in crate::osr) page: crate::osr::host::viewport::PageViewport,
    pub(crate) frame_rate: u32,
    pub(crate) accelerated_paint: bool,
    pub(crate) parent_window: Option<u64>,
}

/// How a window's browser starts: in a Chromium of the window's own, or in
/// the runtime Sabine apps share.
pub(crate) struct BrowserLaunch {
    pub(crate) own: Command,
    #[cfg(target_os = "linux")]
    pub(crate) shared: Option<crate::launch::shared::SharedLaunch>,
    #[cfg(target_os = "linux")]
    pub(crate) handover: crate::launch::singleton::Handover,
}

pub(crate) fn browser_launch(
    runtime_dir: &Path,
    host_binary: &Path,
    endpoint: &IpcEndpoint,
    authentication_token: &str,
    config: &crate::osr::host::OsrHostConfig,
    viewport: CefViewport,
) -> Result<BrowserLaunch, String> {
    let host_binary = host_binary.canonicalize().map_err(|error| {
        format!(
            "could not resolve Sabine host {}: {error}",
            host_binary.display()
        )
    })?;
    sabine_runtime::prepare_runtime_assets(runtime_dir).map_err(|error| error.to_string())?;
    let host_binary = sabine_host::prepare_host_execution(&host_binary, runtime_dir)?;
    let binary_dir = sabine_host::runtime_binary_directory(runtime_dir);
    let token_file = crate::osr::transport::write_token_file(endpoint, authentication_token)
        .map_err(|error| format!("could not write Sabine OSR token file: {error}"))?;
    let window = window_switches(endpoint, &token_file, config, &viewport);
    #[cfg(target_os = "linux")]
    let shared = config
        .shared_runtime
        .then(|| {
            crate::launch::shared::SharedLaunch::new(
                &host_binary,
                runtime_dir,
                &binary_dir,
                &config.browser_options(),
                &browser_profile_key(config),
                &window,
            )
        })
        .flatten();
    let profile = own_browser_profile(config)?;
    let own = own_browser_command(
        runtime_dir,
        &host_binary,
        &binary_dir,
        &profile,
        config,
        &window,
    )?;
    Ok(BrowserLaunch {
        own,
        #[cfg(target_os = "linux")]
        shared,
        #[cfg(target_os = "linux")]
        handover: crate::launch::singleton::Handover {
            profile,
            program: host_binary,
            switches: window,
        },
    })
}

/// The switches of one window's browser, which apply to it alone.
fn window_switches(
    endpoint: &IpcEndpoint,
    token_file: &Path,
    config: &crate::osr::host::OsrHostConfig,
    viewport: &CefViewport,
) -> Vec<OsString> {
    let mut switches: Vec<OsString> = vec![
        format!("--url={}", config.url).into(),
        "--sabine-osr".into(),
        format!("--sabine-osr-endpoint={}", endpoint.argument()).into(),
        format!("--sabine-parent-pid={}", std::process::id()).into(),
    ];
    if viewport.accelerated_paint {
        switches.push("--sabine-shared-texture".into());
    }
    if let Some(parent) = viewport.parent_window {
        switches.push(format!("--sabine-parent-window={parent}").into());
    }
    switches.extend([
        format!("--sabine-osr-token-file={}", token_file.display()).into(),
        format!("--sabine-width={}", viewport.page.width).into(),
        format!("--sabine-height={}", viewport.page.height).into(),
        format!("--sabine-scale={}", viewport.page.scale_argument()).into(),
        format!("--sabine-bridge-policy={}", config.bridge_policy).into(),
        format!("--sabine-active-frame-rate={}", viewport.frame_rate.max(1)).into(),
        format!(
            "--sabine-background-frame-rate={}",
            config.lifecycle.background_frame_rate.max(1)
        )
        .into(),
        format!(
            "--sabine-background-color={}",
            config.background_color.to_opaque_argb_hex()
        )
        .into(),
    ]);
    if config.dev_mode {
        switches.push("--sabine-dev-mode".into());
    }
    if config.lifecycle.retain_hidden_frame {
        switches.push("--sabine-retain-hidden-frame".into());
    }
    if let Some(root) = &config.web_root {
        let mut argument = OsString::from("--sabine-app-root=");
        argument.push(root);
        switches.push(argument);
    }
    if config.local_files {
        switches.push("--sabine-local-files".into());
    }
    if config.transparent {
        switches.push("--sabine-transparent".into());
    }
    switches
}

/// The profile the app's windows share in a Chromium of their own.
fn own_browser_profile(config: &crate::osr::host::OsrHostConfig) -> Result<PathBuf, String> {
    let cache_dir = browser_profile_dir(&browser_profile_key(config));
    std::fs::create_dir_all(&cache_dir)
        .and_then(|()| dunce::canonicalize(&cache_dir))
        .map_err(|error| {
            format!(
                "could not create Sabine browser profile {}: {error}",
                cache_dir.display()
            )
        })
}

/// A Chromium of the window's own, sharing its profile only with the app's
/// other windows.
fn own_browser_command(
    runtime_dir: &Path,
    host_binary: &Path,
    binary_dir: &Path,
    cache_dir: &Path,
    config: &crate::osr::host::OsrHostConfig,
    window: &[OsString],
) -> Result<Command, String> {
    #[cfg(target_os = "linux")]
    let mut command = crate::launch::sandbox::browser_command(host_binary, binary_dir);
    #[cfg(not(target_os = "linux"))]
    let mut command = Command::new(host_binary);
    sabine_runtime::configure_background_command(&mut command);
    sabine_host::apply_runtime_resource_args(&mut command, runtime_dir);
    let options = config.browser_options();
    command
        .args(window)
        .envs(crate::launch::browser::crash_environment(
            crate::diagnostics::CrashReports::for_app(&browser_profile_key(config))
                .folder()
                .to_path_buf(),
            options.crash_upload_url.as_deref(),
        ))
        .arg(format!("--root-cache-path={}", cache_dir.display()))
        .arg(format!(
            "--cache-path={}",
            cache_dir.join("browser").display()
        ));
    crate::launch::browser::apply_browser_launch_args(&mut command, &options, config.dev_mode);
    crate::host::prepare_detachable_child_command(&mut command);
    command.current_dir(binary_dir);
    #[cfg(target_os = "linux")]
    {
        command.env("LD_LIBRARY_PATH", ld_library_path(binary_dir));
        command.env("AMD_DEBUG", crate::launch::browser::chromium_amd_debug());
        let display = crate::osr::transport::runtime_ipc_dir(&browser_profile_key(config))
            .map_err(|error| format!("could not find the app's runtime folder: {error}"))?;
        command.env(crate::launch::browser::DISPLAY_DIR_ENV, display);
    }
    #[cfg(target_os = "windows")]
    {
        let release = binary_dir.to_string_lossy();
        let path = std::env::var("PATH").unwrap_or_default();
        command.env(
            "PATH",
            if path.is_empty() {
                release.into_owned()
            } else {
                format!("{release};{path}")
            },
        );
    }
    command.arg(format!(
        "--default-background-color={}",
        if config.transparent {
            "0x00000000".to_string()
        } else {
            config.background_color.to_opaque_argb_hex()
        }
    ));
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::inherit());
    Ok(command)
}

fn browser_profile_key(config: &crate::osr::host::OsrHostConfig) -> String {
    config
        .app_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .expect("cef_osr_command requires a non-empty app_id")
        .to_string()
}
