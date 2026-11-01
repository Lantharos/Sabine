pub(super) mod signing;

use std::{path::PathBuf, process::Command};

use crate::bundle::{
    BundleFormat,
    package::{artifact_path, ensure_parent, run},
    stage::StagedBundle,
};
use crate::project::BundleApp;

pub(super) fn package_dmg(app: &BundleApp, staged: &StagedBundle) -> Result<PathBuf, String> {
    let artifact = artifact_path(app, staged, BundleFormat::Dmg, "dmg");
    ensure_parent(&artifact)?;
    run(Command::new("hdiutil")
        .arg("create")
        .arg("-volname")
        .arg(&app.name)
        .arg("-srcfolder")
        .arg(&staged.root)
        .arg("-ov")
        .arg("-format")
        .arg("UDZO")
        .arg(&artifact))?;
    Ok(artifact)
}
