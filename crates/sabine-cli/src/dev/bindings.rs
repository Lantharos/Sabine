use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use serde::Deserialize;

use crate::environment::BuildEnvironment;

#[derive(Debug, Default, Deserialize)]
struct SabineFile {
    #[serde(default)]
    app: AppSection,
    bindings: Option<BindingsSection>,
}

#[derive(Debug, Default, Deserialize)]
struct AppSection {
    cargo_manifest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BindingsSection {
    path: String,
}

struct Bindings {
    manifest: PathBuf,
    target: PathBuf,
}

/// Builds the app and has it write its TypeScript bindings, or with `check`,
/// fails when the bindings on disk are out of date.
pub fn run_bindings(source: &Path, check: bool) -> Result<ExitCode, String> {
    let source = std::path::absolute(source).map_err(|error| error.to_string())?;
    let bindings = configured(&source)?
        .ok_or("Sabine.toml has no [bindings] section naming the file to write")?;
    let environment = BuildEnvironment::load(
        &source,
        None,
        &[],
        sabine_service::AppEnvironment::Development,
    )?;
    Ok(if bindings.export(&source, &environment, false, check)? {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Rewrites the bindings before `sabine dev` starts the page, when the app
/// has them.
pub(super) fn refresh(
    source: &Path,
    environment: &BuildEnvironment,
    release: bool,
) -> Result<(), String> {
    match configured(source)? {
        Some(bindings) if !bindings.export(source, environment, release, false)? => {
            Err("could not write the app's TypeScript bindings".into())
        }
        _ => Ok(()),
    }
}

impl Bindings {
    fn export(
        &self,
        source: &Path,
        environment: &BuildEnvironment,
        release: bool,
        check: bool,
    ) -> Result<bool, String> {
        let mut command = Command::new("cargo");
        command
            .args(["run", "--quiet", "--manifest-path"])
            .arg(&self.manifest)
            .current_dir(source)
            .env("SABINE_BINDINGS", &self.target);
        if release {
            command.arg("--release");
        }
        if check {
            command.env("SABINE_BINDINGS_CHECK", "1");
        }
        environment.apply(&mut command);
        command
            .status()
            .map(|status| status.success())
            .map_err(|error| format!("failed to run cargo: {error}"))
    }
}

fn configured(source: &Path) -> Result<Option<Bindings>, String> {
    let path = source.join("Sabine.toml");
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let sabine: SabineFile =
        toml::from_str(&text).map_err(|error| format!("invalid {}: {error}", path.display()))?;
    Ok(sabine.bindings.map(|bindings| Bindings {
        manifest: source.join(sabine.app.cargo_manifest.as_deref().unwrap_or("Cargo.toml")),
        target: source.join(bindings.path),
    }))
}
