use serde_json::Value;

use super::BridgeRequest;
#[cfg(target_os = "linux")]
use crate::osr::host::OsrHostConfig;
use crate::osr::host::native::OsrNativeHost;
#[cfg(target_os = "linux")]
use crate::window::restore::{self, WindowMemory};

#[cfg(target_os = "linux")]
pub(in crate::osr::host) fn window_memory(config: &OsrHostConfig) -> Option<WindowMemory> {
    let folder = restore::folder(config.app_id.as_deref()?)?;
    Some(WindowMemory::open(folder, config.restoring.clone()))
}

/// Linux desktops that tell windows apart open them again with what they
/// showed. Other desktops open apps anew, so there the page has nothing to
/// get back and nothing to keep.
impl OsrNativeHost {
    pub(super) fn answer_restored_state(&self, request: &BridgeRequest) {
        #[cfg(target_os = "linux")]
        let restored = self
            .memory
            .as_ref()
            .and_then(WindowMemory::restored)
            .cloned()
            .unwrap_or(Value::Null);
        #[cfg(not(target_os = "linux"))]
        let restored = Value::Null;
        self.send_bridge_response(request.browser_id, request.request_id, Ok(restored));
    }

    pub(super) fn keep_state(&mut self, request: &BridgeRequest) {
        #[derive(serde::Deserialize)]
        struct Kept {
            state: Value,
        }
        let result = serde_json::from_str::<Kept>(request.payload)
            .map_err(|error| error.to_string())
            .and_then(|kept| self.keep(&kept.state))
            .map(|()| Value::Null);
        self.send_bridge_response(request.browser_id, request.request_id, result);
    }

    #[cfg(target_os = "linux")]
    fn keep(&mut self, state: &Value) -> Result<(), String> {
        match &mut self.memory {
            Some(memory) if self.compositor.tells_windows_apart => memory.keep(state),
            _ => Ok(()),
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn keep(&mut self, _state: &Value) -> Result<(), String> {
        Ok(())
    }

    /// Forgets what the window showed once people closed it, so it never
    /// opens with it again.
    pub(in crate::osr::host) fn forget_state(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(memory) = &mut self.memory {
            memory.forget();
        }
    }
}
