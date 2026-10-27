use std::marker::PhantomData;

use specta::datatype::DataType;
use specta::{Type, Types};

/// An event the app sends to its pages, whose listeners receive a `T`.
#[derive(Debug)]
pub struct BridgeEvent<T> {
    name: &'static str,
    payload: PhantomData<fn() -> T>,
}

impl<T> BridgeEvent<T> {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            payload: PhantomData,
        }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }
}

impl<T> Clone for BridgeEvent<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for BridgeEvent<T> {}

/// The payload of an event sent as bytes, which listeners receive as a
/// `Uint8Array`.
#[derive(Clone, Copy, Debug)]
pub struct Bytes;

impl Type for Bytes {
    fn definition(_: &mut Types) -> DataType {
        DataType::Reference(specta_typescript::define("Uint8Array"))
    }
}
