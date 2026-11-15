use std::{fmt, sync::Arc};

use sabine_service::Permission;
use serde::Deserialize;

use crate::WindowId;

/// A page asks for permissions the app declares, and the app decides.
pub(crate) const PERMISSION_REQUEST_LINE: &str = "SABINE_PERMISSION_REQUEST";

/// What a page asks for: a trusted page of the app's own, or a guest the
/// app let ask for these permissions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRequest {
    pub window: WindowId,
    /// The origin asking, such as `https://meet.example.com`.
    pub origin: String,
    pub permissions: Vec<Permission>,
    /// The guest view asking, when one does instead of the window's page.
    pub guest: Option<String>,
}

/// How the app answers a page's permission request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny,
    /// Lets the person decide in the window, as Sabine does without a
    /// handler. Their choice is remembered for the origin.
    #[default]
    Ask,
}

impl PermissionDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Ask => "ask",
        }
    }
}

#[derive(Deserialize)]
struct RequestLine {
    id: u64,
    origin: String,
    permissions: Vec<String>,
    guest: Option<String>,
}

type PermissionFn = dyn Fn(&PermissionRequest) -> PermissionDecision + Send + Sync;

#[derive(Clone)]
pub(crate) struct PermissionListener(Arc<PermissionFn>);

impl PermissionListener {
    pub(crate) fn new(
        listener: impl Fn(&PermissionRequest) -> PermissionDecision + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(listener))
    }

    /// The window host control that carries the app's answer to the request
    /// `line` holds.
    pub(crate) fn answer(&self, window: WindowId, line: &str) -> Option<String> {
        let json = line
            .strip_prefix(PERMISSION_REQUEST_LINE)?
            .strip_prefix('\t')?;
        let request: RequestLine = serde_json::from_str(json).ok()?;
        let permissions = request
            .permissions
            .iter()
            .map(|name| Permission::parse(name))
            .collect::<Option<Vec<_>>>()?;
        let decision = (self.0)(&PermissionRequest {
            window,
            origin: request.origin,
            permissions,
            guest: request.guest,
        });
        Some(permission_answer(request.id, decision))
    }
}

pub(crate) fn permission_answer(id: u64, decision: PermissionDecision) -> String {
    serde_json::json!({ "id": id, "decision": decision.as_str() }).to_string()
}

impl fmt::Debug for PermissionListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PermissionListener")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_with_the_app_s_decision() {
        let listener = PermissionListener::new(|request| {
            if request.permissions == [Permission::Microphone] && request.guest.is_none() {
                PermissionDecision::Allow
            } else {
                PermissionDecision::Deny
            }
        });
        let line = |permissions: &str| {
            format!(
                "{PERMISSION_REQUEST_LINE}\t{{\"id\":4,\"origin\":\"sabine://app\",\"permissions\":{permissions},\"guest\":null}}"
            )
        };
        assert_eq!(
            listener.answer(1, &line("[\"microphone\"]")).as_deref(),
            Some("{\"decision\":\"allow\",\"id\":4}")
        );
        assert_eq!(
            listener.answer(1, &line("[\"camera\"]")).as_deref(),
            Some("{\"decision\":\"deny\",\"id\":4}")
        );
        assert_eq!(listener.answer(1, &line("[\"teleport\"]")), None);
    }
}
