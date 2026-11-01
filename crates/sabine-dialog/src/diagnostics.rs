use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use sabine_platform::shell::{Requester, show_folder};

pub fn log_directory() -> PathBuf {
    sabine_runtime::diagnostic_path("startup")
        .parent()
        .expect("diagnostic files have a parent directory")
        .to_path_buf()
}

pub(crate) fn details(message: &str) -> String {
    let failure = message.trim();
    let mut text = format!("{message}\n\nLogs: {}", log_directory().display());
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(300);
    let mut entries = Vec::new();
    for component in ["startup", "setup", "installer", "osr"] {
        let Ok(mut file) = File::open(sabine_runtime::diagnostic_path(component)) else {
            continue;
        };
        let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        let offset = length.saturating_sub(32 * 1024);
        if file.seek(SeekFrom::Start(offset)).is_err() {
            continue;
        }
        let mut bytes = Vec::new();
        if file.take(32 * 1024).read_to_end(&mut bytes).is_err() {
            continue;
        }
        for line in String::from_utf8_lossy(&bytes).lines() {
            let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(time) = entry["time"].as_u64().filter(|time| *time >= since) else {
                continue;
            };
            if let Some(message) = entry["message"].as_str() {
                if message.trim() == failure {
                    continue;
                }
                let mut chars = message
                    .chars()
                    .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t');
                let mut message: String = chars.by_ref().take(1000).collect();
                if chars.next().is_some() {
                    message.push_str("… (full entry in logs)");
                }
                entries.push((time, component, message));
            }
        }
    }
    entries.sort_by_key(|entry| entry.0);
    let start = entries.len().saturating_sub(12);
    if !entries.is_empty() {
        text.push_str("\n\nRecent activity across Sabine apps (last 5 minutes)");
    }
    for (time, component, message) in &entries[start..] {
        let seconds = time % 86400;
        text.push_str(&format!(
            "\n\n{:02}:{:02}:{:02} UTC · {component}\n{}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60,
            message.trim()
        ));
    }
    text
}

pub(crate) fn open_logs(
    finished: impl FnOnce(Result<(), String>) + Send + 'static,
) -> Result<(), String> {
    let directory = log_directory();
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    sabine_platform::async_runtime().spawn(async move {
        let result = show_folder(&directory, Requester::default())
            .await
            .map_err(|error| format!("{error}. Logs: {}", directory.display()));
        if let Err(error) = &result {
            sabine_runtime::report_error("notice", error);
        }
        finished(result);
    });
    Ok(())
}
