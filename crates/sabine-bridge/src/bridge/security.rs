use super::command::BridgeCommand;
use super::handlers::{BridgeHandlers, BridgeOutcome};

/// Remote origins whose documents may use the bridge. The app's own pages
/// always may.
#[derive(Clone, Debug, Default)]
pub struct ContentSecurity {
    pub remote_content: bool,
    pub allowed_origins: Vec<String>,
}

impl ContentSecurity {
    pub fn allow_origin(mut self, origin: impl Into<String>) -> Self {
        self.remote_content = true;
        let origin = origin.into();
        if !self
            .allowed_origins
            .iter()
            .any(|allowed| allowed == &origin)
        {
            self.allowed_origins.push(origin);
        }
        self
    }

    pub fn remote_content(mut self, enabled: bool) -> Self {
        self.remote_content = enabled;
        self
    }
}

#[derive(Clone, Debug)]
pub struct BridgeRuntime {
    handlers: BridgeHandlers,
}

impl BridgeRuntime {
    pub fn new(handlers: BridgeHandlers) -> Self {
        Self { handlers }
    }

    /// Dispatch from an authenticated native host after it has authorized the live document.
    /// This entry point must never receive requests directly from page-controlled transports.
    pub fn dispatch_from_authorized_document(&self, command: BridgeCommand) -> BridgeOutcome {
        self.handlers.dispatch(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::BridgeResponse;

    fn command(name: &str) -> BridgeCommand {
        BridgeCommand {
            name: name.to_string(),
            params: serde_json::json!({ "value": 1 }),
            origin: Some("sabine://app".to_string()),
            body: None,
            window: None,
        }
    }

    fn dispatch(runtime: &BridgeRuntime, name: &str) -> crate::BridgeResult {
        match runtime.dispatch_from_authorized_document(command(name)) {
            BridgeOutcome::Ready(result) => result,
            BridgeOutcome::Pending(_) => panic!("expected a synchronous answer"),
        }
    }

    #[test]
    fn dispatches_registered_command() {
        let mut handlers = BridgeHandlers::default();
        handlers.register("notes.list", |command| {
            Ok(BridgeResponse::json(
                serde_json::json!({ "name": command.name }),
            ))
        });
        let runtime = BridgeRuntime::new(handlers);

        let response = dispatch(&runtime, "notes.list").unwrap();
        assert_eq!(response.result["name"], "notes.list");
    }
}
