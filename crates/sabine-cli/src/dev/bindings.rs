use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use crate::project::environment::BuildEnvironment;

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

/// Rewrites the bindings with `cargo run`, when the app has them.
pub(crate) fn refresh(
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

/// Has the app built at `program` write its bindings, when it has them.
pub(crate) fn write_with(
    source: &Path,
    program: &Path,
    environment: &BuildEnvironment,
) -> Result<(), String> {
    let Some(bindings) = configured(source)? else {
        return Ok(());
    };
    let mut command = Command::new(program);
    environment.apply(&mut command);
    let status = command
        .current_dir(source)
        .env("SABINE_BINDINGS", &bindings.target)
        .status()
        .map_err(|error| format!("failed to run {}: {error}", program.display()))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "could not write the app's TypeScript bindings".to_string())
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
    let sabine = crate::project::read_sabine_file(source)?;
    Ok(sabine.bindings.as_ref().map(|bindings| Bindings {
        manifest: sabine.cargo_manifest(source),
        target: source.join(&bindings.path),
    }))
}
