use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

use crate::bundle::signing::run;

/// Code bundles that carry a signature of their own.
const CODE_BUNDLES: &[&str] = &["app", "framework", "appex", "xpc", "bundle", "plugin"];

/// The hardened runtime exceptions Chromium's helpers need: V8 and
/// SwiftShader compile code as they run, and plugins load unsigned code.
const HELPER_ENTITLEMENTS: &[(&str, &str)] = &[
    (
        " Helper (Renderer).app",
        include_str!("entitlements/jit.plist"),
    ),
    (" Helper (GPU).app", include_str!("entitlements/jit.plist")),
    (
        " Helper (Plugin).app",
        include_str!("entitlements/plugin.plist"),
    ),
];

pub(in crate::bundle) struct MacosSigning {
    pub identity: String,
    pub notary: Option<Notary>,
}

pub(in crate::bundle) enum Notary {
    ApiKey {
        key: PathBuf,
        key_id: String,
        issuer: String,
    },
    KeychainProfile(String),
}

impl MacosSigning {
    /// Signs every piece of code in `bundle` before the bundle that holds
    /// it, ending with `bundle` itself, all with the hardened runtime.
    pub(in crate::bundle) fn sign_app(&self, bundle: &Path) -> Result<(), String> {
        let entitlements = Entitlements::write()?;
        self.sign_contents(bundle, &entitlements)?;
        self.sign(bundle, Hardening::App(&entitlements))
    }

    /// Notarizes the signed `bundle` and staples the ticket to it, so it
    /// opens without a network check wherever it is unpacked.
    pub(in crate::bundle) fn notarize_app(&self, bundle: &Path) -> Result<(), String> {
        let Some(notary) = &self.notary else {
            return Ok(());
        };
        let archive = bundle.with_extension("notarization.zip");
        run(
            Command::new("ditto")
                .args(["-c", "-k", "--keepParent"])
                .arg(bundle)
                .arg(&archive),
            "ditto",
        )?;
        let notarized = notary.submit(&archive);
        let _ = fs::remove_file(&archive);
        notarized?;
        notary.staple(bundle)
    }

    pub(in crate::bundle) fn sign_disk_image(&self, image: &Path) -> Result<(), String> {
        self.sign(image, Hardening::None)?;
        if let Some(notary) = &self.notary {
            notary.submit(image)?;
            notary.staple(image)?;
        }
        Ok(())
    }

    fn sign_contents(&self, directory: &Path, entitlements: &Entitlements) -> Result<(), String> {
        for path in sorted_entries(directory)? {
            let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if metadata.is_dir() {
                self.sign_contents(&path, entitlements)?;
                if has_extension(&path, &["app"]) {
                    self.sign(&path, Hardening::App(entitlements))?;
                } else if has_extension(&path, CODE_BUNDLES) {
                    self.sign(&path, Hardening::Preserved)?;
                }
            } else if metadata.is_file() && is_mach_o(&path)? {
                self.sign(&path, Hardening::Preserved)?;
            }
        }
        Ok(())
    }

    fn sign(&self, path: &Path, hardening: Hardening) -> Result<(), String> {
        let mut command = Command::new("codesign");
        command.args(["--force", "--timestamp", "--sign", &self.identity]);
        match hardening {
            Hardening::None => {}
            Hardening::Preserved => {
                command.args(["--options", "runtime", "--preserve-metadata=entitlements"]);
            }
            Hardening::App(entitlements) => {
                command.args(["--options", "runtime"]);
                if let Some(plist) = entitlements.of(path) {
                    command.arg("--entitlements").arg(plist);
                }
            }
        }
        run(command.arg(path), "codesign")
    }
}

impl Notary {
    fn submit(&self, path: &Path) -> Result<(), String> {
        let mut command = Command::new("xcrun");
        command
            .args(["notarytool", "submit"])
            .arg(path)
            .arg("--wait");
        match self {
            Self::ApiKey {
                key,
                key_id,
                issuer,
            } => command
                .arg("--key")
                .arg(key)
                .args(["--key-id", key_id, "--issuer", issuer]),
            Self::KeychainProfile(profile) => command.args(["--keychain-profile", profile]),
        };
        run(&mut command, "notarytool")
    }

    fn staple(&self, path: &Path) -> Result<(), String> {
        run(
            Command::new("xcrun").args(["stapler", "staple"]).arg(path),
            "stapler",
        )
    }
}

enum Hardening<'a> {
    None,
    Preserved,
    App(&'a Entitlements),
}

/// The helpers' entitlements, written out for `codesign` to read.
struct Entitlements {
    directory: tempfile::TempDir,
}

impl Entitlements {
    fn write() -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        for (index, (_, plist)) in HELPER_ENTITLEMENTS.iter().enumerate() {
            fs::write(directory.path().join(format!("{index}.plist")), plist)
                .map_err(|error| error.to_string())?;
        }
        Ok(Self { directory })
    }

    fn of(&self, app: &Path) -> Option<PathBuf> {
        let name = app.file_name()?.to_str()?;
        HELPER_ENTITLEMENTS
            .iter()
            .position(|(suffix, _)| name.ends_with(suffix))
            .map(|index| self.directory.path().join(format!("{index}.plist")))
    }
}

fn sorted_entries(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort();
    Ok(entries)
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extensions.contains(&extension))
}

fn is_mach_o(path: &Path) -> Result<bool, String> {
    let mut magic = [0_u8; 4];
    let read = fs::File::open(path)
        .and_then(|mut file| file.read(&mut magic))
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    Ok(read == 4
        && matches!(
            magic,
            [0xcf, 0xfa, 0xed, 0xfe] | [0xce, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe]
        ))
}
