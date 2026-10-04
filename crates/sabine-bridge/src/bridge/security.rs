use super::command::{BridgeCommand, BridgeError};
use super::handlers::{BridgeHandlers, BridgeOutcome};
use super::registry::{BridgeCommandDescriptor, BridgeRegistry};

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
    registry: BridgeRegistry,
}

impl BridgeRuntime {
    pub fn new(handlers: BridgeHandlers, registry: BridgeRegistry) -> Self {
        Self { handlers, registry }
    }

    /// Dispatch from an authenticated native host after it has authorized the live document.
    /// This entry point must never receive requests directly from page-controlled transports.
    pub fn dispatch_from_authorized_document(&self, command: BridgeCommand) -> BridgeOutcome {
        if let Err(error) = validate_targets(&command, self.registry.descriptor(&command.name)) {
            return BridgeOutcome::Ready(Err(error));
        }
        self.handlers.dispatch(command)
    }
}

fn validate_targets(
    command: &BridgeCommand,
    descriptor: Option<&BridgeCommandDescriptor>,
) -> Result<(), BridgeError> {
    let Some(descriptor) = descriptor else {
        return Ok(());
    };
    if descriptor.targets.is_empty()
        || descriptor
            .targets
            .iter()
            .any(|target| current_bridge_targets().contains(&target.as_str()))
    {
        return Ok(());
    }
    Err(BridgeError::new(format!(
        "Bridge command `{}` is unavailable on this target",
        command.name
    )))
}

fn current_bridge_targets() -> &'static [&'static str] {
    #[cfg(target_os = "linux")]
    {
        &["desktop", "linux"]
    }
    #[cfg(target_os = "windows")]
    {
        &["desktop", "windows"]
    }
    #[cfg(target_os = "macos")]
    {
        &["desktop", "macos"]
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
        let mut registry = BridgeRegistry::default();
        registry.register("notes.list");
        let runtime = BridgeRuntime::new(handlers, registry);

        let response = dispatch(&runtime, "notes.list").unwrap();
        assert_eq!(response.result["name"], "notes.list");
    }

    #[test]
    fn rejects_wrong_target() {
        let mut handlers = BridgeHandlers::default();
        handlers.register("server.only", |_| {
            Ok(BridgeResponse::json(serde_json::json!(true)))
        });
        let mut registry = BridgeRegistry::default();
        registry.register_descriptor(BridgeCommandDescriptor::new("server.only").target("server"));
        let runtime = BridgeRuntime::new(handlers, registry);

        let error = dispatch(&runtime, "server.only").unwrap_err();
        assert!(error.message.contains("unavailable"));
    }
}
