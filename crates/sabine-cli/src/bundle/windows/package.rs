use std::{fs, path::PathBuf, process::Command};

use super::{WindowsArchitecture, artwork, msi, msi_actions, nsis_script};
use crate::bundle::{
    BundleFormat,
    package::{artifact_path, ensure_parent, run},
    stage::StagedBundle,
};
use crate::project::BundleApp;

pub(in crate::bundle) fn package_msi(
    app: &BundleApp,
    staged: &StagedBundle,
) -> Result<PathBuf, String> {
    let wxs = staged.root.join("installer.wxs");
    let architecture = WindowsArchitecture::of(&staged.binary)?.msvc_name();
    let actions = msi_actions::stage(&staged.root, architecture)?;
    let artifact = artifact_path(app, staged, BundleFormat::Msi, "msi");
    let source_dir = dunce::canonicalize(&staged.app_dir).map_err(|error| error.to_string())?;
    let icon = source_dir.join("resources").join("windows-app.ico");
    let icon = icon.is_file().then(|| icon.display().to_string());
    let dialog_bitmap = app
        .installer_image
        .as_deref()
        .map(|image| artwork::wix_dialog(image, &staged.root))
        .transpose()?
        .map(|bitmap| dunce::simplified(&bitmap).display().to_string());
    fs::write(
        &wxs,
        msi::wix_source(
            app,
            &source_dir.display().to_string(),
            &staged.executable,
            icon.as_deref(),
            dialog_bitmap.as_deref(),
            &dunce::simplified(&actions).display().to_string(),
        )?,
    )
    .map_err(|error| error.to_string())?;
    ensure_parent(&artifact)?;
    run(Command::new("wix")
        .arg("build")
        .args(["-arch", &architecture.to_ascii_lowercase()])
        .arg("-wx")
        .args(["-pdbtype", "none"])
        .args(["-ext", "WixToolset.UI.wixext"])
        .arg(dunce::simplified(&wxs))
        .arg("-o")
        .arg(dunce::simplified(&artifact)))?;
    Ok(artifact)
}

pub(in crate::bundle) fn package_exe(
    app: &BundleApp,
    staged: &StagedBundle,
) -> Result<PathBuf, String> {
    WindowsArchitecture::of(&staged.binary)?;
    let script = staged.root.join("installer.nsi");
    let artifact = artifact_path(app, staged, BundleFormat::Exe, "exe");
    let icon = staged.app_dir.join("resources/windows-app.ico");
    let icon = icon.is_file().then(|| icon.display().to_string());
    let welcome_bitmap = app
        .installer_image
        .as_deref()
        .map(|image| artwork::nsis_welcome(image, &staged.root))
        .transpose()?;
    fs::write(
        &script,
        nsis_script(
            app,
            &staged.app_dir,
            &staged.executable,
            &artifact.display().to_string(),
            icon.as_deref(),
            welcome_bitmap.as_deref(),
        )?,
    )
    .map_err(|error| error.to_string())?;
    ensure_parent(&artifact)?;
    run(Command::new("makensis")
        .arg("-WX")
        .current_dir(&staged.root)
        .arg(dunce::simplified(&script)))?;
    Ok(artifact)
}
