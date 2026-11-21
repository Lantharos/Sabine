use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use super::{
    BuildTarget, BundleFormat, linux, macos,
    stage::StagedBundle,
    windows::{self, signing::WindowsSigning},
};
use crate::{project::BundleApp, ui::Step};

/// Packages the staged bundle with the platform's tool and returns the artifact.
pub(super) fn package_bundle(
    app: &BundleApp,
    format: BundleFormat,
    staged: &StagedBundle,
    signing: Option<&WindowsSigning>,
    step: &Step,
) -> Result<PathBuf, String> {
    step.note("Compressing");
    match format {
        BundleFormat::Portable if format.target() == BuildTarget::Windows => archive(
            &staged.root,
            &artifact_path(app, staged, format, "zip"),
            step,
        ),
        BundleFormat::Portable
        | BundleFormat::Linux
        | BundleFormat::Windows
        | BundleFormat::Macos => archive(
            &staged.root,
            &artifact_path(app, staged, format, "tar.zst"),
            step,
        ),
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
fn archive(source: &Path, artifact: &Path, step: &Step) -> Result<PathBuf, String> {
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
        crate::ui::tool(
            tar.args(["-a", "-c", "-f"])
                .arg(artifact)
                .arg("--")
                .arg(source_name),
        )?;
    } else {
        tar.args(["-c", "-f", "-", "--"]).arg(source_name);
        let measured = Measured {
            step,
            read: 0,
            total: size_of(source),
        };
        compress_tar(&mut tar, artifact, measured)?;
    }
    Ok(artifact.to_path_buf())
}

/// The bytes of every file under `path`, which tar's stream is a little
/// larger than.
fn size_of(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return metadata.len();
    }
    fs::read_dir(path).map_or(0, |entries| {
        entries.flatten().map(|entry| size_of(&entry.path())).sum()
    })
}

/// Shows how much of the staged app has gone into the archive.
struct Measured<'a> {
    step: &'a Step,
    read: u64,
    total: u64,
}

impl Measured<'_> {
    fn add(&mut self, read: usize) {
        self.read += read as u64;
        self.step.bytes(self.read, self.total);
    }
}

fn compress_tar(tar: &mut Command, artifact: &Path, mut measured: Measured) -> Result<(), String> {
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
        let stream = child.stdout.as_mut().expect("tar's output is piped");
        let mut chunk = vec![0; 1 << 20];
        loop {
            let read = stream.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            encoder.write_all(&chunk[..read])?;
            measured.add(read);
        }
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
