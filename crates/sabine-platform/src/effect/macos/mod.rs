mod click_through;
mod mask;

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
use click_through::ClickThrough;

/// An `NSVisualEffectView` beneath the page, masked to the blur region, and
/// click-through outside the input region.
pub(super) struct Backend {
    view: Retained<NSView>,
    material: Option<Retained<NSVisualEffectView>>,
    click_through: Option<ClickThrough>,
}

impl Backend {
    pub(super) fn new(window: &Arc<dyn Window>, options: &WindowOptions) -> Option<Self> {
        let main_thread = MainThreadMarker::new()?;
        let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        let view = unsafe { Retained::retain(handle.ns_view.cast::<NSView>().as_ptr()) }?;
        let material = options
            .wants_background_effect()
            .then(|| install_material(main_thread, &view, options.background_effect));
        Some(Self {
            view,
            material,
            click_through: None,
        })
    }

    pub(super) fn update(
        &mut self,
        options: &WindowOptions,
        width: i32,
        height: i32,
        transparent_holes: &[WindowRegionRect],
    ) {
        if let Some(material) = &self.material {
            material.setMaskImage(
                mask::material_mask(options, width, height, transparent_holes).as_deref(),
            );
        }
        match &options.regions.input {
            Some(input) => {
                if self.click_through.is_none() {
                    self.click_through = ClickThrough::new(self.view.clone());
                }
                if let Some(click_through) = &self.click_through {
                    click_through.set_region(input.resolved_rects(width, height));
                }
            }
            None => self.click_through = None,
        }
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
