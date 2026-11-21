use std::{path::Path, process::Command};

use sabine_runtime::{RuntimeConfig, RuntimeLocation, resolve_runtime};
use sabine_service::AppUpdateSource;

use console::style;

use crate::project::{ConfigOverrides, resolve_app};

/// Prints what an app in `source` builds and runs with on this machine.
pub fn run(source: &Path) {
    println!("{}", style("Sabine").bold());
    row("CLI", sabine_service::SABINE_VERSION);
    row(
        "Service",
        sabine_service::installed_service_version()
            .as_deref()
            .unwrap_or("not installed"),
    );
    row(
        "Daemon",
        sabine_service::running_daemon_version()
            .as_deref()
            .unwrap_or("not running"),
    );
    match resolve_runtime(&RuntimeConfig::default()) {
        Ok(runtime) => {
            let location = match &runtime.location {
                RuntimeLocation::System(_) => "system",
                RuntimeLocation::UserLocal(_) => "user",
                RuntimeLocation::Bundled(_) => "bundled",
            };
            let host = sabine_host::available_host(runtime.location.path())
                .map_or("host missing", |_| "host ready");
            row("CEF", &format!("{} ({location}, {host})", runtime.version));
            row("", &runtime.location.path().display().to_string());
        }
        Err(error) => row("CEF", &error.to_string()),
    }

    section("Toolchain");
    for (name, program) in [
        ("Rust", "rustc"),
        ("Cargo", "cargo"),
        ("Bun", "bun"),
        ("Node", "node"),
    ] {
        row(name, &version_of(program));
    }

    section("Platform");
    row(
        "System",
        &format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    );
    #[cfg(target_os = "linux")]
    row("Session", &linux_session());

    section("Project");
    match resolve_app(source, ConfigOverrides::default()) {
        Ok(app) => {
            row("Name", &app.name);
            row("Id", &app.id);
            row("Version", &app.version);
            row("Package", &app.cargo_package);
            if let Some(web) = &app.web {
                row(
                    "Web",
                    &web.url
                        .clone()
                        .unwrap_or_else(|| web.dist.display().to_string()),
                );
            }
            row(
                "Updates",
                &match app.updates.as_ref().map(|updates| &updates.source) {
                    Some(AppUpdateSource::Github { repository }) => {
                        format!("GitHub releases of {repository}")
                    }
                    Some(AppUpdateSource::Http { url }) => url.clone(),
                    Some(AppUpdateSource::Schelf) => "Schelf".to_string(),
                    None => "none".to_string(),
                },
            );
        }
        Err(error) => row("Status", &format!("not an app: {error}")),
    }
}

fn section(title: &str) {
    println!("\n{}", style(title).bold());
}

/// A labelled value, with what's missing or failing picked out.
fn row(label: &str, value: &str) {
    let missing = value.starts_with("not ") || value.contains("missing");
    let value = if missing {
        style(value).yellow()
    } else if label.is_empty() {
        style(value).dim()
    } else {
        style(value)
    };
    println!("  {}  {value}", style(format!("{label:<8}")).dim());
}

fn version_of(program: &str) -> String {
    Command::new(program)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .map(str::to_string)
        })
        .unwrap_or_else(|| "not found".to_string())
}

#[cfg(target_os = "linux")]
fn linux_session() -> String {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown desktop".into());
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        format!("Wayland, {desktop}")
    } else {
        format!("no Wayland display, {desktop}")
    }
}
