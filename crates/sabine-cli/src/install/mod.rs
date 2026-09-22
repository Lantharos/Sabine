pub(crate) mod assets;
pub(crate) mod desktop;
pub(crate) mod source;
pub mod uninstall;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct BundleInstall {
    pub source: std::path::PathBuf,
    pub desktop: bool,
    pub autostart: bool,
    pub env_files: Vec<std::path::PathBuf>,
}

pub(crate) fn bundle_install(id: &str) -> Result<Option<BundleInstall>, String> {
    if !sabine_service::valid_app_id(id) {
        return Err("invalid app identifier".into());
    }
    let path = sabine_service::service_data_dir()
        .join("apps")
        .join(id)
        .join("bundle-install.json");
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| format!("could not read {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn rebuild_bundle(id: &str, install: BundleInstall) -> Result<(), String> {
    crate::bundle::install_bundle(
        source::InstallOptions {
            source: install.source,
            id: Some(id.into()),
            name: None,
            command: None,
            desktop: install.desktop,
            autostart: install.autostart,
        },
        install.env_files,
    )?;
    Ok(())
}

pub fn run(
    options: source::InstallOptions,
    bundle: bool,
    env_files: Vec<std::path::PathBuf>,
) -> Result<std::process::ExitCode, String> {
    if bundle {
        crate::bundle::install_bundle(options, env_files)
    } else {
        source::install(options)
    }
}
