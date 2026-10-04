use std::sync::Arc;

use objc2::rc::Retained;
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
};
use objc2_foundation::MainThreadMarker;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::{WindowBackgroundEffect, WindowOptions, WindowRegionRect};

pub(super) struct Backend {
    material: Option<Retained<NSVisualEffectView>>,
}

impl Backend {
    pub(super) fn new(window: &Arc<dyn Window>, options: &WindowOptions) -> Option<Self> {
        let main_thread = MainThreadMarker::new()?;
        let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        let content_view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let material = options
            .wants_background_effect()
            .then(|| install_material(main_thread, content_view, options.background_effect));
        Some(Self { material })
    }

    pub(super) fn update(
        &mut self,
        _options: &WindowOptions,
        _width: i32,
        _height: i32,
        _transparent_holes: &[WindowRegionRect],
    ) {
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        if let Some(material) = &self.material {
            material.removeFromSuperview();
        }
    }
}

fn install_material(
    main_thread: MainThreadMarker,
    content_view: &NSView,
    effect: WindowBackgroundEffect,
) -> Retained<NSVisualEffectView> {
    let material = match effect {
        WindowBackgroundEffect::HudWindow => NSVisualEffectMaterial::HUDWindow,
        WindowBackgroundEffect::Sidebar => NSVisualEffectMaterial::Sidebar,
        _ => NSVisualEffectMaterial::UnderWindowBackground,
    };
    let effect_view = NSVisualEffectView::initWithFrame(main_thread.alloc(), content_view.bounds());
    effect_view.setMaterial(material);
    effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect_view.setState(NSVisualEffectState::FollowsWindowActiveState);
    effect_view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    content_view.addSubview_positioned_relativeTo(&effect_view, NSWindowOrderingMode::Below, None);
    effect_view
}
