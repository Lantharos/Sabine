use super::{BuildTarget, BundleOptions, prepare_bundle};
use crate::install::{
    desktop,
    source::{InstallOptions, SourceApp},
};
use std::{fs, path::PathBuf, process::ExitCode};

pub fn install_bundle(
    options: InstallOptions,
    env_files: Vec<PathBuf>,
) -> Result<ExitCode, String> {
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let (app, format, staged) = prepare_bundle(BundleOptions {
        source: options.source,
        target: "portable".into(),
        out: temporary.path().to_path_buf(),
        release: true,
        no_build: false,
        binary: None,
        no_web_build: false,
        web_build: None,
        web_root: None,
        web_dist: None,
        id: options.id,
        name: options.name,
        version: None,
        json: false,
        offline: false,
        system_wide: false,
        env_files: env_files.clone(),
    })?;
    let service = sabine_service::SabineService::default();
    let directory = service.root().join("apps").join(&app.id);
    let macos = format.target() == BuildTarget::Macos;
    let release = if macos {
        directory.join("install").join(format!("{}.app", app.id))
    } else {
        directory.join("install")
    };
    let payload = if macos {
        staged.app_dir.as_path()
    } else {
        staged
            .binary
            .parent()
            .ok_or("staged binary has no directory")?
    };
    sabine_service::write_install_inventory(payload, &app.id, &[])
        .map_err(|error| error.to_string())?;
    let binary = release.join(
        staged
            .binary
            .strip_prefix(payload)
            .map_err(|error| error.to_string())?,
    );
    let mut updates = app.updates.clone();
    if let Some(update) = &mut updates {
        update.install_mode = sabine_service::AppInstallMode::Managed;
        update.package_kind = None;
    }
    let manifest = sabine_service::AppManifest {
        id: app.id.clone(),
        name: app.name.clone(),
        version: app.version.clone(),
        executable: staged.binary.clone(),
        args: Vec::new(),
        update: updates,
        sabine: sabine_service::SabineVersion::current(),
    };
    manifest.validate().map_err(|error| error.to_string())?;
    let mut log = sabine_runtime::ProgressLog::default();
    sabine_service::prepare_machine_with_progress(Default::default(), None, |progress| {
        if let Some(line) = log.line(&progress.message, progress.fraction) {
            println!("{line}");
        }
    })
    .map_err(|error| error.to_string())?;
    service
        .install_app_payload(payload, &release, manifest, || false)
        .map_err(|error| error.to_string())?;
    let desktop_app = SourceApp {
        id: app.id.clone(),
        name: app.name,
        version: app.version,
        source: app.source_dir,
        command: None,
        icon: if cfg!(windows) && app.icon.is_some() {
            Some(release.join("resources/windows-app.ico"))
        } else {
            app.icon
        },
        associations: app.associations,
        listing: app.listing,
        autostart: options.autostart,
    };
    let source_record = directory.join("source-install.toml");
    if source_record.is_file() {
        fs::remove_file(source_record).map_err(|error| error.to_string())?;
    }
    fs::write(
        directory.join("bundle-install.json"),
        serde_json::to_vec_pretty(&crate::install::BundleInstall {
            source: desktop_app.source.clone(),
            desktop: options.desktop,
            autostart: options.autostart,
            env_files,
        })
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if options.desktop {
        if macos {
            desktop::install_macos_bundle(&desktop_app.id, &release, desktop_app.listing.listed)?;
        } else {
            desktop::install_entry(&desktop_app, &binary)?;
        }
    }
    if options.autostart {
        desktop::install_autostart(&desktop_app, &binary, Some(&desktop_app.id))?;
    }
    println!(
        "Installed {} {} at {}",
        desktop_app.name,
        desktop_app.version,
        binary.display()
    );
    Ok(ExitCode::SUCCESS)
}
