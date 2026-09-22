use super::{SourceApp, StagedAssets};
use std::path::Path;

pub(super) fn launcher_script(app: &SourceApp, app_dir: &Path, assets: &StagedAssets) -> String {
    #[cfg(target_os = "windows")]
    {
        let command = app.command.clone().unwrap_or_else(|| {
            format!(
                "cargo run --manifest-path \"{}\" --",
                super::cargo_manifest(&app.source).display()
            )
        });
        let mut environment = String::new();
        let manifest = app.source.join("Sabine.toml");
        if manifest.is_file() {
            environment.push_str(&format!(
                "set \"SABINE_MANIFEST_PATH={}\"\r\n",
                manifest.display()
            ));
        }
        if let Some(web_entry) = &assets.web_entry {
            environment.push_str(&format!(
                "set \"SABINE_WEB_ENTRY={}\"\r\n",
                web_entry.display()
            ));
        }
        format!(
            "@echo off\r\nset \"SABINE_APP_ID={}\"\r\nset \"SABINE_APP_DIR={}\"\r\nset \"SABINE_SOURCE_DIR={}\"\r\n{}cd /d \"{}\"\r\n{} %*\r\n",
            app.id,
            app_dir.display(),
            app.source.display(),
            environment,
            app.source.display(),
            command
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let source = shell_quote(&app.source.display().to_string());
        let mut exports = vec![
            format!("export SABINE_APP_ID={}", shell_quote(&app.id)),
            format!(
                "export SABINE_APP_DIR={}",
                shell_quote(&app_dir.display().to_string())
            ),
            format!(
                "export SABINE_SOURCE_DIR={}",
                shell_quote(&app.source.display().to_string())
            ),
        ];
        let manifest = app.source.join("Sabine.toml");
        if manifest.is_file() {
            exports.push(format!(
                "export SABINE_MANIFEST_PATH={}",
                shell_quote(&manifest.display().to_string())
            ));
        }
        if let Some(web_dir) = &assets.web_dir {
            exports.push(format!(
                "export SABINE_WEB_DIR={}",
                shell_quote(&web_dir.display().to_string())
            ));
        }
        if let Some(web_entry) = &assets.web_entry {
            exports.push(format!(
                "export SABINE_WEB_ENTRY={}",
                shell_quote(&web_entry.display().to_string())
            ));
        }
        let exports = exports.join("\n");
        match &app.command {
            Some(command) => format!(
                "#!/bin/sh\nset -e\n{exports}\ncd {source}\nexec sh -c {} sh \"$@\"\n",
                shell_quote(&format!("{command} \"$@\""))
            ),
            None => format!(
                "#!/bin/sh\nset -e\n{exports}\ncd {source}\nexec cargo run --manifest-path {} -- \"$@\"\n",
                shell_quote(&super::cargo_manifest(&app.source).display().to_string())
            ),
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
