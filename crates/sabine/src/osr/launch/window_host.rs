use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use sabine_bridge::{BridgeHandlers, BridgeRuntime};

use super::{OSR_HOST_ARG, require_app_id};
use crate::bridge::{BridgeEventEmitter, prepare_bridge_command, spawn_bridge_dispatch};
use crate::host::{ManagedChild, PrimaryWindow, ProcessCommand, SabineProcessHandle, spawn_child};
use crate::launch::metrics::LaunchMetrics;
use crate::window::config::SabineWindowConfig;
use crate::{SabineError, SabineProcess, SabineResult};

pub(crate) fn launch_process(
    runtime_dir: &Path,
    config: &SabineWindowConfig,
    bridge_handlers: &BridgeHandlers,
    url: &str,
    (handle, command_receiver): (
        SabineProcessHandle,
        crossbeam_channel::Receiver<ProcessCommand>,
    ),
    bridge_emitter: BridgeEventEmitter,
    metrics: LaunchMetrics,
) -> SabineResult<SabineProcess> {
    let app_id = require_app_id(config)?;
    let runtime_lease = sabine_runtime::RuntimeLease::acquire(runtime_dir).map_err(|error| {
        SabineError::CreationFailed {
            message: format!("failed to lease Sabine runtime: {error}"),
        }
    })?;
    let host_binary = crate::host::app_host(config, runtime_dir)
        .map_err(|message| SabineError::CreationFailed { message })?;
    #[cfg(target_os = "linux")]
    let installation = crate::host::Installation::hold(config);
    #[cfg(target_os = "linux")]
    let confinement = crate::launch::confine::apply(config, runtime_dir, &host_binary)?;
    metrics.mark("host.ready");
    let (child_exit_sender, child_exit_receiver) = crossbeam_channel::unbounded();
    let mut process = SabineProcess {
        _runtime_lease: runtime_lease,
        primary: None,
        primary_status: None,
        background: config.background,
        extra_windows: Vec::new(),
        pending_windows: Vec::new(),
        child_exit_sender,
        child_exit_receiver,
        command_receiver,
        handle,
        quitting: false,
        bridge_threads: Vec::new(),
        bridge_emitter,
        desktop_services: None,
        open_urls: config.listeners.open_urls.clone(),
        desktop_event_thread: None,
        desktop_event_stop: None,
        activity: sabine_bridge::ActivityRegistry::keeping_awake(crate::desktop::keep_awake),
        idle: crate::desktop::IdleWatches::default(),
        metrics,
        windows: WindowContext {
            runtime_dir: runtime_dir.to_path_buf(),
            host_binary,
            bridge_handlers: bridge_handlers.clone(),
            primary: config.clone(),
            url: url.to_string(),
            restoring: restoring_window(app_id),
        },
        #[cfg(target_os = "linux")]
        installation,
        #[cfg(target_os = "linux")]
        _confinement: confinement,
    };
    if !config.app_menu.is_empty() {
        process.bridge_emitter.set_app_menu(&config.app_menu);
    }
    if !config.background {
        open_primary_window(&mut process, launch_activation_token())?;
        if let Some(id) = process.id() {
            process.metrics.mark(format!("osr_host.spawned.pid.{id}"));
        }
    }
    Ok(process)
}

#[cfg(target_os = "linux")]
fn restoring_window(app_id: &str) -> Option<String> {
    crate::window::restore::launched(app_id)
}

#[cfg(not(target_os = "linux"))]
fn restoring_window(_app_id: &str) -> Option<String> {
    None
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
    /// The key of the window the desktop asked the first window to open as
    /// again, until that window opens.
    pub(crate) restoring: Option<String>,
}

/// Opens the window the app launches with, again after it closed when the app
/// runs in the background.
pub(crate) fn open_primary_window(
    process: &mut SabineProcess,
    activation_token: Option<String>,
) -> SabineResult<()> {
    let config = process.windows.primary.clone();
    let url = process.windows.url.clone();
    let restoring = process.windows.restoring.take();
    let handlers = process.windows.bridge_handlers.clone();
    let (child, ready) = spawn_window(
        process,
        &config,
        handlers,
        &url,
        activation_token,
        restoring,
    )?;
    process.primary = Some(PrimaryWindow {
        child,
        ready,
        is_ready: false,
    });
    Ok(())
}

/// Opens another window with the first window's app id and commands, and the
/// commands of its own, which answer in place of the first window's for a
/// command both register.
pub(crate) fn attach_open_window(
    process: &mut SabineProcess,
    config: &SabineWindowConfig,
    own_handlers: BridgeHandlers,
    url: &str,
) -> SabineResult<u32> {
    let mut window_config = config.clone();
    window_config.app_id = process.windows.primary.app_id.clone();
    for descriptor in process.windows.primary.bridge.descriptors() {
        window_config.bridge.register_descriptor(descriptor.clone());
    }
    window_config.listeners.open_urls = process.open_urls.clone();
    window_config.logging = process.windows.primary.logging.clone();
    window_config.listeners.desktop_events =
        process.windows.primary.listeners.desktop_events.clone();
    let mut handlers = process.windows.bridge_handlers.clone();
    handlers.extend(own_handlers);
    let (child, _) = spawn_window(process, &window_config, handlers, url, None, None)?;
    let window_id = child.id();
    process.extra_windows.push(child);
    Ok(window_id)
}

fn spawn_window(
    process: &mut SabineProcess,
    config: &SabineWindowConfig,
    handlers: BridgeHandlers,
    url: &str,
    activation_token: Option<String>,
    restoring: Option<String>,
) -> SabineResult<(ManagedChild, crossbeam_channel::Receiver<()>)> {
    let context = &process.windows;
    let mut child = spawn_osr_host_child(
        &context.runtime_dir,
        &context.host_binary,
        config,
        url,
        activation_token,
        restoring,
    )?;
    let dispatch = spawn_bridge_dispatch(
        &mut child,
        BridgeRuntime::new(handlers),
        process.activity.clone(),
        process.idle.clone(),
        &process.bridge_emitter,
        config.listeners.clone(),
        process.handle.clone(),
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
/// launch opens, or the one an activation opens for a background app. Only
/// the window the launch opens may open as a window the desktop restores.
fn spawn_osr_host_child(
    runtime_dir: &Path,
    host_binary: &Path,
    config: &SabineWindowConfig,
    url: &str,
    activation_token: Option<String>,
    restoring: Option<String>,
) -> SabineResult<std::process::Child> {
    let app_id = require_app_id(config)?;
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
        "max_size": config.max_size,
        "maximized": config.maximized,
        "remember_size": config.remember_size,
        "resizable": config.resizable,
        "visible": config.visible,
        "active": config.active,
        "hide_on_blur": config.hide_on_blur,
        "hide_on_close": config.hide_on_close,
        "asks_before_closing": config.listeners.close_request.is_some(),
        "asks_permissions": config.listeners.permission.is_some(),
        "skip_taskbar": config.skip_taskbar,
        "always_on_top": config.always_on_top,
        "transparent": config.transparent,
        "background_color": config.background_color.to_rgba8(),
        "splash_delay_ms": config.splash.map(|delay| delay.as_millis() as u64),
        "background_effect": config.background_effect.as_str(),
        "backdrop_role": config.backdrop.role.as_str(),
        "backdrop_preference": config.backdrop.preference.as_str(),
        "chrome": config.chrome.as_str(),
        "titlebar_overlay": config.titlebar_overlay,
        "bridge_policy": bridge_policy(config, url),
        "regions": crate::osr::protocol::regions_to_json(&config.regions),
        "drag_regions": crate::osr::protocol::rects_to_json(&config.drag_regions),
        "drag_exclusion_regions": crate::osr::protocol::rects_to_json(&config.drag_exclusion_regions),
        "control_regions": crate::osr::protocol::control_regions_to_json(&config.control_regions),
        "lifecycle": crate::osr::protocol::lifecycle_to_json(&config.lifecycle),
        "dev_mode": config.dev_mode(),
        "remote_devtools_port": config.effective_remote_devtools_port(),
        "crash_upload_url": if config.dev_mode() { None } else { config.crash_upload_url.as_deref() },
    });
    #[cfg(target_os = "linux")]
    {
        body["vaapi_hardware_decode"] = config.browser.vaapi_hardware_decode.into();
        body["activation_token"] = activation_token.into();
        body["shared_runtime"] = config.shares_runtime().into();
        body["password_store"] = config.password_store().as_str().into();
        body["parent_window"] = config.parent_window.clone().into();
        body["modal"] = config.modal.into();
        body["restoring"] = restoring.into();
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (activation_token, restoring, app_id);
    let mut command = window_host_command()?;
    sabine_runtime::configure_background_command(&mut command);
    command
        .arg(OSR_HOST_ARG)
        .env_remove("XDG_ACTIVATION_TOKEN")
        .env_remove("DESKTOP_STARTUP_ID")
        .stderr(Stdio::piped());
    #[cfg(target_os = "linux")]
    command
        .env_remove(crate::window::restore::RESTORE_WINDOWS)
        .envs(crate::launch::sound::app_environment(
            app_id,
            config.app_name.as_deref().unwrap_or(app_id),
            config.sound_role,
        ));
    prepare_bridge_command(&mut command);
    let mut child = spawn_child(command).map_err(|error| SabineError::CreationFailed {
        message: format!("failed to launch Sabine OSR host: {error}"),
    })?;
    sabine_runtime::capture_diagnostics(&mut child, "osr");
    let mut message = body.to_string();
    message.push('\n');
    child
        .stdin
        .as_mut()
        .expect("the window host's input is piped")
        .write_all(message.as_bytes())
        .map_err(|error| SabineError::CreationFailed {
            message: format!("failed to configure Sabine OSR host: {error}"),
        })?;
    Ok(child)
}

/// What the window's pages may do: which documents get the bridge, the commands they
/// call, the permissions they may ask for, and how their downloads and spellcheck go.
fn bridge_policy(config: &SabineWindowConfig, url: &str) -> serde_json::Value {
    serde_json::json!({
        "enabled": true,
        "documentPrefix": if url.starts_with(crate::launch::APP_URL_PREFIX) { crate::launch::APP_URL_PREFIX } else { "" },
        "origins": if config.security.remote_content { config.security.allowed_origins.clone() } else { Vec::new() },
        "commandOrigins": config.bridge.descriptors().iter()
            .filter(|descriptor| !descriptor.allowed_origins.is_empty())
            .map(|descriptor| (descriptor.name.clone(), serde_json::json!(descriptor.allowed_origins)))
            .collect::<serde_json::Map<String, serde_json::Value>>(),
        "commands": sabine_bridge::page_commands(config.bridge.commands(), config.native_media()),
        "bytes": true,
        "consoleLog": config.logs_page_console(),
        "permissions": config.permission_names(),
        "downloads": {
            "decide": config.listeners.downloads.decide.is_some(),
            "report": config.listeners.downloads.listens(),
        },
        "spellcheck": config.spellcheck.to_json(),
    })
}

/// The app's own program as a window host. On Linux it runs from the file
/// this process runs, which stays the same program when an update replaced
/// the app's files meanwhile.
#[cfg(target_os = "linux")]
fn window_host_command() -> SabineResult<Command> {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new("/proc/self/exe");
    if let Ok(program) = std::env::current_exe() {
        command.arg0(program);
    }
    Ok(command)
}

#[cfg(not(target_os = "linux"))]
fn window_host_command() -> SabineResult<Command> {
    std::env::current_exe()
        .map(Command::new)
        .map_err(|error| SabineError::CreationFailed {
            message: error.to_string(),
        })
}
