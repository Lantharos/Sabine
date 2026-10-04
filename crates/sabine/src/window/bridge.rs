use std::future::Future;

use sabine_bridge::{
    BridgeCommand, BridgeCommandDescriptor, BridgeError, BridgeResponse, BridgeResult,
};

use super::SabineWindow;

impl SabineWindow {
    /// Registers a command the window's pages can call. The handler runs on
    /// one of the bridge's worker threads, so it should not wait on slow work;
    /// use [`Self::bridge_handler_async`] for that.
    pub fn bridge_handler<F>(self, command_name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(BridgeCommand) -> BridgeResult + Send + Sync + 'static,
    {
        self.bridge_descriptor_handler(BridgeCommandDescriptor::new(command_name), handler)
    }

    /// Registers a command that deserializes params into `Req` and
    /// serializes the handler return value as JSON.
    pub fn bridge_typed<Req, Res, F>(self, command_name: impl Into<String>, handler: F) -> Self
    where
        Req: serde::de::DeserializeOwned,
        Res: serde::Serialize,
        F: Fn(Req) -> Result<Res, BridgeError> + Send + Sync + 'static,
    {
        self.bridge_handler(command_name, move |command| {
            let request = serde_json::from_value(command.params)
                .map_err(|error| BridgeError::new(format!("invalid bridge params: {error}")))?;
            let response = handler(request)?;
            let value = serde_json::to_value(response).map_err(|error| {
                BridgeError::new(format!("failed to encode bridge result: {error}"))
            })?;
            Ok(BridgeResponse::json(value))
        })
    }

    /// Registers a command limited to the descriptor's targets, which the
    /// descriptor's extra origins may also call.
    pub fn bridge_descriptor_handler<F>(
        mut self,
        descriptor: BridgeCommandDescriptor,
        handler: F,
    ) -> Self
    where
        F: Fn(BridgeCommand) -> BridgeResult + Send + Sync + 'static,
    {
        self.bridge_handlers
            .register(descriptor.name.clone(), handler);
        self.config.bridge.register_descriptor(descriptor);
        self
    }

    /// Registers a command whose handler is awaited on the bridge's async
    /// runtime, so waiting on I/O or timers holds no thread. The runtime is
    /// Tokio, so Tokio timers and I/O work inside the handler.
    pub fn bridge_handler_async<F, Fut>(self, command_name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(BridgeCommand) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = BridgeResult> + Send + 'static,
    {
        self.bridge_descriptor_handler_async(BridgeCommandDescriptor::new(command_name), handler)
    }

    pub fn bridge_descriptor_handler_async<F, Fut>(
        mut self,
        descriptor: BridgeCommandDescriptor,
        handler: F,
    ) -> Self
    where
        F: Fn(BridgeCommand) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = BridgeResult> + Send + 'static,
    {
        self.bridge_handlers
            .register_async(descriptor.name.clone(), handler);
        self.config.bridge.register_descriptor(descriptor);
        self
    }
}
