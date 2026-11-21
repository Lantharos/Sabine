mod format;
mod install;
mod linux;
mod macos;
mod metadata;
mod package;
mod signing;
mod stage;
mod system;
mod windows;

pub use install::install_bundle;
pub use system::install_system_bundle;

use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use crate::{
    dev::{bindings, web_detect},
    install::source::InstallOptions,
    project::{BundleApp, ConfigOverrides, environment::BuildEnvironment},
    ui::{self, Step},
    util::shell_command,
};
use format::{BuildTarget, BundleFormat};
use package::package_bundle;
use signing::Signing;
use stage::{StagedBundle, binary_path, stage_bundle};

#[derive(Debug)]
pub struct BundleOptions {
    pub build: BuildOptions,
    pub targets: Vec<String>,
    pub out: PathBuf,
    pub build_only: bool,
    pub json: bool,
    pub offline: bool,
}

#[derive(Debug)]
pub struct BuildOptions {
    pub source: PathBuf,
    pub release: bool,
    pub no_build: bool,
    pub binary: Option<PathBuf>,
    pub no_web_build: bool,
    pub web_build: Option<String>,
    pub web_root: Option<PathBuf>,
    pub web_dist: Option<PathBuf>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    /// The operating system owns updates, so the bundle carries no update source.
    pub system_wide: bool,
    pub env_files: Vec<PathBuf>,
}

/// An app built for one operating system, ready to stage in its formats.
struct Build {
    app: BundleApp,
    binary: PathBuf,
}

pub fn bundle(options: BundleOptions) -> Result<ExitCode, String> {
    let started = std::time::Instant::now();
    let formats = parse_formats(&options.targets)?;
    let target = formats[0].target();
    if options.offline && target.as_str() != std::env::consts::OS {
        return Err("Offline bundles must be built on their target operating system so the runtime and service match the application".to_string());
    }
    let signing = if options.build_only {
        Signing::Unsigned
    } else {
        Signing::from_env(target)?
    };
    let (json, offline, build_only) = (options.json, options.offline, options.build_only);
    let out = std::path::absolute(&options.out).map_err(|error| error.to_string())?;
    let build = build(options.build, formats[0])?;
    if offline && build.app.standalone {
        return Err(
            "A standalone app's bundles always carry Chromium and its host; leave out --offline"
                .to_string(),
        );
    }
    if build_only {
        if json {
            println!(
                "{}",
                serde_json::json!({ "ok": true, "binary": build.binary })
            );
        } else {
            ui::result(&ui::shown(&build.binary), &file_size(&build.binary));
        }
        return Ok(ExitCode::SUCCESS);
    }
    crate::project::updates::require_public_key(build.app.updates.as_ref())?;
    let mut artifacts = Vec::new();
    for format in formats {
        let step = Step::start(format!("Packaging {}", format.as_str()));
        step.note("Laying out the app");
        let staged = stage_bundle(&build.app, format, &build.binary, &out, offline)?;
        step.note("Signing");
        signing.sign_payload(format, &staged, &build.app.permissions)?;
        let artifact = package_bundle(&build.app, format, &staged, signing.windows(), &step)?;
        signing.sign_artifact(format, &artifact)?;
        step.done_with(&file_size(&artifact));
        if json {
            println!("{}", bundle_json(&build.app, format, &staged, &artifact));
        }
        artifacts.push(artifact);
    }
    if !json {
        eprintln!();
        for artifact in &artifacts {
            ui::result(&ui::shown(artifact), &file_size(artifact));
        }
        ui::summary(&format!(
            "Bundled {} {} in {}",
            build.app.name,
            build.app.version,
            ui::duration(started.elapsed())
        ));
    }
    Ok(ExitCode::SUCCESS)
}

fn file_size(path: &Path) -> String {
    std::fs::metadata(path).map_or(String::new(), |metadata| ui::size(metadata.len()))
}

/// Builds and stages the app as a portable bundle in `out`, for installing
/// it on this machine.
fn stage_portable(
    options: &InstallOptions,
    env_files: Vec<PathBuf>,
    out: &Path,
    system_wide: bool,
) -> Result<(BundleApp, StagedBundle), String> {
    let build = build(
        BuildOptions {
            source: options.source.clone(),
            release: true,
            no_build: false,
            binary: None,
            no_web_build: false,
            web_build: None,
            web_root: None,
            web_dist: None,
            id: options.id.clone(),
            name: options.name.clone(),
            version: None,
            system_wide,
            env_files,
        },
        BundleFormat::Portable,
    )?;
    let staged = stage_bundle(
        &build.app,
        BundleFormat::Portable,
        &build.binary,
        out,
        false,
    )?;
    Ok((build.app, staged))
}

fn parse_formats(targets: &[String]) -> Result<Vec<BundleFormat>, String> {
    let formats = targets
        .iter()
        .map(|target| {
            BundleFormat::parse(target).ok_or_else(|| format!("unknown bundle target `{target}`; use linux, portable, deb, rpm, appimage, windows, exe, msi, macos, or dmg"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let Some(first) = formats.first() else {
        return Err("name at least one bundle target".into());
    };
    if formats
        .iter()
        .any(|format| format.target() != first.target())
    {
        return Err("bundle targets built together must run on the same operating system".into());
    }
    Ok(formats)
}

fn build(options: BuildOptions, format: BundleFormat) -> Result<Build, String> {
    let mut app = crate::project::resolve_app(
        &options.source,
        ConfigOverrides {
            id: options.id,
            name: options.name,
            version: options.version,
            web_build: options.web_build,
            web_root: options.web_root,
            web_dist: options.web_dist,
        },
    )?;
    if options.system_wide {
        app.updates = None;
    }
    let target = format.target();
    let environment = BuildEnvironment::load(
        &app.source_dir,
        None,
        &options.env_files,
        sabine_service::AppEnvironment::Production,
    )?;
    let binary = if let Some(binary) = options.binary {
        std::path::absolute(binary).map_err(|error| error.to_string())?
    } else if options.no_build {
        binary_path(&app, format, options.release)
    } else {
        build_rust(&app, target, options.release, &environment)?
    };
    if !binary.is_file() {
        return Err(format!(
            "built binary was not found at {}; pass --binary to package an existing executable or --no-build only when the default Cargo output already exists",
            binary.display()
        ));
    }
    if !options.no_web_build {
        let web_environment = BuildEnvironment::load(
            &app.source_dir,
            app.web.as_ref().map(|web| web.root.as_path()),
            &options.env_files,
            sabine_service::AppEnvironment::Production,
        )?;
        let step = Step::start("Writing TypeScript bindings");
        if target == BuildTarget::host() {
            bindings::write_with(&app.source_dir, &binary, &environment)?;
        } else {
            bindings::refresh(&app.source_dir, &environment, options.release)?;
        }
        step.done();
        build_web(&app, &web_environment)?;
    }
    Ok(Build { app, binary })
}

fn build_web(app: &BundleApp, environment: &BuildEnvironment) -> Result<(), String> {
    let Some(web) = &app.web else {
        return Ok(());
    };
    if !web.has_local_assets {
        return Ok(());
    }
    let Some(command) = &web.build_command else {
        return Ok(());
    };
    web_detect::ensure_dependencies(&web.root)?;
    let step = Step::start("Building the interface");
    let mut process = shell_command(command);
    environment.apply(&mut process);
    ui::run(
        &step,
        process.current_dir(&web.root),
        &format!("`{command}`"),
    )?;
    step.done();
    if !web.dist.exists() {
        return Err(format!(
            "web build completed but dist path does not exist: {}",
            web.dist.display()
        ));
    }
    Ok(())
}

fn build_rust(
    app: &BundleApp,
    target: BuildTarget,
    release: bool,
    environment: &BuildEnvironment,
) -> Result<PathBuf, String> {
    if !app.cargo_manifest.is_file() {
        return Err(format!(
            "missing Cargo.toml at {}",
            app.cargo_manifest.display()
        ));
    }
    let mut command = Command::new("cargo");
    environment.apply(&mut command);
    command
        .current_dir(&app.source_dir)
        .arg("build")
        .arg("--message-format=json-render-diagnostics")
        .arg("--bin")
        .arg(&app.cargo_package)
        .arg("--manifest-path")
        .arg(&app.cargo_manifest);
    if release {
        command.arg("--release");
    }
    if let Some(rust_target) = target.rust_target() {
        command.arg("--target").arg(rust_target);
        if target == BuildTarget::Windows {
            link_statically_as_gui(&mut command, rust_target);
        }
    }
    command.env("SABINE_BUILD_TARGET", target.as_str());
    let step = Step::start(format!("Building {}", app.name));
    let mut executable = None;
    ui::run_cargo(&step, &mut command, |line| {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        if message["reason"] == "compiler-artifact"
            && message["target"]["name"] == app.cargo_package
            && let Some(path) = message["executable"].as_str()
        {
            executable = Some(PathBuf::from(path));
        }
    })?;
    let executable = executable
        .ok_or_else(|| format!("cargo did not produce the {} executable", app.cargo_package))?;
    step.done();
    Ok(executable)
}

/// Windows apps carry the C runtime and start without a console. Cargo
/// merges these flags with the project's own for the target, unless the
/// environment's flags replace all of them.
fn link_statically_as_gui(command: &mut Command, rust_target: &str) {
    const FLAGS: [&str; 4] = [
        "-C",
        "target-feature=+crt-static",
        "-C",
        "link-arg=/SUBSYSTEM:WINDOWS",
    ];
    if let Some(mut flags) = std::env::var_os("CARGO_ENCODED_RUSTFLAGS") {
        if !flags.is_empty() {
            flags.push("\x1f");
        }
        flags.push(FLAGS.join("\x1f"));
        command.env("CARGO_ENCODED_RUSTFLAGS", flags);
    } else if let Ok(mut flags) = std::env::var("RUSTFLAGS") {
        if !flags.is_empty() {
            flags.push(' ');
        }
        flags.push_str(&FLAGS.join(" "));
        command.env("RUSTFLAGS", flags);
    } else {
        let flags = FLAGS.map(|flag| format!("{flag:?}")).join(", ");
        command
            .arg("--config")
            .arg(format!("target.{rust_target}.rustflags = [{flags}]"));
    }
}

fn bundle_json(
    app: &BundleApp,
    format: BundleFormat,
    staged: &StagedBundle,
    artifact: &std::path::Path,
) -> String {
    serde_json::json!({
        "ok": true,
        "target": format.as_str(),
        "app": { "id": app.id, "name": app.name, "version": app.version },
        "path": staged.root,
        "executable": staged
            .root
            .parent()
            .and_then(|parent| staged.binary.strip_prefix(parent).ok()),
        "artifacts": [artifact],
    })
    .to_string()
}
