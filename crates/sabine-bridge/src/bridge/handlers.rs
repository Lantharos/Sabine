use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

use super::command::{BridgeCommand, BridgeError, BridgeResult};

/// The answer of an async handler, which the bridge awaits without holding a
/// worker thread.
pub type BridgeFuture = Pin<Box<dyn Future<Output = BridgeResult> + Send>>;

/// What a dispatched command produced: its answer, or a future that answers it.
pub enum BridgeOutcome {
    Ready(BridgeResult),
    Pending(BridgeFuture),
}

#[derive(Clone)]
enum BridgeHandler {
    Sync(Arc<dyn Fn(BridgeCommand) -> BridgeResult + Send + Sync>),
    Async(Arc<dyn Fn(BridgeCommand) -> BridgeFuture + Send + Sync>),
}

#[derive(Clone, Default)]
pub struct BridgeHandlers {
    handlers: BTreeMap<String, BridgeHandler>,
}

impl std::fmt::Debug for BridgeHandlers {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BridgeHandlers")
            .field("commands", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl BridgeHandlers {
    pub fn register<F>(&mut self, command_name: impl Into<String>, handler: F)
    where
        F: Fn(BridgeCommand) -> BridgeResult + Send + Sync + 'static,
    {
        self.handlers
            .insert(command_name.into(), BridgeHandler::Sync(Arc::new(handler)));
    }

    pub fn register_async<F, Fut>(&mut self, command_name: impl Into<String>, handler: F)
    where
        F: Fn(BridgeCommand) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = BridgeResult> + Send + 'static,
    {
        self.handlers.insert(
            command_name.into(),
            BridgeHandler::Async(Arc::new(move |command| Box::pin(handler(command)))),
        );
    }

    pub fn dispatch(&self, command: BridgeCommand) -> BridgeOutcome {
        match self.handlers.get(&command.name) {
            Some(BridgeHandler::Sync(handler)) => BridgeOutcome::Ready(handler(command)),
            Some(BridgeHandler::Async(handler)) => BridgeOutcome::Pending(handler(command)),
            None => BridgeOutcome::Ready(Err(BridgeError::new(format!(
                "Bridge command `{}` has no handler",
                command.name
            )))),
        }
    }
}
