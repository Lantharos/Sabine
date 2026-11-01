mod init;
pub(crate) mod workflow;

use base64::{Engine, engine::general_purpose::STANDARD};
use sabine_service::{
    AppArtifact, AppArtifactKind, AppReleaseManifest, AppUpdateSource, SystemReleaseArtifact,
    SystemReleaseManifest, public_key_from_private, sign_app_release, sign_system_release,
};

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(crate) use init::initialize_github_release;

use crate::project::{ConfigOverrides, absolute_path, resolve_app};

pub(crate) struct ManifestRequest<'a> {
    pub source: &'a Path,
    pub output: &'a Path,
    pub channel: &'a str,
    pub artifact_specs: &'a [String],
    pub executable_specs: &'a [String],
    pub base_url: Option<&'a str>,
}

pub(crate) fn write_manifest(request: ManifestRequest) -> Result<(), String> {
    let ManifestRequest {
        source,
        output,
        channel,
        artifact_specs,
        executable_specs,
        base_url,
    } = request;
    let app = resolve_app(source, ConfigOverrides::default())?;
    let updates = app.updates.ok_or_else(|| {
        "Sabine.toml names no update source; configure [updates], or build inside GitHub Actions"
            .to_string()
    })?;
    let tag = format!("v{}", app.version);
    if let Ok(ref_name) = std::env::var("GITHUB_REF_NAME")
        && ref_name != tag
    {
        return Err(format!(
            "release tag {ref_name} does not match the app version {tag}"
        ));
    }
    if updates.channel != channel {
        return Err(format!(
            "requested channel {channel} does not match Sabine.toml channel {}",
            updates.channel
        ));
    }
    let base_url = match (base_url, &updates.source) {
        (Some(base_url), _) if base_url.starts_with("https://") => {
            base_url.trim_end_matches('/').to_string()
        }
        (Some(_), _) => return Err("--base-url must use HTTPS".to_string()),
        (None, AppUpdateSource::Github { repository }) => {
            format!("https://github.com/{repository}/releases/download/{tag}")
        }
        (None, AppUpdateSource::Http { .. }) => {
            return Err(
                "pass --base-url with the HTTPS folder the packages are uploaded to".to_string(),
            );
        }
    };
    let executables = parse_assignments(executable_specs)?;
    let mut artifacts = BTreeMap::new();
    for (target, raw_path) in parse_assignments(artifact_specs)? {
        let path = app.source_dir.join(raw_path);
        if !path.is_file() {
            return Err(format!(
                "release artifact does not exist: {}",
                path.display()
            ));
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("invalid artifact name: {}", path.display()))?;
        let kind = artifact_kind(&path)?;
        artifacts.insert(
            target.clone(),
            AppArtifact {
                url: format!("{base_url}/{file_name}"),
                sha256: sha256(&path)?,
                kind,
                executable: executables.get(&target).map(PathBuf::from),
            },
        );
    }
    if artifacts.is_empty() {
        return Err("at least one --artifact target=path is required".to_string());
    }
    let mut manifest = AppReleaseManifest {
        schema: 1,
        app_id: app.id,
        version: app.version,
        channel: channel.to_string(),
        published_at: unix_now(),
        requires_sabine: sabine_service::SabineVersion::current(),
        artifacts,
        signature: String::new(),
    };
    sign_app_release(&mut manifest, &signing_key()?).map_err(|error| error.to_string())?;
    write_output(output, &manifest)
}

/// Lays out the service, daemon and CEF host of a Sabine system release in
/// `output`, as every installation unpacks them.
pub(crate) fn stage_system_release(
    service: &Path,
    host: &Path,
    output: &Path,
) -> Result<(), String> {
    sabine_runtime::remove_directory_if_present(output).map_err(|error| error.to_string())?;
    sabine_service::stage_system_files(output, &absolute_path(service)?, &absolute_path(host)?)
        .map_err(|error| error.to_string())
}

pub(crate) fn write_system_manifest(
    version: &str,
    directory: &Path,
    output: &Path,
) -> Result<(), String> {
    let version = version.trim_start_matches('v');
    if version.is_empty() {
        return Err("system release version is required".to_string());
    }
    if sabine_service::SabineVersion::parse(version)
        != Some(sabine_service::SabineVersion::current())
    {
        return Err(format!(
            "system release version must be {}",
            sabine_service::SABINE_VERSION
        ));
    }
    let directory = absolute_path(directory)?;
    let mut artifacts = BTreeMap::new();
    for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if !path.is_file()
            || path
                .file_name()
                .is_some_and(|name| name == "sabine-release.json")
        {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("invalid artifact name: {}", path.display()))?
            .to_string();
        artifacts.insert(
            name.clone(),
            SystemReleaseArtifact {
                sha256: sha256(&path)?,
                size: std::fs::metadata(&path)
                    .map_err(|error| error.to_string())?
                    .len(),
                url: format!(
                    "https://github.com/Lantharos/Sabine/releases/download/v{version}/{name}"
                ),
            },
        );
    }
    if artifacts.is_empty() {
        return Err("system release directory contains no artifacts".to_string());
    }
    let mut manifest = SystemReleaseManifest {
        schema: 1,
        version: version.to_string(),
        published_at: unix_now(),
        compatibility: sabine_service::SystemCompatibility::current(),
        artifacts,
        signature: String::new(),
    };
    sign_system_release(&mut manifest, &signing_key()?).map_err(|error| error.to_string())?;
    write_output(output, &manifest)
}

fn write_output(output: &Path, manifest: &impl serde::Serialize) -> Result<(), String> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn generate_signing_key(public_output: &Path) -> Result<String, String> {
    let private_key = new_private_key()?;
    let public_key = public_key_from_private(&private_key).map_err(|error| error.to_string())?;
    if let Some(parent) = public_output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(public_output, format!("{public_key}\n")).map_err(|error| error.to_string())?;
    Ok(private_key)
}

pub(crate) fn signing_public_key() -> Result<String, String> {
    public_key_from_private(&signing_key()?).map_err(|error| error.to_string())
}

fn unix_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the system clock is after 1970")
        .as_secs()
        .to_string()
}

fn signing_key() -> Result<String, String> {
    std::env::var("SABINE_UPDATE_SIGNING_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| "SABINE_UPDATE_SIGNING_KEY is required".to_string())
}

fn new_private_key() -> Result<String, String> {
    let mut seed = [0_u8; 32];
    getrandom::fill(&mut seed).map_err(|error| error.to_string())?;
    Ok(STANDARD.encode(seed))
}

fn parse_assignments(values: &[String]) -> Result<BTreeMap<String, String>, String> {
    values
        .iter()
        .map(|value| {
            let (key, value) = value
                .split_once('=')
                .ok_or_else(|| format!("expected target=value, got `{value}`"))?;
            if key.is_empty() || value.is_empty() {
                return Err(format!("expected target=value, got `{value}`"));
            }
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

fn artifact_kind(path: &Path) -> Result<AppArtifactKind, String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name.ends_with(".tar.gz") || name.ends_with(".zip") {
        Ok(AppArtifactKind::Archive)
    } else if name.ends_with(".deb") {
        Ok(AppArtifactKind::Deb)
    } else if name.ends_with(".rpm") {
        Ok(AppArtifactKind::Rpm)
    } else if name.ends_with(".msi") {
        Ok(AppArtifactKind::Msi)
    } else if name.ends_with(".exe") {
        Ok(AppArtifactKind::Exe)
    } else if name.ends_with(".dmg") {
        Ok(AppArtifactKind::Dmg)
    } else if name.ends_with(".appimage") {
        Ok(AppArtifactKind::AppImage)
    } else {
        Err(format!(
            "cannot infer update artifact kind from {}",
            path.display()
        ))
    }
}

fn sha256(path: &Path) -> Result<String, String> {
    sabine_runtime::sha256_file(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))
}
