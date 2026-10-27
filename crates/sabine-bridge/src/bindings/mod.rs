mod event;
mod numbers;
mod phases;
mod render;
mod walk;

use std::collections::BTreeMap;

use specta::datatype::DataType;
use specta::{Type, Types};

pub use event::{BridgeEvent, Bytes};

type Definition = fn(&mut Types) -> DataType;

#[derive(Clone, Copy, Debug)]
struct CommandSignature {
    params: Definition,
    result: Definition,
}

/// The typed commands and events an app's pages can use, from which Sabine
/// writes the app's TypeScript bindings.
#[derive(Clone, Debug, Default)]
pub struct BridgeBindings {
    commands: BTreeMap<String, CommandSignature>,
    events: BTreeMap<&'static str, Definition>,
}

impl BridgeBindings {
    pub fn command<Req: Type, Res: Type>(&mut self, name: impl Into<String>) {
        self.commands.insert(
            name.into(),
            CommandSignature {
                params: Req::definition,
                result: Res::definition,
            },
        );
    }

    pub fn event<T: Type>(&mut self, event: &BridgeEvent<T>) {
        self.events.insert(event.name(), T::definition);
    }

    /// Renders the bindings as a TypeScript module that imports
    /// `@lantharos/sabine`.
    pub fn render(&self) -> Result<String, String> {
        render::render(self)
    }
}
