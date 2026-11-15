use std::{
    process::{Command, ExitCode, ExitStatus},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use super::{
    DevOptions, bindings, process_tree::ProcessTree, watch::RustChanges, web_detect::DevProject,
};
use crate::{project::environment::BuildEnvironment, util::shell_command};

pub(super) struct Session<'a> {
    pub project: &'a DevProject,
    pub options: &'a DevOptions,
    pub environment: &'a BuildEnvironment,
    pub frontend: Option<&'a mut ProcessTree>,
    pub interrupted: &'a AtomicBool,
}

enum Outcome {
    Exited(ExitStatus),
    Changed,
    Stopped(Result<ExitCode, String>),
}

/// Runs the app until it exits, or until Ctrl+C. Without a custom command,
/// a change to its Rust sources rebuilds and relaunches it, and an app that
/// fails waits for the next change.
pub(super) fn run(mut session: Session) -> Result<ExitCode, String> {
    if session.options.command.is_some() {
        return run_once(&mut session);
    }
    let package = session
        .project
        .cargo_manifest
        .parent()
        .ok_or("Cargo.toml has no folder")?;
    let mut changes = RustChanges::watch(package)?;
    loop {
        let mut app = spawn(&session)?;
        let outcome = supervise(&mut session, &mut app, &mut changes);
        let _ = app.terminate();
        match outcome {
            Outcome::Stopped(result) => return result,
            Outcome::Exited(status) if status.success() => return Ok(exit_code(status)),
            Outcome::Exited(status) => {
                eprintln!("sabine: the app exited with {status}; waiting for changes");
                if let Some(stopped) = wait_for_change(&mut session, &mut changes) {
                    return stopped;
                }
            }
            Outcome::Changed => {}
        }
        eprintln!("sabine: Rust sources changed; rebuilding");
        while let Err(error) = bindings::refresh(
            &session.project.source,
            session.environment,
            session.options.release,
        ) {
            eprintln!("sabine: {error}; waiting for changes");
            if let Some(stopped) = wait_for_change(&mut session, &mut changes) {
                return stopped;
            }
        }
    }
}

fn run_once(session: &mut Session) -> Result<ExitCode, String> {
    let mut app = spawn(session)?;
    loop {
        if let Some(stopped) = stopped(session) {
            let _ = app.terminate();
            return stopped;
        }
        if let Some(status) = app
            .try_wait()
            .map_err(|error| format!("failed waiting for the app: {error}"))?
        {
            return Ok(exit_code(status));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn spawn(session: &Session) -> Result<ProcessTree, String> {
    ProcessTree::spawn(&mut app_command(session))
        .map_err(|error| format!("failed to start the app: {error}"))
}

fn app_command(session: &Session) -> Command {
    let Session {
        project,
        options,
        environment,
        ..
    } = session;
    let mut command = if let Some(custom) = &options.command {
        #[cfg(unix)]
        let mut command = shell_command(&format!("{custom} \"$@\""));
        #[cfg(unix)]
        command.arg("sabine-app");
        #[cfg(windows)]
        let mut command = shell_command(custom);
        command.args(&options.args);
        command
    } else {
        let mut command = Command::new("cargo");
        command
            .arg("run")
            .arg("--manifest-path")
            .arg(&project.cargo_manifest);
        if options.release {
            command.arg("--release");
        }
        command.arg("--").args(&options.args);
        command
    };
    command.current_dir(&project.source);
    environment.apply(&mut command);
    command.env_remove("SABINE_DEV_URL");
    if let Some(id) = options
        .app_id
        .clone()
        .or_else(|| std::env::var("SABINE_APP_ID").ok())
        .or_else(|| project.app_id.clone())
    {
        command.env(
            "SABINE_APP_ID",
            sabine_service::AppEnvironment::Development.app_id(&id),
        );
    }
    if let Some(frontend) = &project.frontend {
        command.env("SABINE_DEV_URL", &frontend.url);
    }
    if let Some(manifest) = &project.sabine_manifest {
        command.env("SABINE_MANIFEST_PATH", manifest);
    }
    command
}

fn supervise(session: &mut Session, app: &mut ProcessTree, changes: &mut RustChanges) -> Outcome {
    loop {
        if let Some(stopped) = stopped(session) {
            return Outcome::Stopped(stopped);
        }
        match app.try_wait() {
            Ok(Some(status)) => return Outcome::Exited(status),
            Ok(None) => {}
            Err(error) => {
                return Outcome::Stopped(Err(format!("failed waiting for the app: {error}")));
            }
        }
        if changes.settled() {
            return Outcome::Changed;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Waits for the sources to change, or returns why `sabine dev` stops.
fn wait_for_change(
    session: &mut Session,
    changes: &mut RustChanges,
) -> Option<Result<ExitCode, String>> {
    loop {
        if let Some(stopped) = stopped(session) {
            return Some(stopped);
        }
        if changes.settled() {
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Ctrl+C, or a frontend server that exited while the app needs it.
fn stopped(session: &mut Session) -> Option<Result<ExitCode, String>> {
    if session.interrupted.load(Ordering::SeqCst) {
        return Some(Ok(ExitCode::from(130)));
    }
    let frontend = session.frontend.as_mut()?;
    match frontend.try_wait() {
        Ok(None) => None,
        Ok(Some(status)) => Some(Err(format!(
            "frontend exited while the app was running: {status}"
        ))),
        Err(error) => Some(Err(format!("failed waiting for frontend: {error}"))),
    }
}

pub(super) fn wait_frontend(
    child: Option<&mut ProcessTree>,
    interrupted: &AtomicBool,
) -> Result<ExitCode, String> {
    let Some(child) = child else {
        while !interrupted.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(100));
        }
        return Ok(ExitCode::from(130));
    };
    loop {
        if interrupted.load(Ordering::SeqCst) {
            let _ = child.terminate();
            return Ok(ExitCode::from(130));
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("failed waiting for frontend: {error}"))?
        {
            return Ok(exit_code(status));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn exit_code(status: ExitStatus) -> ExitCode {
    ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8)
}
