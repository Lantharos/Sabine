use super::{BundleFormat, BundleOptions, prepare_bundle};
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
    let target = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "portable"
    };
    let (app, format, staged) = prepare_bundle(BundleOptions {
        source: options.source,
        target: target.into(),
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
        env_files: env_files.clone(),
    })?;
    let service = sabine_service::SabineService::default();
    let directory = service.root().join("apps").join(&app.id);
    let release = if format == BundleFormat::Macos {
        directory.join("install").join(format!("{}.app", app.id))
    } else {
        directory.join("install")
    };
    let payload = if format == BundleFormat::Macos {
        staged.app_dir.as_path()
    } else {
        staged
            .binary
            .parent()
            .ok_or("staged binary has no directory")?
    };
    write_inventory(payload, &app.id)?;
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
    sabine_service::prepare_machine_with_progress(Default::default(), None, |progress| {
        println!("{}", progress.message)
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
        mime_types: app.mime_types,
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
        if format == BundleFormat::Macos {
            desktop::link_macos_bundle(&desktop_app.id, &release)?;
        } else {
            desktop::install_entry(&desktop_app, &binary)?;
        }
    }
    if options.autostart {
        desktop::install_autostart(&desktop_app, &binary, Some(&app.id))?;
    }
    println!(
        "Installed {} {} at {}",
        desktop_app.name,
        desktop_app.version,
        binary.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn write_inventory(root: &std::path::Path, id: &str) -> Result<(), String> {
    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                directories.push(path);
            } else {
                files.push(
                    path.strip_prefix(root)
                        .map_err(|error| error.to_string())?
                        .to_path_buf(),
                );
            }
        }
    }
    fs::write(
        root.join(".sabine-install.json"),
        serde_json::to_vec(&serde_json::json!({ "id": id, "files": files }))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}
