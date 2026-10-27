use std::{
    fs,
    path::Path,
    process::{Command, ExitCode},
};

use serde::Deserialize;

use crate::environment::BuildEnvironment;

const DEFAULT_PATH: &str = "src/bindings.ts";

#[derive(Debug, Default, Deserialize)]
struct SabineFile {
    #[serde(default)]
    app: AppSection,
    #[serde(default)]
    bindings: BindingsSection,
}

#[derive(Debug, Default, Deserialize)]
struct AppSection {
    cargo_manifest: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct BindingsSection {
    path: Option<String>,
}

/// Builds the app and has it write its TypeScript bindings, or with `check`,
/// fails when the bindings on disk are out of date.
pub fn run_bindings(source: &Path, check: bool) -> Result<ExitCode, String> {
    let source = std::path::absolute(source).map_err(|error| error.to_string())?;
    let sabine = read_sabine_file(&source.join("Sabine.toml"))?;
    let manifest = source.join(sabine.app.cargo_manifest.as_deref().unwrap_or("Cargo.toml"));
    let target = source.join(sabine.bindings.path.as_deref().unwrap_or(DEFAULT_PATH));

    let mut command = Command::new("cargo");
    command
        .args(["run", "--quiet", "--manifest-path"])
        .arg(&manifest)
        .current_dir(&source)
        .env("SABINE_BINDINGS", &target);
    if check {
        command.env("SABINE_BINDINGS_CHECK", "1");
    }
    BuildEnvironment::load(
        &source,
        None,
        &[],
        sabine_service::AppEnvironment::Development,
    )?
    .apply(&mut command);
    let status = command
        .status()
        .map_err(|error| format!("failed to run cargo: {error}"))?;
    Ok(if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn read_sabine_file(path: &Path) -> Result<SabineFile, String> {
    if !path.is_file() {
        return Ok(SabineFile::default());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("invalid {}: {error}", path.display()))
}
