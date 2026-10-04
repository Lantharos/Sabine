#[derive(Clone, Debug, PartialEq)]
pub struct BridgeCommand {
    pub name: String,
    pub params: serde_json::Value,
    pub origin: Option<String>,
    /// Bytes the page sent with the call, such as a file to save.
    pub body: Option<Vec<u8>>,
    /// The window whose page made the call.
    pub window: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct BridgeResponse {
    pub result: serde_json::Value,
    /// Answers the call with bytes, which the page receives as a `Uint8Array`.
    pub body: Option<Vec<u8>>,
}

impl BridgeResponse {
    pub fn json(result: serde_json::Value) -> Self {
        Self { result, body: None }
    }

    pub fn bytes(body: impl Into<Vec<u8>>) -> Self {
        Self {
            result: serde_json::Value::Null,
            body: Some(body.into()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BridgeError {
    pub message: String,
}

impl BridgeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BridgeError {}

pub type BridgeResult = std::result::Result<BridgeResponse, BridgeError>;
