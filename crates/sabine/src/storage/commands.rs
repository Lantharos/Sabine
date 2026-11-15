//! The store and secrets commands every page of the app can call.

use serde::Deserialize;
use serde_json::Value;

use super::{Secrets, Store};
use crate::bridge::BridgeEventEmitter;
use crate::{BridgeCommand, BridgeError, BridgeResponse, BridgeResult, SabineWindow};

/// Sent to every window when a key of the store changes.
const STORE_CHANGED_EVENT: &str = "store.changed";

#[derive(Deserialize)]
struct Key {
    key: String,
}

#[derive(Deserialize)]
struct Entry {
    key: String,
    value: Value,
}

#[derive(Deserialize)]
struct Secret {
    key: String,
    value: String,
}

fn params<T: serde::de::DeserializeOwned>(command: &BridgeCommand) -> Result<T, BridgeError> {
    serde_json::from_value(command.params.clone())
        .map_err(|error| BridgeError::new(format!("{}: {error}", command.name)))
}

fn json(value: impl serde::Serialize) -> BridgeResult {
    serde_json::to_value(value)
        .map(BridgeResponse::json)
        .map_err(|error| BridgeError::new(error.to_string()))
}

impl SabineWindow {
    pub(crate) fn storage_commands(self, store: &Store, secrets: &Secrets) -> Self {
        let (get, set, delete, entries, clear) = (
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
        );
        let (read, keep, forget) = (secrets.clone(), secrets.clone(), secrets.clone());
        self.bridge_handler("sabine.store.get", move |command| {
            json(get.get(&params::<Key>(&command)?.key))
        })
        .bridge_handler("sabine.store.set", move |command| {
            let Entry { key, value } = params(&command)?;
            set.set(key, value)
                .map_err(|error| BridgeError::new(error.to_string()))?;
            json(Value::Null)
        })
        .bridge_handler("sabine.store.delete", move |command| {
            json(delete.delete(&params::<Key>(&command)?.key))
        })
        .bridge_handler("sabine.store.entries", move |_| json(entries.entries()))
        .bridge_handler("sabine.store.clear", move |_| {
            clear.clear();
            json(Value::Null)
        })
        .bridge_handler_async("sabine.secrets.get", move |command| {
            let secrets = read.clone();
            async move {
                let Key { key } = params(&command)?;
                json(secrets.get(&key).await.map_err(BridgeError::new)?)
            }
        })
        .bridge_handler_async("sabine.secrets.set", move |command| {
            let secrets = keep.clone();
            async move {
                let Secret { key, value } = params(&command)?;
                secrets.set(&key, &value).await.map_err(BridgeError::new)?;
                json(Value::Null)
            }
        })
        .bridge_handler_async("sabine.secrets.delete", move |command| {
            let secrets = forget.clone();
            async move {
                let Key { key } = params(&command)?;
                json(secrets.delete(&key).await.map_err(BridgeError::new)?)
            }
        })
    }
}

/// Tells every window of the app about each change to the store.
pub(crate) fn forward_store_changes(store: &Store, emitter: BridgeEventEmitter) {
    store.on_change(move |change| {
        emitter.emit(
            STORE_CHANGED_EVENT,
            serde_json::json!({ "key": change.key, "value": change.value }),
        );
    });
}
