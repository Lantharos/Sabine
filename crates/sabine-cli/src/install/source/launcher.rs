use std::path::{Path, PathBuf};

use super::SourceApp;

/// What opens a development install: the program and the arguments it takes.
pub(super) struct Launcher {
    pub program: PathBuf,
    pub arguments: Vec<String>,
}

impl Launcher {
    /// A script that runs `sabine dev` on the checkout.
    #[cfg(unix)]
    pub(super) fn install(app: &SourceApp, app_dir: &Path) -> Result<Self, String> {
        let mut arguments = vec![
            crate::handover::entry_path()?
                .to_string_lossy()
                .into_owned(),
            "dev".into(),
            app.source.to_string_lossy().into_owned(),
        ];
        if let Some(command) = &app.command {
            arguments.extend(["--command".into(), command.clone()]);
        }
        arguments.push("--".into());
        let command = arguments
            .iter()
            .map(|value| format!("'{}'", value.replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join(" ");
        let program = app_dir.join("launch.sh");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nexport SABINE_APP_ID='{}'\nexec {command} \"$@\"\n",
                app.id
            ),
        )
        .map_err(|error| error.to_string())?;
        sabine_runtime::make_executable(&program).map_err(|error| error.to_string())?;
        Ok(Self {
            program,
            arguments: Vec::new(),
        })
    }

    /// `sabine launch`, so no batch file ever parses what Windows passes on.
    #[cfg(windows)]
    pub(super) fn install(app: &SourceApp, _: &Path) -> Result<Self, String> {
        Ok(Self {
            program: crate::handover::entry_path()?,
            arguments: vec!["launch".into(), app.id.clone()],
        })
    }

    #[cfg(windows)]
    pub(super) fn command_line(&self) -> String {
        format!(
            "\"{}\" {}",
            self.program.display(),
            self.arguments.join(" ")
        )
    }
}
