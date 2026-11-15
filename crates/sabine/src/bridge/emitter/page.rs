use super::{BridgeEventEmitter, HOST_CONTROL_PREFIX};
use crate::host::WindowId;

impl BridgeEventEmitter {
    /// Shows a desktop notification from one window. Its pages hear about
    /// clicks through `notification.click` events carrying its id.
    pub fn notify(&self, window: WindowId, notification: &sabine_platform::Notification) -> bool {
        self.deliver(
            Some(window),
            format!(
                "{HOST_CONTROL_PREFIX}\tnotify\t{}",
                crate::osr::protocol::notification_to_json(notification)
            ),
            None,
        )
    }

    /// Prints PDF, image and text files as one document from one window,
    /// through the desktop's print dialog. Pages get the outcome from
    /// `print.files` instead.
    pub fn print_files(
        &self,
        window: WindowId,
        paths: &[std::path::PathBuf],
        title: Option<&str>,
    ) -> bool {
        self.deliver(
            Some(window),
            format!(
                "{HOST_CONTROL_PREFIX}\tprint\t{}",
                serde_json::json!({ "paths": paths, "title": title })
            ),
            None,
        )
    }

    /// Drives the guests of one window from Rust, for example to restore a
    /// session's tabs before its page loads. Pages see the results through
    /// guest events; prefer `sabine.guest` for work the page drives.
    pub fn guest_control(
        &self,
        window: WindowId,
        control: &sabine_bridge::GuestHostControl,
    ) -> bool {
        self.deliver(
            Some(window),
            format!(
                "{HOST_CONTROL_PREFIX}\t{}\t{}",
                control.command_name(),
                control.to_host_value()
            ),
            None,
        )
    }

    /// Pauses, resumes or cancels a download the window's page started.
    pub fn download_control(
        &self,
        window: WindowId,
        download_id: &str,
        control: crate::DownloadControl,
    ) -> bool {
        let value = serde_json::json!({ "downloadId": download_id, "action": control.as_str() });
        self.host_control(Some(window), "download", &value.to_string())
    }

    /// Turns spellchecking in one window's pages on or off and picks its
    /// languages, as `spellcheck` does when the window opens.
    pub fn set_spellcheck(&self, window: WindowId, spellcheck: &crate::Spellcheck) -> bool {
        self.host_control(
            Some(window),
            "spellcheck.set",
            &spellcheck.to_json().to_string(),
        )
    }

    /// Adds a word to the custom dictionary of the window's profile, which
    /// spellchecking then accepts in every window of the app.
    pub fn add_spellcheck_word(&self, window: WindowId, word: &str) -> bool {
        let value = serde_json::json!({ "word": word });
        self.host_control(Some(window), "spellcheck.addWord", &value.to_string())
    }

    pub(crate) fn answer_window(&self, window: WindowId, command: &str, answer: &str) -> bool {
        self.host_control(Some(window), command, answer)
    }
}
