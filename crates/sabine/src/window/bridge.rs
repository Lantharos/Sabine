use std::future::Future;

use sabine_bridge::{
    BridgeCommand, BridgeCommandDescriptor, BridgeError, BridgeEvent, BridgeResponse, BridgeResult,
};
use specta::Type;

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
    /// serializes the handler return value as JSON. The command is part of
    /// the app's TypeScript bindings.
    pub fn bridge_typed<Req, Res, F>(mut self, command_name: impl Into<String>, handler: F) -> Self
    where
        Req: serde::de::DeserializeOwned + Type,
        Res: serde::Serialize + Type,
        F: Fn(Req) -> Result<Res, BridgeError> + Send + Sync + 'static,
    {
        let command_name = command_name.into();
        self.bridge_bindings
            .command::<Req, Res>(command_name.clone());
        self.bridge_handler(command_name, move |command| {
            let request = serde_json::from_value(command.params)
                .map_err(|error| BridgeError::new(format!("invalid bridge params: {error}")))?;
            json_response(handler(request)?)
        })
    }

    /// Registers an async command that deserializes params into `Req` and
    /// serializes the handler's answer as JSON. It is awaited like
    /// [`Self::bridge_handler_async`].
    pub fn bridge_typed_async<Req, Res, F, Fut>(
        mut self,
        command_name: impl Into<String>,
        handler: F,
    ) -> Self
    where
        Req: serde::de::DeserializeOwned + Type,
        Res: serde::Serialize + Type,
        F: Fn(Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Res, BridgeError>> + Send + 'static,
    {
        let command_name = command_name.into();
        self.bridge_bindings
            .command::<Req, Res>(command_name.clone());
        self.bridge_handler_async(command_name, move |command| {
            let request = serde_json::from_value::<Req>(command.params)
                .map_err(|error| BridgeError::new(format!("invalid bridge params: {error}")));
            let answer = request.map(&handler);
            async move { json_response(answer?.await?) }
        })
    }

    /// Adds an event the app sends with
    /// [`BridgeEventEmitter::send`](crate::BridgeEventEmitter::send) to the
    /// app's TypeScript bindings.
    pub fn bridge_event<T: Type>(mut self, event: &BridgeEvent<T>) -> Self {
        self.bridge_bindings.event(event);
        self
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

fn json_response(answer: impl serde::Serialize) -> BridgeResult {
    let value = serde_json::to_value(answer)
        .map_err(|error| BridgeError::new(format!("failed to encode bridge result: {error}")))?;
    Ok(BridgeResponse::json(value))
}
