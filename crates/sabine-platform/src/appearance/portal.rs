use std::{
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use ashpd::desktop::{
    Color,
    settings::{ColorScheme, Settings},
};
use futures_util::{
    StreamExt,
    future::{self, AbortHandle},
    stream,
};
use winit::window::{Theme, Window};

use super::{Appearance, channel};

/// Follows the appearance the XDG desktop portal reports, waking the window
/// whenever it changes.
pub struct AppearanceWatcher {
    state: Arc<Mutex<PortalAppearance>>,
    abort: AbortHandle,
    thread: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct PortalAppearance {
    scheme: ColorScheme,
    accent: Option<[u8; 3]>,
}

enum Change {
    Scheme(ColorScheme),
    Accent(Color),
}

impl AppearanceWatcher {
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let state = Arc::new(Mutex::new(PortalAppearance::default()));
        let watched = Arc::clone(&state);
        let (watch, abort) = future::abortable(async move {
            let _ = watch(&watched, &wake).await;
        });
        let thread = thread::spawn(move || {
            let _ = pollster::block_on(watch);
        });
        Self {
            state,
            abort,
            thread: Some(thread),
        }
    }

    /// The portal's appearance. Without a light or dark preference, the
    /// window's own theme decides.
    pub fn appearance(&self, window: &dyn Window) -> Appearance {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        Appearance {
            dark: match state.scheme {
                ColorScheme::PreferDark => true,
                ColorScheme::PreferLight => false,
                ColorScheme::NoPreference => window.theme() == Some(Theme::Dark),
            },
            accent: state.accent,
        }
    }
}

impl Drop for AppearanceWatcher {
    fn drop(&mut self) {
        self.abort.abort();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn watch(state: &Mutex<PortalAppearance>, wake: &impl Fn()) -> Result<(), ashpd::Error> {
    let settings = Settings::new().await?;
    let scheme = settings.color_scheme().await.unwrap_or_default();
    let accent = settings.accent_color().await.ok();
    let mut changes = stream::select(
        settings
            .receive_color_scheme_changed()
            .await?
            .map(Change::Scheme)
            .boxed(),
        settings
            .receive_accent_color_changed()
            .await?
            .map(Change::Accent)
            .boxed(),
    );
    {
        let mut current = state.lock().unwrap_or_else(|error| error.into_inner());
        current.scheme = scheme;
        current.accent = accent.and_then(srgb);
    }
    wake();
    while let Some(change) = changes.next().await {
        {
            let mut current = state.lock().unwrap_or_else(|error| error.into_inner());
            match change {
                Change::Scheme(scheme) => current.scheme = scheme,
                Change::Accent(accent) => current.accent = srgb(accent),
            }
        }
        wake();
    }
    Ok(())
}

/// Components outside 0 to 1 mean the desktop has no accent color.
fn srgb(color: Color) -> Option<[u8; 3]> {
    let components = [color.red(), color.green(), color.blue()];
    components
        .iter()
        .all(|component| (0.0..=1.0).contains(component))
        .then(|| components.map(channel))
}
