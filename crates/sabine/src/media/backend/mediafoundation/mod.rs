// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Media visuals belong to the window's composition device and cannot move to
// another one, so every mount builds fresh visuals. Positions are physical
// pixels, matching the page swapchain above them.

mod engine;
mod session;
mod streams;
mod worker;

use std::{sync::Arc, thread::JoinHandle};

use windows::Win32::Graphics::DirectComposition::{
    IDCompositionRectangleClip, IDCompositionVisual,
};
use winit::window::Window;

use super::{Backend, Player};
use crate::media::command::{Command, Events, PlayerOptions};
use crate::media::geometry::Layout;
use crate::render::Composition;
use worker::{Order, Surface, Worker};

#[derive(Default)]
pub(in crate::media) struct MediaFoundation {
    composition: Option<Arc<Composition>>,
    stopping: Vec<JoinHandle<()>>,
}

pub(in crate::media) struct MfPlayer {
    worker: Worker,
    placement: Option<Placement>,
}

/// The visuals the window thread positions: a clip for the visible part of
/// the element, holding the video visual with its rounded corners.
struct Placement {
    composition: Arc<Composition>,
    clip: IDCompositionVisual,
    clip_shape: IDCompositionRectangleClip,
    video: IDCompositionVisual,
    video_shape: IDCompositionRectangleClip,
}

impl Backend for MediaFoundation {
    type Player = MfPlayer;

    fn attach(&mut self, window: &dyn Window) -> Result<(), String> {
        self.composition = Some(Composition::for_window(window)?);
        Ok(())
    }

    fn detach(&mut self) {
        self.composition = None;
    }

    fn spawn(&mut self, options: PlayerOptions, events: Events) -> Result<MfPlayer, String> {
        Ok(MfPlayer {
            worker: Worker::spawn(options, events),
            placement: None,
        })
    }

    fn mount(&mut self, player: &mut MfPlayer) -> Result<(), String> {
        let composition = self.composition.as_ref().ok_or("the window is gone")?;
        let placement = unsafe { Placement::new(composition) }.map_err(|error| error.message())?;
        player.worker.send(Order::Surface(Some(Surface {
            composition: Arc::clone(composition),
            visual: placement.video.clone(),
        })));
        player.placement = Some(placement);
        Ok(())
    }

    fn stop(&mut self, player: MfPlayer) {
        self.stopping.retain(|thread| !thread.is_finished());
        self.stopping.push(player.worker.stop());
    }

    fn flush(&mut self) {
        if let Some(composition) = &self.composition {
            composition.commit();
        }
    }
}

impl Drop for MediaFoundation {
    /// Waits for every engine to shut down before the window goes away.
    fn drop(&mut self) {
        for thread in self.stopping.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Player for MfPlayer {
    fn send(&mut self, command: Command) {
        self.worker.send(Order::Page(command));
    }

    fn place(&mut self, layout: Option<Layout>) {
        if let Some(placement) = &self.placement {
            let placed = unsafe { placement.place(layout) };
            if let Err(error) = placed {
                eprintln!("Sabine media: could not place video: {error}");
            }
        }
        self.worker
            .send(Order::Frame(layout.map(|layout| layout.frame)));
    }

    fn unmount(&mut self) {
        self.placement = None;
        self.worker.send(Order::Surface(None));
    }
}

impl Placement {
    unsafe fn new(composition: &Arc<Composition>) -> windows::core::Result<Self> {
        unsafe {
            let device = composition.device();
            let clip = device.CreateVisual()?;
            let clip_shape = device.CreateRectangleClip()?;
            let video = device.CreateVisual()?;
            let video_shape = device.CreateRectangleClip()?;
            clip.SetClip(&clip_shape)?;
            video.SetClip(&video_shape)?;
            clip.AddVisual(&video, false, None)?;
            let placement = Self {
                composition: Arc::clone(composition),
                clip,
                clip_shape,
                video,
                video_shape,
            };
            placement.place(None)?;
            composition.media().AddVisual(&placement.clip, true, None)?;
            Ok(placement)
        }
    }

    /// Moves the visuals; hidden visuals keep an empty clip.
    unsafe fn place(&self, layout: Option<Layout>) -> windows::core::Result<()> {
        let (origin, size, video, radius) = match layout {
            Some(layout) => {
                let frame = layout.frame;
                (
                    (
                        (f64::from(layout.hole.surface.x) * layout.scale) as f32,
                        (f64::from(layout.hole.surface.y) * layout.scale) as f32,
                    ),
                    (frame.buffer.0 as f32, frame.buffer.1 as f32),
                    frame.video,
                    frame.radius,
                )
            }
            None => ((0.0, 0.0), (0.0, 0.0), [0.0; 4], 0.0),
        };
        unsafe {
            self.clip.SetOffsetX2(origin.0.round())?;
            self.clip.SetOffsetY2(origin.1.round())?;
            set_rect(&self.clip_shape, size.0, size.1, 0.0)?;
            self.video.SetOffsetX2(video[0].round())?;
            self.video.SetOffsetY2(video[1].round())?;
            set_rect(
                &self.video_shape,
                video[2].round(),
                video[3].round(),
                radius,
            )
        }
    }
}

impl Drop for Placement {
    fn drop(&mut self) {
        let _ = unsafe { self.composition.media().RemoveVisual(&self.clip) };
    }
}

unsafe fn set_rect(
    shape: &IDCompositionRectangleClip,
    width: f32,
    height: f32,
    radius: f32,
) -> windows::core::Result<()> {
    unsafe {
        shape.SetLeft2(0.0)?;
        shape.SetTop2(0.0)?;
        shape.SetRight2(width)?;
        shape.SetBottom2(height)?;
        for set in [
            IDCompositionRectangleClip::SetTopLeftRadiusX2,
            IDCompositionRectangleClip::SetTopLeftRadiusY2,
            IDCompositionRectangleClip::SetTopRightRadiusX2,
            IDCompositionRectangleClip::SetTopRightRadiusY2,
            IDCompositionRectangleClip::SetBottomLeftRadiusX2,
            IDCompositionRectangleClip::SetBottomLeftRadiusY2,
            IDCompositionRectangleClip::SetBottomRightRadiusX2,
            IDCompositionRectangleClip::SetBottomRightRadiusY2,
        ] {
            set(shape, radius)?;
        }
        Ok(())
    }
}
