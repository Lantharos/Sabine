use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::bridge::BridgeEventEmitter;
use crate::host::PrimaryWindow;
#[cfg(target_os = "linux")]
use crate::host::ld_library_path;
use crate::host::{ManagedChild, browser_profile_dir, prepare_child_command};
use crate::launch::metrics::LaunchMetrics;
use crate::osr::transport::IpcEndpoint;
use crate::window::config::SabineWindowConfig;
use crate::{
    SabineError, SabineProcess, SabineResult, prepare_bridge_command, spawn_bridge_dispatch,
};
use sabine_bridge::{BridgeHandlers, BridgeRuntime};

pub(crate) const OSR_HOST_ARG: &str = "--sabine-osr-host";

pub(crate) fn run_from_args(args: &[String]) -> bool {
    let Some(index) = args.iter().position(|arg| arg == OSR_HOST_ARG) else {
        return false;
    };
    let Some(config_path) = args.get(index + 1).map(PathBuf::from) else {
        sabine_runtime::report_error(
            "window",
            "the window host was started without its configuration",
        );
        std::process::exit(1);
    };
    if let Err(error) = crate::osr::host::run(config_path) {
        crate::launch::bootstrap::show_failure("The application could not continue", &error);
        std::process::exit(1);
    }
    true
}

pub(crate) fn require_app_id(config: &SabineWindowConfig) -> SabineResult<&str> {
    let app_id = config.app_id.as_deref().map(str::trim).unwrap_or_default();
    sabine_service::valid_app_id(app_id)
        .then_some(app_id)
        .ok_or_else(|| SabineError::CreationFailed {
            message: "app_id is required and may contain only lowercase letters, digits, dots, and hyphens"
                .to_string(),
        })
}

pub(crate) fn launch_process(
    runtime_dir: &Path,
    config: &SabineWindowConfig,
    bridge_handlers: &BridgeHandlers,
    url: &str,
    metrics: LaunchMetrics,
) -> SabineResult<SabineProcess> {
    require_app_id(config)?;
    let runtime_lease = sabine_runtime::RuntimeLease::acquire(runtime_dir).map_err(|error| {
        SabineError::CreationFailed {
            message: format!("failed to lease Sabine runtime: {error}"),
        }
    })?;
    let host_binary = crate::host::ensure_host(runtime_dir)
        .map_err(|message| SabineError::CreationFailed { message })?;
    metrics.mark("host.ready");
    let (child_exit_sender, child_exit_receiver) = crossbeam_channel::unbounded();
    let (command_sender, command_receiver) = crossbeam_channel::unbounded();
    let mut process = SabineProcess {
        _runtime_lease: runtime_lease,
        primary: None,
        primary_status: None,
        background: config.background,
        extra_windows: Vec::new(),
        child_exit_sender,
        child_exit_receiver,
        command_sender,
        command_receiver,
        bridge_threads: Vec::new(),
        bridge_emitter: BridgeEventEmitter::default(),
        desktop_services: None,
        open_urls: config.open_urls.clone(),
        desktop_event_thread: None,
        desktop_event_stop: None,
        activity: sabine_bridge::ActivityRegistry::default(),
        metrics,
        windows: WindowContext {
            runtime_dir: runtime_dir.to_path_buf(),
            host_binary,
            bridge_handlers: bridge_handlers.clone(),
            primary: config.clone(),
            url: url.to_string(),
        },
    };
    if !config.background {
        open_primary_window(&mut process, launch_activation_token())?;
        if let Some(id) = process.id() {
            process.metrics.mark(format!("osr_host.spawned.pid.{id}"));
        }
    }
    Ok(process)
}

#[cfg(target_os = "linux")]
fn launch_activation_token() -> Option<String> {
    crate::desktop::startup_activation_token()
}

#[cfg(not(target_os = "linux"))]
fn launch_activation_token() -> Option<String> {
    None
}

pub(crate) struct WindowContext {
    pub(crate) runtime_dir: PathBuf,
    pub(crate) host_binary: PathBuf,
    pub(crate) bridge_handlers: BridgeHandlers,
    pub(crate) primary: SabineWindowConfig,
    pub(crate) url: String,
}

/// Opens the window the app launches with, again after it closed when the app
/// runs in the background.
pub(crate) fn open_primary_window(
    process: &mut SabineProcess,
    activation_token: Option<String>,
) -> SabineResult<()> {
    let config = process.windows.primary.clone();
    let url = process.windows.url.clone();
    let (child, ready) = spawn_window(process, &config, &url, activation_token)?;
    process.primary = Some(PrimaryWindow {
        child,
        ready,
        is_ready: false,
    });
    Ok(())
}

pub(crate) fn attach_open_window(
    process: &mut SabineProcess,
    config: &SabineWindowConfig,
    url: &str,
) -> SabineResult<u32> {
    let mut window_config = config.clone();
    window_config.app_id = process.windows.primary.app_id.clone();
    window_config.bridge = process.windows.primary.bridge.clone();
    let (child, _) = spawn_window(process, &window_config, url, None)?;
    let window_id = child.id();
    process.extra_windows.push(child);
    Ok(window_id)
}

fn spawn_window(
    process: &mut SabineProcess,
    config: &SabineWindowConfig,
    url: &str,
    activation_token: Option<String>,
) -> SabineResult<(ManagedChild, crossbeam_channel::Receiver<()>)> {
    let context = &process.windows;
    let mut child = spawn_osr_host_child(
        &context.runtime_dir,
        &context.host_binary,
        config,
        url,
        activation_token,
    )?;
    let dispatch = spawn_bridge_dispatch(
        &mut child,
        BridgeRuntime::new(context.bridge_handlers.clone(), config.bridge.clone()),
        process.activity.clone(),
        &process.bridge_emitter,
        config.listeners.clone(),
    );
    process
        .bridge_threads
        .retain(|thread| !thread.is_finished());
    process.bridge_threads.push(dispatch.thread);
    let child = ManagedChild::new(child, process.child_exit_sender.clone()).map_err(|error| {
        SabineError::CreationFailed {
            message: format!("failed to own OSR process: {error}"),
        }
    })?;
    Ok((child, dispatch.ready))
}

/// Only a window someone asked for carries an activation token: the one the
/// launch opens, or the one an activation opens for a background app.
pub(crate) fn spawn_osr_host_child(
    runtime_dir: &Path,
    host_binary: &Path,
    config: &SabineWindowConfig,
    url: &str,
    activation_token: Option<String>,
) -> SabineResult<std::process::Child> {
    let _ = require_app_id(config)?;
    let host_config_path =
        std::env::temp_dir().join(format!("sabine-osr-{}.json", osr_instance_key()));
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut body = serde_json::json!({
        "runtime_dir": runtime_dir,
        "host_binary": host_binary,
        "url": url,
        "web_root": config.web_root()?,
        "local_files": config.local_files,
        "app_id": config.app_id,
        "title": config.title,
        "width": config.width,
        "height": config.height,
        "min_width": config.min_width,
        "min_height": config.min_height,
        "resizable": config.resizable,
        "visible": config.visible,
        "active": config.active,
        "hide_on_blur": config.hide_on_blur,
        "hide_on_close": config.hide_on_close,
        "skip_taskbar": config.skip_taskbar,
        "always_on_top": config.always_on_top,
        "transparent": config.transparent,
        "background_color": config.background_color.to_rgba8(),
        "splash_delay_ms": config.splash.map(|delay| delay.as_millis() as u64),
        "background_effect": config.background_effect.as_str(),
        "chrome": config.chrome.as_str(),
        "titlebar_overlay": config.titlebar_overlay,
        "bridge_policy": {
            "enabled": true,
            "documentPrefix": if url.starts_with(crate::launch::APP_URL_PREFIX) { crate::launch::APP_URL_PREFIX } else { "" },
            "origins": if config.security.remote_content { config.security.allowed_origins.clone() } else { Vec::new() },
            "commandOrigins": config.bridge.descriptors().iter()
                .filter(|descriptor| !descriptor.allowed_origins.is_empty())
                .map(|descriptor| (descriptor.name.clone(), serde_json::json!(descriptor.allowed_origins)))
                .collect::<serde_json::Map<String, serde_json::Value>>(),
            "commands": sabine_bridge::page_commands(config.bridge.commands(), config.native_media()),
            "bytes": true,
        },
        "regions": crate::osr::protocol::regions_to_json(&config.regions),
        "drag_regions": crate::osr::protocol::rects_to_json(&config.drag_regions),
        "drag_exclusion_regions": crate::osr::protocol::rects_to_json(&config.drag_exclusion_regions),
        "control_regions": crate::osr::protocol::control_regions_to_json(&config.control_regions),
        "lifecycle": crate::osr::protocol::lifecycle_to_json(&config.lifecycle),
        "dev_mode": config.dev_mode(),
        "remote_devtools_port": config.effective_remote_devtools_port(),
        "remote_devtools_disabled": config.browser.remote_devtools_disabled,
    });
    #[cfg(target_os = "linux")]
    {
        body["vaapi_hardware_decode"] = config.browser.vaapi_hardware_decode.into();
        body["activation_token"] = activation_token.into();
        body["shared_runtime"] = config.browser.shares_runtime(config.dev_mode()).into();
    }
    #[cfg(not(target_os = "linux"))]
    let _ = activation_token;
    std::fs::write(&host_config_path, body.to_string()).map_err(|error| {
        SabineError::CreationFailed {
            message: format!("failed to write Sabine OSR host config: {error}"),
        }
    })?;

    let exe = std::env::current_exe().map_err(|error| SabineError::CreationFailed {
        message: error.to_string(),
    })?;
    let mut command = Command::new(exe);
    sabine_runtime::configure_background_command(&mut command);
    command
        .arg(OSR_HOST_ARG)
        .arg(&host_config_path)
        .env_remove("XDG_ACTIVATION_TOKEN")
        .env_remove("DESKTOP_STARTUP_ID")
        .stderr(Stdio::piped());
    prepare_bridge_command(&mut command);
    prepare_child_command(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| SabineError::CreationFailed {
            message: format!("failed to launch Sabine OSR host: {error}"),
        })?;
    sabine_runtime::capture_diagnostics(&mut child, "osr");
    Ok(child)
}

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
    let own = own_browser_command(runtime_dir, &host_binary, &binary_dir, config, &window)?;
    Ok(BrowserLaunch {
        own,
        #[cfg(target_os = "linux")]
        shared,
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

/// A Chromium of the window's own, sharing its profile only with the app's
/// other windows.
fn own_browser_command(
    runtime_dir: &Path,
    host_binary: &Path,
    binary_dir: &Path,
    config: &crate::osr::host::OsrHostConfig,
    window: &[OsString],
) -> Result<Command, String> {
    let cache_dir = browser_profile_dir(&browser_profile_key(config));
    let cache_dir = std::fs::create_dir_all(&cache_dir)
        .and_then(|()| dunce::canonicalize(&cache_dir))
        .map_err(|error| {
            format!(
                "could not create Sabine browser profile {}: {error}",
                cache_dir.display()
            )
        })?;
    let mut command = Command::new(host_binary);
    sabine_runtime::configure_background_command(&mut command);
    sabine_host::apply_runtime_resource_args(&mut command, runtime_dir);
    command
        .args(window)
        .arg(format!("--root-cache-path={}", cache_dir.display()))
        .arg(format!(
            "--cache-path={}",
            cache_dir.join("browser").display()
        ));
    crate::apply_browser_launch_args(&mut command, &config.browser_options(), config.dev_mode);
    crate::host::prepare_detachable_child_command(&mut command);
    command.current_dir(binary_dir);
    #[cfg(target_os = "linux")]
    {
        command.env("LD_LIBRARY_PATH", ld_library_path(binary_dir));
        command.env("AMD_DEBUG", crate::launch::browser::chromium_amd_debug());
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

fn osr_instance_key() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}
