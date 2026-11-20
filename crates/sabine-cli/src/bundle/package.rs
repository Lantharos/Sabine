use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use super::{
    BuildTarget, BundleFormat, linux, macos,
    stage::StagedBundle,
    windows::{self, signing::WindowsSigning},
};
use crate::project::BundleApp;

/// Packages the staged bundle with the platform's tool and returns the artifact.
pub(super) fn package_bundle(
    app: &BundleApp,
    format: BundleFormat,
    staged: &StagedBundle,
    signing: Option<&WindowsSigning>,
) -> Result<PathBuf, String> {
    match format {
        BundleFormat::Portable if format.target() == BuildTarget::Windows => {
            archive(&staged.root, &artifact_path(app, staged, format, "zip"))
        }
        BundleFormat::Portable
        | BundleFormat::Linux
        | BundleFormat::Windows
        | BundleFormat::Macos => {
            archive(&staged.root, &artifact_path(app, staged, format, "tar.zst"))
        }
        BundleFormat::Deb => linux::package::package_deb(app, staged),
        BundleFormat::Rpm => linux::package::package_rpm(app, staged),
        BundleFormat::AppImage => linux::package::package_appimage(app, staged),
        BundleFormat::Dmg => macos::package_dmg(app, staged),
        BundleFormat::Msi => windows::package::package_msi(app, staged),
        BundleFormat::Exe => windows::package::package_exe(app, staged, signing),
    }
}

/// Archives `source` with `tar`: as a zip, which the `tar` of Windows
/// writes, or as a tarball compressed with zstd, whose long-range matching
/// finds the runtime's repeated files.
fn archive(source: &Path, artifact: &Path) -> Result<PathBuf, String> {
    ensure_parent(artifact)?;
    let source_name = source
        .file_name()
        .ok_or_else(|| format!("invalid artifact source {}", source.display()))?;
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let mut tar = Command::new("tar");
    tar.arg("-C").arg(parent);
    if artifact
        .extension()
        .is_some_and(|extension| extension == "zip")
    {
        run(tar
            .args(["-a", "-c", "-f"])
            .arg(artifact)
            .arg("--")
            .arg(source_name))?;
    } else {
        tar.args(["-c", "-f", "-", "--"]).arg(source_name);
        compress_tar(&mut tar, artifact)?;
    }
    Ok(artifact.to_path_buf())
}

fn compress_tar(tar: &mut Command, artifact: &Path) -> Result<(), String> {
    let mut child = tar
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run tar: {error}"))?;
    let compressed = (|| {
        let mut encoder = zstd::stream::write::Encoder::new(fs::File::create(artifact)?, 19)?;
        encoder.long_distance_matching(true)?;
        encoder.window_log(27)?;
        encoder.multithread(
            std::thread::available_parallelism().map_or(1, |threads| threads.get() as u32),
        )?;
        io::copy(
            child.stdout.as_mut().expect("tar's output is piped"),
            &mut encoder,
        )?;
        encoder.finish()?.sync_all()
    })();
    let status = child.wait().map_err(|error| error.to_string())?;
    compressed.map_err(|error| format!("could not compress {}: {error}", artifact.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("tar failed with {status}"))
    }
}

pub(super) fn artifact_path(
    app: &BundleApp,
    staged: &StagedBundle,
    format: BundleFormat,
    extension: &str,
) -> PathBuf {
    staged
        .root
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&staged.root)
        .join("artifacts")
        .join(format!(
            "{}-{}-{}.{}",
            app.id,
            app.version,
            format.as_str(),
            extension
        ))
}

pub(super) fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Runs a packaging tool, which must be installed.
pub(super) fn run(command: &mut Command) -> Result<(), String> {
    let tool = command.get_program().to_string_lossy().into_owned();
    let status = command.status().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("{tool} is required to build this package; install it and retry")
        } else {
            format!("failed to run {tool}: {error}")
        }
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{tool} failed with {status}"))
    }
}
