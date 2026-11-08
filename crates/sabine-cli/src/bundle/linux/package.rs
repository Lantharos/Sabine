use crate::bundle::{
    BundleFormat,
    package::{artifact_path, ensure_parent, run},
    stage::StagedBundle,
};
use crate::project::BundleApp;
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
};

const CEF_RPM_LIBRARIES: &[&str] = &[
    "libgtk-3.so.0",
    "libnss3.so",
    "libnspr4.so",
    "libasound.so.2",
    "libcups.so.2",
    "libXcomposite.so.1",
    "libXdamage.so.1",
    "libXrandr.so.2",
    "libgbm.so.1",
    "libxkbcommon.so.0",
    "libudev.so.1",
    "libwayland-client.so.0",
];

const CEF_DEB_DEPENDENCIES: &str = "libgtk-3-0t64 | libgtk-3-0, libnss3, libnspr4, libasound2t64 | libasound2, libcups2t64 | libcups2, libxcomposite1, libxdamage1, libxrandr2, libgbm1, libxkbcommon0, libudev1, libwayland-client0";

pub(in crate::bundle) fn architecture(
    binary: &Path,
) -> Result<(&'static str, &'static str), String> {
    let mut header = [0; 20];
    fs::File::open(binary)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|error| error.to_string())?;
    if &header[..4] != b"\x7fELF" || header[4] != 2 || header[5] != 1 {
        return Err("Linux packages require a 64-bit little-endian ELF executable".into());
    }
    match u16::from_le_bytes([header[18], header[19]]) {
        62 => Ok(("amd64", "x86_64")),
        183 => Ok(("arm64", "aarch64")),
        _ => Err("Linux packages support x86_64 and aarch64".into()),
    }
}

pub(in crate::bundle) fn package_deb(
    app: &BundleApp,
    staged: &StagedBundle,
) -> Result<PathBuf, String> {
    let debian = staged.app_dir.join("DEBIAN");
    fs::create_dir_all(&debian).map_err(|error| error.to_string())?;
    fs::write(
        debian.join("control"),
        deb_control(
            app,
            &staged.binary,
            directory_size_kb(&staged.app_dir)?,
            &deb_dependencies(staged)?,
        )?,
    )
    .map_err(|error| error.to_string())?;
    let artifact = artifact_path(app, staged, BundleFormat::Deb, "deb");
    ensure_parent(&artifact)?;
    run(Command::new("dpkg-deb")
        .arg("--root-owner-group")
        .arg("--build")
        .arg(&staged.app_dir)
        .arg(&artifact))?;
    Ok(artifact)
}

pub(in crate::bundle) fn package_rpm(
    app: &BundleApp,
    staged: &StagedBundle,
) -> Result<PathBuf, String> {
    let spec = staged.root.join(format!("{}.spec", app.id));
    fs::write(
        &spec,
        rpm_spec(
            app,
            &staged.executable,
            &staged.binary,
            &share_files(&staged.app_dir)?,
        )?,
    )
    .map_err(|error| error.to_string())?;
    let artifact = artifact_path(app, staged, BundleFormat::Rpm, "rpm");
    let rpm_dir = staged.root.join("rpms");
    fs::create_dir_all(&rpm_dir).map_err(|error| error.to_string())?;
    let spec = fs::canonicalize(&spec).map_err(|error| error.to_string())?;
    let rpm_dir = fs::canonicalize(&rpm_dir).map_err(|error| error.to_string())?;
    let source_dir = fs::canonicalize(&staged.app_dir).map_err(|error| error.to_string())?;
    run(Command::new("rpmbuild")
        .arg("-bb")
        .arg(&spec)
        .arg("--buildroot")
        .arg(staged.root.join("rpm-buildroot"))
        .arg("--define")
        .arg(format!("_rpmdir {}", rpm_dir.display()))
        .arg("--define")
        .arg(format!(
            "_topdir {}",
            staged.root.join("rpmbuild").display()
        ))
        .arg("--define")
        .arg(format!("sabine_source {}", source_dir.display())))?;
    let built = find_file_with_extension(&rpm_dir, "rpm")
        .ok_or_else(|| "rpmbuild completed without producing an RPM".to_string())?;
    ensure_parent(&artifact)?;
    fs::copy(built, &artifact).map_err(|error| error.to_string())?;
    Ok(artifact)
}

pub(in crate::bundle) fn package_appimage(
    app: &BundleApp,
    staged: &StagedBundle,
) -> Result<PathBuf, String> {
    let artifact = artifact_path(app, staged, BundleFormat::AppImage, "AppImage");
    let (_, architecture) = architecture(&staged.binary)?;
    ensure_parent(&artifact)?;
    run(Command::new("appimagetool")
        .env("ARCH", architecture)
        .arg(&staged.app_dir)
        .arg(&artifact))?;
    Ok(artifact)
}

/// The files staged under `usr/share`, as absolute paths on the target system.
fn share_files(app_dir: &Path) -> Result<Vec<String>, String> {
    fn walk(root: &Path, directory: &Path, files: &mut Vec<String>) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(root, &path, files)?;
            } else if let Ok(relative) = path.strip_prefix(root) {
                files.push(format!("/{}", relative.to_string_lossy()));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(app_dir, &app_dir.join("usr/share"), &mut files).map_err(|error| error.to_string())?;
    files.sort();
    Ok(files)
}

fn find_file_with_extension(root: &Path, extension: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_with_extension(&path, extension) {
                return Some(found);
            }
        } else if path
            .extension()
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(extension))
        {
            return Some(path);
        }
    }
    None
}

fn directory_size_kb(path: &Path) -> Result<u64, String> {
    fn walk(path: &Path, total: &mut u64) -> io::Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                walk(&entry.path(), total)?;
            } else {
                *total += metadata.len();
            }
        }
        Ok(())
    }
    let mut bytes = 0;
    walk(path, &mut bytes).map_err(|error| error.to_string())?;
    Ok(bytes.div_ceil(1024))
}

fn deb_control(
    app: &BundleApp,
    binary: &Path,
    installed_size_kb: u64,
    dependencies: &str,
) -> Result<String, String> {
    let maintainer = app
        .maintainer
        .as_deref()
        .filter(|value| {
            value.rsplit_once('<').is_some_and(|(name, email)| {
                !name.trim().is_empty()
                    && email.ends_with('>')
                    && email.contains('@')
                    && !email.chars().any(char::is_whitespace)
            })
        })
        .ok_or(
            "Debian packaging requires app.maintainer or a Cargo author in the form Name <email>",
        )?;
    let version = package_version(&app.version)?;
    let (architecture, _) = architecture(binary)?;
    Ok(format!(
        "Package: {}\nVersion: {version}\nSection: utils\nPriority: optional\nArchitecture: {architecture}\nMaintainer: {maintainer}\nInstalled-Size: {}\nDepends: {dependencies}\nDescription: {}\n{}",
        debian_package_name(&app.id),
        installed_size_kb.max(1),
        app.name,
        deb_description(app.description.as_deref()),
    ))
}

/// Debian package names are lowercase and have no underscores, unlike the
/// app id that names everything the package installs.
fn debian_package_name(app_id: &str) -> String {
    app_id.to_ascii_lowercase().replace('_', "-")
}

/// The extended description below a control file's first line: each line
/// indented by a space, and blank lines as a lone dot.
fn deb_description(description: Option<&str>) -> String {
    description
        .into_iter()
        .flat_map(|text| text.trim().lines())
        .map(|line| match line.trim() {
            "" => " .\n".to_string(),
            line => format!(" {line}\n"),
        })
        .collect()
}

fn deb_dependencies(staged: &StagedBundle) -> Result<String, String> {
    if sabine_runtime::find_program("dpkg-shlibdeps").is_none() {
        return Err("Debian packaging requires dpkg-dev on the target Debian/Ubuntu build environment to calculate library dependencies".into());
    }
    let debian = staged.root.join("debian");
    fs::create_dir_all(&debian).map_err(|error| error.to_string())?;
    fs::write(debian.join("control"), "Source: sabine-package\n\nPackage: sabine-package\nArchitecture: any\nDescription: Application package\n").map_err(|error| error.to_string())?;
    let output = Command::new("dpkg-shlibdeps")
        .arg("-O")
        .arg(format!("-e{}", staged.binary.display()))
        .current_dir(&staged.root)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "could not calculate Debian dependencies: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let output = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let libraries = output
        .lines()
        .find_map(|line| line.strip_prefix("shlibs:Depends="))
        .ok_or("dpkg-shlibdeps did not report shared library dependencies")?;
    Ok(if libraries.is_empty() {
        CEF_DEB_DEPENDENCIES.to_string()
    } else {
        format!("{libraries}, {CEF_DEB_DEPENDENCIES}")
    })
}

fn rpm_spec(
    app: &BundleApp,
    executable: &str,
    binary: &Path,
    share_files: &[String],
) -> Result<String, String> {
    let license = app
        .license
        .as_deref()
        .ok_or("RPM packaging requires app.license or Cargo package license")?;
    let (_, architecture) = architecture(binary)?;
    Ok(format!(
        r#"Name: {name}
Version: {version}
Release: 1%{{?dist}}
Summary: {summary}
License: {license}
BuildArch: {architecture}
%global source_date_epoch_from_changelog 0
{requires}

%description
{description}

%prep

%build

%install
mkdir -p "%{{buildroot}}"
cp -a "%{{sabine_source}}/." "%{{buildroot}}/"

%files
/usr/bin/{executable}
/usr/lib/sabine/{id}
{share_files}"#,
        name = app.id.replace('.', "-"),
        version = package_version(&app.version)?.replace('-', "_"),
        summary = app.name.replace('%', "%%"),
        description = app
            .description
            .as_deref()
            .unwrap_or(&app.name)
            .trim()
            .replace('%', "%%"),
        license = license.replace('%', "%%"),
        id = app.id,
        requires = CEF_RPM_LIBRARIES
            .iter()
            .map(|library| format!("Requires: {library}()(64bit)\n"))
            .collect::<String>()
            .trim_end(),
        share_files = share_files
            .iter()
            .map(|file| format!("{file}\n"))
            .collect::<String>(),
    ))
}

fn package_version(version: &str) -> Result<String, String> {
    let version = semver::Version::parse(version).map_err(|error| error.to_string())?;
    let core = format!("{}.{}.{}", version.major, version.minor, version.patch);
    Ok(if version.pre.is_empty() {
        core
    } else {
        format!("{core}~{}", version.pre)
    })
}
