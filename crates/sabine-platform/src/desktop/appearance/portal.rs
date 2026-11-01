use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use tokio::task::JoinHandle;
use winit::window::{Theme, Window};
use zbus::{Connection, OwnedVariant};

use super::{Appearance, channel};
use crate::portal;

const SETTINGS: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const COLOR_SCHEME: &str = "color-scheme";
const ACCENT_COLOR: &str = "accent-color";
const PREFER_DARK: u32 = 1;
const PREFER_LIGHT: u32 = 2;

/// Follows the appearance the XDG desktop portal reports, waking the window
/// whenever it changes.
pub struct AppearanceWatcher {
    state: Arc<Mutex<PortalAppearance>>,
    watch: JoinHandle<()>,
}

#[derive(Default)]
struct PortalAppearance {
    scheme: u32,
    accent: Option<[u8; 3]>,
}

impl PortalAppearance {
    fn apply(&mut self, key: &str, value: OwnedVariant) {
        match key {
            COLOR_SCHEME => self.scheme = u32::try_from(value).unwrap_or_default(),
            ACCENT_COLOR => self.accent = <(f64, f64, f64)>::try_from(value).ok().and_then(srgb),
            _ => {}
        }
    }
}

impl AppearanceWatcher {
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let state = Arc::new(Mutex::new(PortalAppearance::default()));
        let watched = Arc::clone(&state);
        let watch = crate::async_runtime().spawn(async move {
            let _ = watch(&watched, &wake).await;
        });
        Self { state, watch }
    }

    /// The portal's appearance. Without a light or dark preference, the
    /// window's own theme decides.
    pub fn appearance(&self, window: &dyn Window) -> Appearance {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        Appearance {
            dark: match state.scheme {
                PREFER_DARK => true,
                PREFER_LIGHT => false,
                _ => window.theme() == Some(Theme::Dark),
            },
            accent: state.accent,
        }
    }
}

impl Drop for AppearanceWatcher {
    fn drop(&mut self) {
        self.watch.abort();
    }
}

async fn watch(state: &Mutex<PortalAppearance>, wake: &impl Fn()) -> zbus::Result<()> {
    let connection = Connection::session().await?;
    let settings = portal::proxy(&connection, SETTINGS).await?;
    let mut changes = settings.receive_signal("SettingChanged").await?;
    for key in [COLOR_SCHEME, ACCENT_COLOR] {
        if let Ok(value) = settings
            .call::<_, _, OwnedVariant>("ReadOne", &(NAMESPACE, key))
            .await
        {
            lock(state).apply(key, value);
        }
    }
    wake();
    while let Some(change) = changes.next().await {
        let Ok((namespace, key, value)) = change
            .body()
            .deserialize::<(String, String, OwnedVariant)>()
        else {
            continue;
        };
        if namespace == NAMESPACE && matches!(key.as_str(), COLOR_SCHEME | ACCENT_COLOR) {
            lock(state).apply(&key, value);
            wake();
        }
    }
    Ok(())
}

fn lock(state: &Mutex<PortalAppearance>) -> std::sync::MutexGuard<'_, PortalAppearance> {
    state.lock().unwrap_or_else(|error| error.into_inner())
}

/// Components outside 0 to 1 mean the desktop has no accent color.
fn srgb((red, green, blue): (f64, f64, f64)) -> Option<[u8; 3]> {
    let components = [red, green, blue];
    components
        .iter()
        .all(|component| (0.0..=1.0).contains(component))
        .then(|| components.map(channel))
}
