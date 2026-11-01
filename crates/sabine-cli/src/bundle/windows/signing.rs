use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::bundle::signing::run;

pub(in crate::bundle) struct WindowsSigning {
    pub certificate: WindowsCertificate,
    pub timestamp_url: String,
}

pub(in crate::bundle) enum WindowsCertificate {
    File {
        path: PathBuf,
        password: Option<String>,
    },
    Store {
        thumbprint: String,
    },
}

impl WindowsSigning {
    /// Signs the programs and libraries the app ships, leaving the shared
    /// Chromium runtime of an offline bundle as its builders signed it.
    pub(in crate::bundle) fn sign_app(&self, directory: &Path) -> Result<(), String> {
        let mut code = Vec::new();
        collect_code(directory, &directory.join("resources/runtimes"), &mut code)?;
        self.sign(&code)
    }

    pub(in crate::bundle) fn sign(&self, paths: &[PathBuf]) -> Result<(), String> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut command = Command::new("signtool");
        command.args([
            "sign",
            "/fd",
            "SHA256",
            "/td",
            "SHA256",
            "/tr",
            &self.timestamp_url,
        ]);
        match &self.certificate {
            WindowsCertificate::File { path, password } => {
                command.arg("/f").arg(path);
                if let Some(password) = password {
                    command.args(["/p", password]);
                }
            }
            WindowsCertificate::Store { thumbprint } => {
                command.args(["/sha1", thumbprint]);
            }
        }
        run(command.args(paths), "signtool")
    }
}

fn collect_code(directory: &Path, runtimes: &Path, code: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            if path != runtimes {
                collect_code(&path, runtimes, code)?;
            }
        } else if path.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("exe") || extension.eq_ignore_ascii_case("dll")
        }) {
            code.push(path);
        }
    }
    Ok(())
}
