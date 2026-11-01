use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

use crate::bundle::signing::run;

/// Code bundles that carry a signature of their own.
const CODE_BUNDLES: &[&str] = &["app", "framework", "appex", "xpc", "bundle", "plugin"];

pub(in crate::bundle) struct MacosSigning {
    pub identity: String,
    pub notary: Option<Notary>,
}

pub(in crate::bundle) struct Notary {
    pub apple_id: String,
    pub team_id: String,
    pub password: String,
}

impl MacosSigning {
    /// Signs every piece of code in `bundle` before the bundle that holds
    /// it, ending with `bundle` itself, all with the hardened runtime.
    pub(in crate::bundle) fn sign_app(&self, bundle: &Path) -> Result<(), String> {
        self.sign_contents(bundle)?;
        self.sign(bundle, true)
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
        self.sign(image, false)?;
        if let Some(notary) = &self.notary {
            notary.submit(image)?;
            notary.staple(image)?;
        }
        Ok(())
    }

    fn sign_contents(&self, directory: &Path) -> Result<(), String> {
        for path in sorted_entries(directory)? {
            let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if metadata.is_dir() {
                self.sign_contents(&path)?;
                if is_code_bundle(&path) {
                    self.sign(&path, true)?;
                }
            } else if metadata.is_file() && is_mach_o(&path)? {
                self.sign(&path, true)?;
            }
        }
        Ok(())
    }

    fn sign(&self, path: &Path, hardened: bool) -> Result<(), String> {
        let mut command = Command::new("codesign");
        command.args(["--force", "--timestamp", "--sign", &self.identity]);
        if hardened {
            command.args(["--options", "runtime", "--preserve-metadata=entitlements"]);
        }
        run(command.arg(path), "codesign")
    }
}

impl Notary {
    fn submit(&self, path: &Path) -> Result<(), String> {
        run(
            Command::new("xcrun")
                .args(["notarytool", "submit"])
                .arg(path)
                .args([
                    "--apple-id",
                    &self.apple_id,
                    "--team-id",
                    &self.team_id,
                    "--password",
                    &self.password,
                    "--wait",
                ]),
            "notarytool",
        )
    }

    fn staple(&self, path: &Path) -> Result<(), String> {
        run(
            Command::new("xcrun").args(["stapler", "staple"]).arg(path),
            "stapler",
        )
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

fn is_code_bundle(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| CODE_BUNDLES.contains(&extension))
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
