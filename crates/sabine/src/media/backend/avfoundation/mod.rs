//! AVFoundation playback in layers stacked beneath the window's Metal layer.

mod observer;
mod playback;
mod selection;

use std::rc::Rc;

use objc2::{ClassType, MainThreadMarker, rc::Retained};
use objc2_app_kit::NSView;
use objc2_av_foundation::{AVLayerVideoGravityResizeAspect, AVPlayerLayer};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGColor;
use objc2_foundation::NSObjectProtocol;
use objc2_quartz_core::{CALayer, CAMetalLayer, CATransaction};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use super::{Backend, Player};
use crate::media::command::{Command, Events, PlayerOptions};
use crate::media::geometry::{Layout, Rect};
use playback::Playback;

/// Holds the players' layers beneath the page, in top-left window
/// coordinates whatever the view's own orientation.
#[derive(Default)]
pub(in crate::media) struct AvFoundation {
    stage: Option<Retained<CALayer>>,
}

pub(in crate::media) struct AvPlayer {
    playback: Rc<Playback>,
    clip: Retained<CALayer>,
    video: Retained<AVPlayerLayer>,
}

impl Backend for AvFoundation {
    type Player = AvPlayer;

    fn attach(&mut self, window: &dyn Window) -> Result<(), String> {
        let Ok(RawWindowHandle::AppKit(handle)) =
            window.window_handle().map(|handle| handle.as_raw())
        else {
            return Err("native media needs an AppKit window".to_string());
        };
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let root = view.layer().ok_or("the window has no layer")?;
        let stage = CALayer::new();
        transaction(|| {
            stage.setGeometryFlipped(!root.contentsAreFlipped());
            stage.setFrame(root.bounds());
            match metal_layer(&root) {
                Some(metal) => root.insertSublayer_below(&stage, Some(&metal)),
                None => root.addSublayer(&stage),
            }
        });
        self.stage = Some(stage);
        Ok(())
    }

    fn detach(&mut self) {
        if let Some(stage) = self.stage.take() {
            transaction(|| stage.removeFromSuperlayer());
        }
    }

    fn spawn(&mut self, options: PlayerOptions, events: Events) -> Result<AvPlayer, String> {
        let main_thread = MainThreadMarker::new().expect("windows run on the main thread");
        let playback = Playback::new(options, events, main_thread)?;
        let clip = CALayer::new();
        let video = unsafe { AVPlayerLayer::playerLayerWithPlayer(Some(&playback.player)) };
        transaction(|| {
            clip.setMasksToBounds(true);
            clip.setHidden(true);
            video.setMasksToBounds(true);
            video.setBackgroundColor(Some(&CGColor::new_generic_gray(0.0, 1.0)));
            if let Some(gravity) = unsafe { AVLayerVideoGravityResizeAspect } {
                unsafe { video.setVideoGravity(gravity) };
            }
            clip.addSublayer(&video);
        });
        Ok(AvPlayer {
            playback,
            clip,
            video,
        })
    }

    fn mount(&mut self, player: &mut AvPlayer) -> Result<(), String> {
        let stage = self.stage.as_ref().ok_or("the window is gone")?;
        transaction(|| stage.addSublayer(&player.clip));
        Ok(())
    }

    fn stop(&mut self, player: AvPlayer) {
        player.unmount_layers();
    }

    fn flush(&mut self) {
        let Some(stage) = &self.stage else {
            return;
        };
        if let Some(root) = stage.superlayer() {
            let bounds = root.bounds();
            if stage.frame() != bounds {
                transaction(|| stage.setFrame(bounds));
            }
        }
    }
}

impl AvPlayer {
    fn unmount_layers(&self) {
        transaction(|| self.clip.removeFromSuperlayer());
    }
}

impl Player for AvPlayer {
    fn send(&mut self, command: Command) {
        self.playback.apply(command);
    }

    fn place(&mut self, layout: Option<Layout>) {
        transaction(|| {
            let Some(layout) = layout else {
                self.clip.setHidden(true);
                return;
            };
            let visible = layout.hole.bounds;
            let bounds = layout.bounds;
            self.clip.setFrame(cg_rect(visible));
            self.video.setFrame(cg_rect(Rect::new(
                bounds.x - visible.x,
                bounds.y - visible.y,
                bounds.width,
                bounds.height,
            )));
            self.video.setCornerRadius(layout.radius);
            self.clip.setHidden(false);
        });
    }

    fn unmount(&mut self) {
        self.unmount_layers();
    }
}

/// Applies layer changes at once, without Core Animation's implicit animations.
fn transaction(change: impl FnOnce()) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    change();
    CATransaction::commit();
}

/// The layer wgpu renders the window into, which the stage sits beneath.
fn metal_layer(root: &CALayer) -> Option<Retained<CALayer>> {
    unsafe { root.sublayers() }?
        .iter()
        .filter(|layer| layer.isKindOfClass(CAMetalLayer::class()))
        .last()
}

fn cg_rect(rect: Rect) -> CGRect {
    CGRect::new(
        CGPoint::new(rect.x, rect.y),
        CGSize::new(rect.width, rect.height),
    )
}
