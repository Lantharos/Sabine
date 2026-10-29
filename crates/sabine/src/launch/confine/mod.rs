//! Holds an app outside an app sandbox to its `[sandbox]` declaration with
//! Landlock and a system call filter, which every process it starts inherits.

mod paths;
mod seccomp;

use std::path::Path;

use landlock::{
    Access, AccessFs, AccessNet, NetPort, PathBeneath, PathFd, RestrictSelfAttr, RestrictionStatus,
    Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetError, RulesetStatus, Scope,
};
use sabine_service::Sandbox;

use crate::error::{SabineError, SabineResult};
use crate::window::config::SabineWindowConfig;
use paths::{LANDLOCK, Own, Paths};

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
    crate::launch::dev_url_parts(config.dev_url.as_deref()?).map(|parts| parts.port)
}

/// Confines the app to what its `[sandbox]` declares, unless it declares
/// nothing, says it can't run in a sandbox, already runs in one or the
/// system gave it full access.
pub(crate) fn apply(config: &SabineWindowConfig, runtime: &Path, host: &Path) -> SabineResult<()> {
    let Some(Sandbox::Confined {
        network,
        filesystems,
        devices,
        ..
    }) = &config.sandbox
    else {
        return Ok(());
    };
    if super::sandbox::in_app_sandbox() || std::env::var_os(FULL_ACCESS).is_some() {
        return Ok(());
    }
    let app_id = crate::osr::launch::require_app_id(config)?;
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
        .declared(filesystems, devices);
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
    seccomp::apply().map_err(|error| failed(&error))
}
