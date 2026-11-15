use std::sync::Arc;

use sabine_service::{DeclaredPermission, Permission};

use crate::SabineWindow;
use crate::window::{
    Download, DownloadDecision, PermissionDecision, PermissionListener, PermissionRequest,
};

impl SabineWindow {
    /// Lets the app's pages, and guests it allows, ask for `permission`, as
    /// `[permissions]` in `Sabine.toml` does. Every other request is denied.
    pub fn permission(mut self, permission: Permission) -> Self {
        if !self
            .config
            .permissions
            .iter()
            .any(|declared| declared.permission == permission)
        {
            self.config.permissions.push(DeclaredPermission {
                permission,
                reason: None,
            });
        }
        self
    }

    /// Decides the permission requests of the app's pages, and of guests it
    /// allowed to ask, before the person is asked. Requests for permissions
    /// the app doesn't declare are denied without reaching it. `listener`
    /// runs on the thread every window's listeners share, so it should
    /// answer quickly.
    pub fn on_permission_request(
        mut self,
        listener: impl Fn(&PermissionRequest) -> PermissionDecision + Send + Sync + 'static,
    ) -> Self {
        self.config.listeners.permission = Some(PermissionListener::new(listener));
        self
    }

    /// Decides where the downloads the window's page starts go, instead of
    /// asking the person with the desktop's save dialog. Guests' downloads
    /// stay with the page, through `sabine.guest`.
    pub fn on_download(
        mut self,
        listener: impl Fn(&Download) -> DownloadDecision + Send + Sync + 'static,
    ) -> Self {
        self.config.listeners.downloads.decide = Some(Arc::new(listener));
        self
    }

    /// Calls `listener` as each download the window's page started moves
    /// along, from its request to its end.
    pub fn on_download_updated(
        mut self,
        listener: impl Fn(&Download) + Send + Sync + 'static,
    ) -> Self {
        self.config.listeners.downloads.updated = Some(Arc::new(listener));
        self
    }

    /// Turns spellchecking in the window's pages on or off.
    pub fn spellcheck(mut self, enabled: bool) -> Self {
        self.config.spellcheck.enabled = Some(enabled);
        self
    }

    /// The languages the window's pages check spelling in, such as `en-US`.
    pub fn spellcheck_languages(
        mut self,
        languages: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.config.spellcheck.languages = Some(languages.into_iter().map(Into::into).collect());
        self
    }
}
