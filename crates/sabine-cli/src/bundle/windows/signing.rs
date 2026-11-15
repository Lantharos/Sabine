use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(windows)]
use super::certificate::ImportedCertificate;
use crate::bundle::signing::run;

pub(in crate::bundle) struct WindowsSigning {
    thumbprint: String,
    store: Option<String>,
    timestamp_url: String,
    #[cfg(windows)]
    _imported: Option<ImportedCertificate>,
}

impl WindowsSigning {
    pub(in crate::bundle) fn from_store(
        thumbprint: String,
        timestamp_url: String,
    ) -> Result<Self, String> {
        if thumbprint.len() != 40 || !thumbprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(
                "SABINE_WINDOWS_CERTIFICATE_SHA1 must be a 40-digit hexadecimal thumbprint".into(),
            );
        }
        Self::new(thumbprint, None, timestamp_url)
    }

    #[cfg(windows)]
    pub(in crate::bundle) fn from_file(
        path: &Path,
        password: Option<&str>,
        timestamp_url: String,
    ) -> Result<Self, String> {
        let imported = ImportedCertificate::import(path, password)?;
        let mut signing = Self::new(
            imported.thumbprint.clone(),
            Some(imported.store.clone()),
            timestamp_url,
        )?;
        signing._imported = Some(imported);
        Ok(signing)
    }

    #[cfg(not(windows))]
    pub(in crate::bundle) fn from_file(
        _: &Path,
        _: Option<&str>,
        _: String,
    ) -> Result<Self, String> {
        Err("signing with a .pfx certificate needs Windows, where signtool runs".into())
    }

    fn new(
        thumbprint: String,
        store: Option<String>,
        timestamp_url: String,
    ) -> Result<Self, String> {
        if timestamp_url
            .chars()
            .any(|character| character.is_whitespace() || "\"'%$".contains(character))
        {
            return Err(
                "SABINE_WINDOWS_TIMESTAMP_URL must not contain spaces, quotes, % or $".into(),
            );
        }
        Ok(Self {
            thumbprint,
            store,
            timestamp_url,
            #[cfg(windows)]
            _imported: None,
        })
    }

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
        run(
            Command::new("signtool").args(self.arguments()).args(paths),
            "signtool",
        )
    }

    /// The NSIS instruction that signs the uninstaller setup writes.
    pub(in crate::bundle) fn nsis_uninstaller(&self) -> String {
        let command = std::iter::once("signtool".to_string())
            .chain(self.arguments())
            .map(|argument| format!("\"{argument}\""))
            .collect::<Vec<_>>()
            .join(" ");
        format!("!uninstfinalize '{command} \"%1\"' = 0\n")
    }

    fn arguments(&self) -> Vec<String> {
        let mut arguments = [
            "sign",
            "/fd",
            "SHA256",
            "/td",
            "SHA256",
            "/tr",
            &self.timestamp_url,
            "/sha1",
            &self.thumbprint,
        ]
        .map(str::to_string)
        .to_vec();
        if let Some(store) = &self.store {
            arguments.extend(["/s".to_string(), store.clone()]);
        }
        arguments
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
