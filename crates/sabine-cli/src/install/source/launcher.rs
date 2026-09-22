use super::SourceApp;

pub(super) fn launcher_script(app: &SourceApp) -> Result<String, String> {
    let cli = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut arguments = vec![
        cli.to_string_lossy().into_owned(),
        "dev".into(),
        app.source.to_string_lossy().into_owned(),
    ];
    if let Some(command) = &app.command {
        arguments.extend(["--command".into(), command.clone()]);
    }
    arguments.push("--".into());
    #[cfg(windows)]
    {
        let command = arguments
            .iter()
            .map(|value| format!("\"{}\"", value.replace('%', "%%").replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" ");
        Ok(format!(
            "@echo off\r\nset \"SABINE_APP_ID={}\"\r\n{command} %*\r\n",
            app.id
        ))
    }
    #[cfg(not(windows))]
    {
        let command = arguments
            .iter()
            .map(|value| format!("'{}'", value.replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join(" ");
        Ok(format!(
            "#!/bin/sh\nexport SABINE_APP_ID='{}'\nexec {command} \"$@\"\n",
            app.id
        ))
    }
}
