use std::{path::Path, process::Command};

/// Installed copies of an app that updates from GitHub releases read the
/// repository's latest release, and a repository has only one, so such an app
/// has to be the only app its repository holds.
pub(super) fn ensure_only_app(source_dir: &Path) -> Result<(), String> {
    let Some(manifests) = tracked_manifests(source_dir) else {
        return Ok(());
    };
    let others: Vec<&str> = manifests
        .iter()
        .map(String::as_str)
        .filter(|path| *path != "Sabine.toml")
        .collect();
    if others.is_empty() {
        return Ok(());
    }
    Err(format!(
        "this repository also holds {}. Apps that update from GitHub releases have to be the only app in their repository, because installed copies read its latest release; update this app from Schelf or an HTTP source instead",
        others.join(", ")
    ))
}

/// Every `Sabine.toml` git tracks in the repository around `source_dir`,
/// relative to it, or `None` outside a git repository.
fn tracked_manifests(source_dir: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(source_dir)
        .args(["ls-files", "-z", "--", ":(top,glob)**/Sabine.toml"])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    Some(
        String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect(),
    )
}
