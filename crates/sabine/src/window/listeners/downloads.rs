use std::{fmt, path::PathBuf, sync::Arc};

use serde::Deserialize;

use crate::WindowId;

/// A download of the window's page started or changed.
pub(crate) const DOWNLOAD_LINE: &str = "SABINE_DOWNLOAD";

/// A download the window's page started, as it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    pub window: WindowId,
    pub id: String,
    pub url: String,
    /// The name the site suggests for the file.
    pub filename: String,
    pub mime_type: String,
    /// The size the site gives, when it gives one.
    pub total_bytes: Option<u64>,
    pub received_bytes: u64,
    pub state: DownloadState,
    /// Where the file is saved, once that is known.
    pub save_path: Option<PathBuf>,
    /// Why an interrupted download stopped, such as `networkFailed`.
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadState {
    Requested,
    Progress,
    Completed,
    Cancelled,
    Interrupted,
}

/// Where a download the page started goes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DownloadDecision {
    SaveTo(PathBuf),
    /// Asks the person where to save it, in the desktop's save dialog.
    #[default]
    Ask,
    Cancel,
}

/// What the app can do to a download under way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadControl {
    Pause,
    Resume,
    Cancel,
}

impl DownloadControl {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadLine {
    download_id: String,
    url: String,
    filename: String,
    mime_type: String,
    total_bytes: i64,
    received_bytes: u64,
    state: DownloadState,
    save_path: String,
    error: Option<String>,
}

type DecideFn = dyn Fn(&Download) -> DownloadDecision + Send + Sync;
type UpdatedFn = dyn Fn(&Download) + Send + Sync;

/// The app's say over its pages' downloads: where each one goes, and what
/// it hears as they run.
#[derive(Clone, Default)]
pub(crate) struct DownloadListeners {
    pub(crate) decide: Option<Arc<DecideFn>>,
    pub(crate) updated: Option<Arc<UpdatedFn>>,
}

impl DownloadListeners {
    pub(crate) fn listens(&self) -> bool {
        self.decide.is_some() || self.updated.is_some()
    }

    /// Hands the download `line` reports to the app, and returns the window
    /// host control that answers a requested one.
    pub(crate) fn deliver(&self, window: WindowId, line: &str) -> Option<String> {
        let download = parse(window, line)?;
        if let Some(updated) = &self.updated {
            updated(&download);
        }
        let decide = self
            .decide
            .as_ref()
            .filter(|_| download.state == DownloadState::Requested)?;
        let answer = match decide(&download) {
            DownloadDecision::SaveTo(path) => {
                serde_json::json!({ "downloadId": download.id, "action": "accept", "savePath": path })
            }
            DownloadDecision::Ask => {
                serde_json::json!({ "downloadId": download.id, "action": "accept", "showDialog": true })
            }
            DownloadDecision::Cancel => {
                serde_json::json!({ "downloadId": download.id, "action": "cancel" })
            }
        };
        Some(answer.to_string())
    }
}

fn parse(window: WindowId, line: &str) -> Option<Download> {
    let json = line.strip_prefix(DOWNLOAD_LINE)?.strip_prefix('\t')?;
    let line: DownloadLine = serde_json::from_str(json).ok()?;
    Some(Download {
        window,
        id: line.download_id,
        url: line.url,
        filename: line.filename,
        mime_type: line.mime_type,
        total_bytes: u64::try_from(line.total_bytes)
            .ok()
            .filter(|bytes| *bytes > 0),
        received_bytes: line.received_bytes,
        state: line.state,
        save_path: Some(PathBuf::from(line.save_path)).filter(|path| !path.as_os_str().is_empty()),
        error: line.error,
    })
}

impl fmt::Debug for DownloadListeners {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DownloadListeners")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(state: &str) -> String {
        format!(
            "{DOWNLOAD_LINE}\t{{\"downloadId\":\"3\",\"url\":\"https://example.com/a.zip\",\"filename\":\"a.zip\",\"mimeType\":\"application/zip\",\"totalBytes\":0,\"receivedBytes\":0,\"state\":\"{state}\",\"savePath\":\"\"}}"
        )
    }

    #[test]
    fn the_app_chooses_where_requested_downloads_go() {
        let listeners = DownloadListeners {
            decide: Some(Arc::new(|download| {
                assert_eq!(download.total_bytes, None);
                DownloadDecision::SaveTo(PathBuf::from("/tmp").join(&download.filename))
            })),
            updated: None,
        };
        let answer: serde_json::Value =
            serde_json::from_str(&listeners.deliver(1, &line("requested")).unwrap()).unwrap();
        assert_eq!(answer["savePath"], "/tmp/a.zip");
        assert_eq!(answer["action"], "accept");
        assert_eq!(listeners.deliver(1, &line("progress")), None);
    }
}
