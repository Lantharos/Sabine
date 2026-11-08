use std::time::{Duration, Instant};

use wayland_client::{
    Connection, Dispatch, QueueHandle, globals::GlobalList, protocol::wl_surface::WlSurface,
};
use wayland_protocols::wp::presentation_time::client::{
    wp_presentation::{self, WpPresentation},
    wp_presentation_feedback::{self, WpPresentationFeedback},
};

use super::worker::State;
use crate::osr::host::input::{FrameEnd, InputLatency, InputStamp};

/// Times the moves each page frame carries until the compositor shows it,
/// or until it takes the frame when it can't tell when it shows one.
pub(super) struct FrameTiming {
    latency: InputLatency,
    presentation: Option<WpPresentation>,
    clock: Option<libc::clockid_t>,
    carried: Vec<InputStamp>,
}

impl FrameTiming {
    pub(super) fn new(
        latency: InputLatency,
        globals: &GlobalList,
        handle: &QueueHandle<State>,
    ) -> Self {
        Self {
            latency,
            presentation: globals.bind(handle, 1..=1, ()).ok(),
            clock: None,
            carried: Vec::new(),
        }
    }

    /// Hands the moves of a frame the compositor never got to the next one.
    pub(super) fn carry(&mut self, inputs: Vec<InputStamp>) {
        self.carried.extend(inputs);
    }

    /// Times `inputs` against the frame `surface` shows from its next commit.
    pub(super) fn track(
        &mut self,
        surface: &WlSurface,
        handle: &QueueHandle<State>,
        mut inputs: Vec<InputStamp>,
    ) {
        inputs.append(&mut self.carried);
        if inputs.is_empty() {
            return;
        }
        match (&self.presentation, self.clock) {
            (Some(presentation), Some(_)) => {
                presentation.feedback(surface, handle, inputs);
            }
            _ => self
                .latency
                .record(&inputs, Instant::now(), FrameEnd::Compositor),
        }
    }

    fn presented(&self, inputs: &[InputStamp], seconds: u64, nanos: u32) {
        let Some(clock) = self.clock else {
            return;
        };
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let observed = Instant::now();
        if unsafe { libc::clock_gettime(clock, &mut now) } != 0 {
            return;
        }
        let now = Duration::new(now.tv_sec as u64, now.tv_nsec as u32);
        let shown = Duration::new(seconds, nanos);
        let shown = match now.checked_sub(shown) {
            Some(age) => observed.checked_sub(age).unwrap_or(observed),
            None => observed + (shown - now),
        };
        self.latency.record(inputs, shown, FrameEnd::Screen);
    }
}

impl Drop for FrameTiming {
    fn drop(&mut self) {
        if let Some(presentation) = self.presentation.take() {
            presentation.destroy();
        }
    }
}

impl Dispatch<WpPresentation, ()> for State {
    fn event(
        state: &mut Self,
        _: &WpPresentation,
        event: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event
            && let Some(timing) = state.timing.as_mut()
        {
            timing.clock = Some(clk_id as libc::clockid_t);
        }
    }
}

impl Dispatch<WpPresentationFeedback, Vec<InputStamp>> for State {
    fn event(
        state: &mut Self,
        _: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        inputs: &Vec<InputStamp>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(timing) = state.timing.as_mut() else {
            return;
        };
        match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                ..
            } => timing.presented(
                inputs,
                u64::from(tv_sec_hi) << 32 | u64::from(tv_sec_lo),
                tv_nsec,
            ),
            wp_presentation_feedback::Event::Discarded => timing.carry(inputs.clone()),
            _ => {}
        }
    }
}
