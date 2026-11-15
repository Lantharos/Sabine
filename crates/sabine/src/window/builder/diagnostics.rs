use crate::{Logging, SabineWindow};

impl SabineWindow {
    /// Sends the app's `log` records to its log file, and the pages' console
    /// messages too when `logging` asks for them.
    pub fn logging(mut self, logging: Logging) -> Self {
        self.config.logging = Some(logging);
        self
    }

    /// Sends the app's crash reports to `url` as multipart `POST` requests,
    /// at most five a day. Without it, reports stay in the app's data folder.
    /// Development runs never send them. `[crash] upload_url` in `Sabine.toml`
    /// sets it too.
    pub fn crash_upload_url(mut self, url: impl Into<String>) -> Self {
        self.config.crash_upload_url = Some(url.into());
        self
    }
}
