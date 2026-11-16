//! Holds an app outside an app sandbox to its `[sandbox]` declaration with
//! Landlock, a system call filter and a filtered session bus, which every
//! process it starts inherits.

mod accessibility;
mod bus;
pub mod documents;
mod paths;
mod seccomp;
mod sound;

use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use landlock::{
    Access, AccessFs, AccessNet, NetPort, PathBeneath, PathFd, RestrictSelfAttr, RestrictionStatus,
    Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetError, RulesetStatus, Scope,
};
use sabine_platform::SingleInstancePolicy;
use sabine_service::Sandbox;

use crate::error::{SabineError, SabineResult};
use crate::window::config::SabineWindowConfig;
use bus::SessionBus;
use paths::{LANDLOCK, Own, Paths, Runtime};
use sound::PlaybackServer;

const FULL_ACCESS: &str = "SABINE_FULL_ACCESS";

/// The ports an app that declares it stays offline still reaches in
/// development: its dev server and the DevTools port its Chromium opens.
struct Ports {
    connect: Option<u16>,
    bind: Option<u16>,
}

fn restrict(paths: Paths, ports: Option<Ports>) -> Result<RestrictionStatus, RulesetError> {
    let mut ruleset = Ruleset::default()
        .handle_access(AccessFs::from_all(LANDLOCK))?
        .scope(Scope::AbstractUnixSocket | Scope::Signal)?;
    if ports.is_some() {
        ruleset = ruleset.handle_access(AccessNet::from_all(LANDLOCK))?;
    }
    let rules = paths.0.into_iter().filter_map(|(path, access)| {
        let fd = PathFd::new(path).ok()?;
        Some(Ok::<_, RulesetError>(PathBeneath::new(fd, access)))
    });
    let mut ruleset = ruleset.create()?.add_rules(rules)?;
    if let Some(ports) = ports {
        let reached = [
            (ports.connect, AccessNet::ConnectTcp),
            (ports.bind, AccessNet::BindTcp),
        ];
        ruleset = ruleset.add_rules(reached.into_iter().filter_map(|(port, access)| {
            Some(Ok::<_, RulesetError>(NetPort::new(port?, access)))
        }))?;
    }
    ruleset.all_threads(true)?.restrict_self()
}

fn dev_port(config: &SabineWindowConfig) -> Option<u16> {
    crate::launch::dev_server::port(config.dev_url.as_deref()?)
}

/// Whether Sabine confines the app to its `[sandbox]`: it declares one it
/// runs in, doesn't run in an app sandbox already, and the system didn't
/// give it full access.
pub(crate) fn applies(config: &SabineWindowConfig) -> bool {
    matches!(config.sandbox, Some(Sandbox::Confined { .. }))
        && !super::sandbox::in_app_sandbox()
        && std::env::var_os(FULL_ACCESS).is_none()
}

/// The socket later launches of a single-instance app hand it their
/// arguments through, which it wakes itself through as it ends.
fn instance_socket(config: &SabineWindowConfig, app_id: &str) -> Option<PathBuf> {
    let services = &config.desktop_services;
    services
        .single_instance_policy
        .filter(|policy| *policy != SingleInstancePolicy::AllowMultiple)?;
    let id = services.single_instance_id.as_deref().unwrap_or(app_id);
    sabine_runtime::instance::instance_socket(&crate::desktop::sanitize_id(id)).ok()
}

/// The app's own runtime folder, which only it and the processes it starts
/// use.
fn own_runtime_folder(app_id: &str) -> SabineResult<PathBuf> {
    crate::osr::transport::runtime_ipc_dir(app_id)
        .and_then(|folder| {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&folder)
                .map(|()| folder)
        })
        .map_err(|error| SabineError::CreationFailed {
            message: format!("could not create the app's runtime folder: {error}"),
        })
}

/// What a confined app runs on while it lives: its filtered session bus and
/// the sound server it plays through, which both end with the app.
pub(crate) struct Confinement {
    _bus: Option<SessionBus>,
    _sound: Option<PlaybackServer>,
}

/// Confines the app to what its `[sandbox]` declares, when [`applies`] says
/// Sabine does, and hands back what it runs on, which runs while the app
/// holds it.
pub(crate) fn apply(
    config: &SabineWindowConfig,
    runtime: &Path,
    host: &Path,
) -> SabineResult<Option<Confinement>> {
    let Some(Sandbox::Confined {
        network,
        filesystems,
        devices,
        talk,
        own,
        pipewire,
        microphone,
        camera,
        ..
    }) = &config.sandbox
    else {
        return Ok(None);
    };
    if !applies(config) {
        return Ok(None);
    }
    let app_id = crate::osr::launch::require_app_id(config)?;
    let own_folder = own_runtime_folder(app_id)?;
    let bus = bus::filter(talk, own, &own_folder);
    let sound = (!*microphone)
        .then(|| sound::playback(&own_folder))
        .flatten();
    let program = std::env::current_exe()
        .ok()
        .and_then(|program| program.parent().map(Path::to_path_buf));
    let profile = crate::host::browser_profile_dir(app_id);
    let own = Own {
        app_id,
        program,
        web_root: config.web_root()?,
        runtime,
        host: host.parent().unwrap_or(host),
        profile: profile.parent().unwrap_or(&profile).to_path_buf(),
    };
    let mut paths = Paths::default();
    paths
        .system()
        .desktop()
        .own(&own)
        .runtime(&Runtime {
            own: own_folder,
            instance: instance_socket(config, app_id),
            records: *microphone,
            media: *pipewire
                || *microphone
                || *camera
                || config
                    .permissions
                    .iter()
                    .any(|declared| declared.permission == sabine_service::Permission::Screen),
        })
        .declared(filesystems, devices);
    if *camera {
        paths.cameras();
    }
    let ports = (!network).then(|| Ports {
        connect: dev_port(config),
        bind: config.effective_remote_devtools_port(),
    });
    let failed = |error: &dyn std::fmt::Display| SabineError::CreationFailed {
        message: format!("could not confine the app to its sandbox: {error}"),
    };
    let status = restrict(paths, ports).map_err(|error| failed(&error))?;
    if status.ruleset == RulesetStatus::NotEnforced {
        sabine_runtime::report_error(
            "sandbox",
            "this kernel has no Landlock, so the app's [sandbox] limits only its system calls",
        );
    }
    seccomp::apply().map_err(|error| failed(&error))?;
    Ok(Some(Confinement {
        _bus: bus,
        _sound: sound,
    }))
}
