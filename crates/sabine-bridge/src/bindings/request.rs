use serde::de::DeserializeOwned;
use specta::datatype::DataType;
use specta::{Type, Types};

use crate::{BridgeCommand, BridgeError};

/// What a typed command's handler takes: its params alone, or [`WithBody`]
/// for its params and the bytes the page sends with the call.
pub trait TypedRequest: Sized {
    /// Whether the command reads bytes sent with the call, which its typed
    /// binding then accepts as the `body` option.
    const BODY: bool;

    fn params_definition(types: &mut Types) -> DataType;

    fn from_command(command: BridgeCommand) -> Result<Self, BridgeError>;
}

impl<T: DeserializeOwned + Type> TypedRequest for T {
    const BODY: bool = false;

    fn params_definition(types: &mut Types) -> DataType {
        T::definition(types)
    }

    fn from_command(command: BridgeCommand) -> Result<Self, BridgeError> {
        params(command.params)
    }
}

/// A typed command's params and the bytes the page sent with the call, such
/// as a file to save, or `None` when it sent none.
#[derive(Clone, Debug, PartialEq)]
pub struct WithBody<T> {
    pub params: T,
    pub body: Option<Vec<u8>>,
}

impl<T: DeserializeOwned + Type> TypedRequest for WithBody<T> {
    const BODY: bool = true;

    fn params_definition(types: &mut Types) -> DataType {
        T::definition(types)
    }

    fn from_command(command: BridgeCommand) -> Result<Self, BridgeError> {
        Ok(Self {
            params: params(command.params)?,
            body: command.body,
        })
    }
}

fn params<T: DeserializeOwned>(params: serde_json::Value) -> Result<T, BridgeError> {
    serde_json::from_value(params)
        .map_err(|error| BridgeError::new(format!("invalid bridge params: {error}")))
}
