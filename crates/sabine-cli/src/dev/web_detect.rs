use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::project::read_sabine_file;
use sabine_service::manifest::WebSection;

#[derive(Debug, Clone)]
pub struct DevProject {
    pub source: PathBuf,
    pub app_id: Option<String>,
    pub cargo_manifest: PathBuf,
    pub sabine_manifest: Option<PathBuf>,
    pub frontend: Option<DevFrontend>,
}

#[derive(Debug, Clone)]
pub struct DevFrontend {
    pub root: PathBuf,
    pub url: String,
    /// The local port the server listens on, which `sabine dev` waits for.
    pub port: Option<u16>,
    /// What starts the server, unless something else already runs it.
    pub command: Option<String>,
    /// The package manager of a JavaScript web project, which installs its
    /// dependencies first.
    pub package_manager: Option<&'static str>,
}

pub fn resolve_dev_project(source: &Path) -> Result<DevProject, String> {
    let source_dir = absolute_path(source)?;
    let sabine_path = source_dir.join("Sabine.toml");
    let mut sabine = read_sabine_file(&source_dir)?;
    if let Some(sandbox) = sabine.sandbox.take() {
        sandbox.resolve()?;
    }
    let cargo_manifest = sabine.cargo_manifest(&source_dir);
    if !cargo_manifest.is_file() {
        return Err(format!(
            "no Cargo.toml found at {} (looked from {})",
            cargo_manifest.display(),
            source_dir.display()
        ));
    }
    let frontend = resolve_frontend(&source_dir, &sabine.web)?;
    Ok(DevProject {
        source: source_dir,
        app_id: sabine.app.id,
        cargo_manifest,
        sabine_manifest: sabine_path.is_file().then_some(sabine_path),
        frontend,
    })
}

fn resolve_frontend(source_dir: &Path, web: &WebSection) -> Result<Option<DevFrontend>, String> {
    let configured_root = web.root.as_ref().map(|root| source_dir.join(root));
    let package_root = configured_root
        .clone()
        .filter(|root| root.join("package.json").is_file())
        .or_else(|| detect_package_root(source_dir));
    let package = package_root
        .as_deref()
        .map(|root| read_package_json(&root.join("package.json")))
        .transpose()?;
    let root = package_root
        .or(configured_root)
        .unwrap_or_else(|| source_dir.to_path_buf());
    let package_manager = package
        .as_ref()
        .map(|package| package_manager(&root, package));
    let dev_url = nonempty(web.dev_url.as_deref());
    let has_dev_url = dev_url.is_some();
    let (url, port) = match dev_url {
        Some(url) => {
            let port = web.dev_port.or_else(|| parse_localhost_port(&url));
            (url, port)
        }
        None => {
            let port = web
                .dev_port
                .or_else(|| package.as_ref().and_then(|_| detect_vite_port(&root)))
                .unwrap_or(5173);
            (vite_dev_url(port), Some(port))
        }
    };
    let script = package.as_ref().and_then(detect_dev_script);
    let command = nonempty(web.dev_command.as_deref()).or_else(|| {
        let manager = package_manager?;
        let script = script?;
        Some(match port {
            Some(port) => format!("{manager} run {script} -- --port {port} --strictPort"),
            None => format!("{manager} run {script}"),
        })
    });
    if command.is_none() && !has_dev_url {
        return Ok(None);
    }
    Ok(Some(DevFrontend {
        root,
        url,
        port,
        command,
        package_manager,
    }))
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

pub fn detect_package_root(source_dir: &Path) -> Option<PathBuf> {
    ["ui", "frontend", "web", "."]
        .iter()
        .map(|candidate| source_dir.join(candidate))
        .find(|candidate| candidate.join("package.json").is_file())
}

pub fn detect_web_build_command(root: &Path) -> Option<String> {
    let package_json = root.join("package.json");
    if !package_json.is_file() {
        return None;
    }
    let text = fs::read_to_string(package_json).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    value
        .get("scripts")
        .and_then(|scripts| scripts.get("build"))
        .and_then(serde_json::Value::as_str)?;
    Some(format!("{} run build", package_manager(root, &value)))
}

pub fn package_manager(root: &Path, package: &serde_json::Value) -> &'static str {
    if root.join("bun.lock").exists() || root.join("bun.lockb").exists() {
        return "bun";
    }
    if root.join("pnpm-lock.yaml").exists() {
        return "pnpm";
    }
    if root.join("yarn.lock").exists() {
        return "yarn";
    }
    if package
        .get("packageManager")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.starts_with("bun@"))
    {
        return "bun";
    }
    if sabine_runtime::find_program("bun").is_some() {
        "bun"
    } else {
        "npm"
    }
}

fn detect_dev_script(package: &serde_json::Value) -> Option<String> {
    let scripts = package.get("scripts")?;
    for name in ["dev", "start"] {
        if scripts
            .get(name)
            .and_then(serde_json::Value::as_str)
            .is_some()
        {
            return Some(name.to_string());
        }
    }
    None
}

const VITE_CONFIGS: [&str; 4] = [
    "vite.config.ts",
    "vite.config.js",
    "vite.config.mjs",
    "vite.config.cjs",
];

fn detect_vite_port(root: &Path) -> Option<u16> {
    let project_root = root.parent();
    [Some(root), project_root]
        .into_iter()
        .flatten()
        .flat_map(|directory| VITE_CONFIGS.iter().map(move |name| directory.join(name)))
        .filter_map(|path| fs::read_to_string(path).ok())
        .find_map(|text| extract_port_from_vite_config(&text))
}

fn extract_port_from_vite_config(text: &str) -> Option<u16> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("port:")
            .or_else(|| trimmed.strip_prefix("port :"))
        {
            let digits: String = rest
                .trim()
                .trim_matches(',')
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect();
            if let Ok(port) = digits.parse::<u16>() {
                return Some(port);
            }
        }
    }
    None
}

fn read_package_json(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| error.to_string())?
    };
    Ok(dunce_simplify(absolute))
}

fn dunce_simplify(path: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

fn vite_dev_url(port: u16) -> String {
    format!("http://localhost:{port}")
}

fn parse_localhost_port(url: &str) -> Option<u16> {
    let url = url.trim();
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if let Some((host, port)) = host_port.rsplit_once(':')
        && matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
    {
        return port.parse().ok();
    }
    None
}
