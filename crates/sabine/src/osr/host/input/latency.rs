use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::osr::host::config::OsrHostConfig;
use crate::osr::host::native::OsrNativeHost;

const REPORT_EVERY: Duration = Duration::from_secs(2);
const FRAMELESS_AFTER: Duration = Duration::from_millis(250);
const KINDS: [InputKind; 3] = [InputKind::Pointer, InputKind::Touch, InputKind::Pen];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::osr::host) enum InputKind {
    Pointer,
    Touch,
    Pen,
}

impl InputKind {
    fn name(self) -> &'static str {
        match self {
            Self::Pointer => "pointer",
            Self::Touch => "touch",
            Self::Pen => "pen",
        }
    }
}

/// How far a frame got when its time was taken.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::osr::host) enum FrameEnd {
    /// The compositor's presentation time, when the frame started to show.
    Screen,
    /// The moment Sabine handed the frame to the compositor.
    Compositor,
}

impl FrameEnd {
    fn describe(self) -> &'static str {
        match self {
            Self::Screen => "on screen",
            Self::Compositor => "handed to the compositor",
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::osr::host) struct InputStamp {
    kind: InputKind,
    at: Instant,
}

/// Times pointer, touch and pen moves from the event loop to the first page
/// frame that arrived after them, while `SABINE_TRACE` is set.
#[derive(Clone)]
pub(in crate::osr::host) struct InputLatency(Arc<Mutex<Tracker>>);

struct Tracker {
    label: String,
    waiting: Vec<InputStamp>,
    painted: Vec<InputStamp>,
    since: Option<Instant>,
    samples: Vec<(InputKind, FrameEnd, Duration)>,
    frameless: [usize; KINDS.len()],
}

impl InputLatency {
    pub(in crate::osr::host) fn traced(config: &OsrHostConfig) -> Option<Self> {
        crate::launch::metrics::trace_enabled().then(|| {
            Self(Arc::new(Mutex::new(Tracker {
                label: config
                    .app_id
                    .as_deref()
                    .unwrap_or(&*config.title)
                    .to_string(),
                waiting: Vec::new(),
                painted: Vec::new(),
                since: None,
                samples: Vec::new(),
                frameless: [0; KINDS.len()],
            })))
        })
    }

    pub(in crate::osr::host) fn note_input(&self, kind: InputKind) {
        let at = Instant::now();
        if let Ok(mut tracker) = self.0.lock() {
            tracker.drop_frameless(at);
            tracker.waiting.push(InputStamp { kind, at });
        }
    }

    /// Hands the moves made so far to the page frame that just arrived.
    pub(in crate::osr::host) fn note_page_frame(&self) {
        if let Ok(mut tracker) = self.0.lock() {
            tracker.drop_frameless(Instant::now());
            let waiting = std::mem::take(&mut tracker.waiting);
            tracker.painted.extend(waiting);
        }
    }

    /// The moves the newest page frame carries, for the compositor to time.
    pub(in crate::osr::host) fn take_painted(&self) -> Vec<InputStamp> {
        self.0
            .lock()
            .map(|mut tracker| std::mem::take(&mut tracker.painted))
            .unwrap_or_default()
    }

    pub(in crate::osr::host) fn record(
        &self,
        inputs: &[InputStamp],
        shown: Instant,
        end: FrameEnd,
    ) {
        if inputs.is_empty() {
            return;
        }
        let Ok(mut tracker) = self.0.lock() else {
            return;
        };
        let since = *tracker.since.get_or_insert(shown);
        tracker.samples.extend(
            inputs
                .iter()
                .map(|input| (input.kind, end, shown.saturating_duration_since(input.at))),
        );
        if shown.saturating_duration_since(since) >= REPORT_EVERY {
            tracker.report();
        }
    }
}

impl Tracker {
    fn drop_frameless(&mut self, now: Instant) {
        let stale = self
            .waiting
            .iter()
            .take_while(|input| now.saturating_duration_since(input.at) > FRAMELESS_AFTER)
            .count();
        for input in self.waiting.drain(..stale) {
            self.frameless[input.kind as usize] += 1;
        }
    }

    fn report(&mut self) {
        for kind in KINDS {
            for end in [FrameEnd::Screen, FrameEnd::Compositor] {
                let mut times = self
                    .samples
                    .iter()
                    .filter(|sample| sample.0 == kind && sample.1 == end)
                    .map(|sample| sample.2)
                    .collect::<Vec<_>>();
                if times.is_empty() {
                    continue;
                }
                times.sort_unstable();
                let frameless = std::mem::take(&mut self.frameless[kind as usize]);
                eprintln!(
                    "sabine trace [{}] osr-host input {} {} moves {}: p50 {} p95 {} max {}{}",
                    self.label,
                    kind.name(),
                    times.len(),
                    end.describe(),
                    millis(percentile(&times, 50)),
                    millis(percentile(&times, 95)),
                    millis(times[times.len() - 1]),
                    if frameless == 0 {
                        String::new()
                    } else {
                        format!(", {frameless} more painted nothing")
                    },
                );
            }
        }
        self.samples.clear();
        self.since = None;
    }
}

fn percentile(sorted: &[Duration], percent: usize) -> Duration {
    sorted[(sorted.len() * percent).div_ceil(100).saturating_sub(1)]
}

fn millis(duration: Duration) -> String {
    format!("{:.1}ms", duration.as_secs_f64() * 1000.0)
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn note_input(&self, kind: InputKind) {
        if let Some(latency) = &self.input_latency {
            latency.note_input(kind);
        }
    }

    pub(in crate::osr::host) fn note_page_frame(&self) {
        if let Some(latency) = &self.input_latency {
            latency.note_page_frame();
        }
    }

    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn painted_inputs(&self) -> Vec<InputStamp> {
        self.input_latency
            .as_ref()
            .map(InputLatency::take_painted)
            .unwrap_or_default()
    }

    /// Times the moves the window's own frame carried to the compositor.
    pub(in crate::osr::host) fn note_window_frame(&self) {
        if let Some(latency) = &self.input_latency {
            latency.record(
                &latency.take_painted(),
                Instant::now(),
                FrameEnd::Compositor,
            );
        }
    }
}
