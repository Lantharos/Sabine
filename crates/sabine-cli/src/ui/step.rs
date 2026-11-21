use std::{
    sync::{
        Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use sabine_runtime::ProgressLog;

const TICK: Duration = Duration::from_millis(80);
const SPINNER: [&str; 11] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", " "];
const BAR: &str = "━━─";

const WORKING: u8 = 0;
const COUNTING: u8 = 1;
const BYTES: u8 = 2;
const PERCENT: u8 = 3;

fn drawn(shape: u8) -> ProgressStyle {
    let template = match shape {
        COUNTING => {
            "  {spinner:.cyan} {prefix:.bold}  {bar:30.cyan/dim} {pos}/{len}  {elapsed:.dim}  {wide_msg}"
        }
        BYTES => {
            "  {spinner:.cyan} {prefix:.bold}  {bar:30.cyan/dim} {bytes}/{total_bytes}  {bytes_per_sec:.dim}  {eta:.dim}"
        }
        PERCENT => {
            "  {spinner:.cyan} {prefix:.bold}  {bar:30.cyan/dim} {percent:>3}%  {elapsed:.dim}  {wide_msg}"
        }
        _ => "  {spinner:.cyan} {prefix:.bold}  {elapsed:.dim}  {wide_msg}",
    };
    ProgressStyle::with_template(template)
        .expect("the templates are valid")
        .tick_strings(&SPINNER)
        .progress_chars(BAR)
}

/// One stage of a longer job: a spinner while it runs, a bar when there's
/// something to count, and a line saying how long it took once it's done.
pub(crate) struct Step {
    title: String,
    started: Instant,
    bar: Option<ProgressBar>,
    shape: AtomicU8,
    log: Mutex<ProgressLog>,
    finished: bool,
}

impl Step {
    pub(crate) fn start(title: impl Into<String>) -> Self {
        let title = title.into();
        let bar = super::live().then(|| {
            let bar = ProgressBar::new_spinner()
                .with_style(drawn(WORKING))
                .with_prefix(title.clone());
            bar.enable_steady_tick(TICK);
            bar
        });
        if bar.is_none() {
            eprintln!("{title}…");
        }
        Self {
            title,
            started: Instant::now(),
            bar,
            shape: AtomicU8::new(WORKING),
            log: Mutex::default(),
            finished: false,
        }
    }

    /// What the step is busy with right now, such as the crate being compiled.
    pub(crate) fn note(&self, text: &str) {
        if let Some(bar) = &self.bar {
            bar.set_message(style(text).for_stderr().dim().to_string());
        }
    }

    pub(crate) fn count(&self, done: u64, total: u64) {
        self.measure(COUNTING, done, total);
    }

    pub(crate) fn bytes(&self, done: u64, total: u64) {
        self.measure(BYTES, done, total.max(done));
    }

    /// Progress the Sabine service reports, such as a runtime download: a bar
    /// while it's drawn, and otherwise a line now and then.
    pub(crate) fn progress(&self, message: &str, fraction: Option<f32>) {
        if self.bar.is_none() {
            let line = self
                .log
                .lock()
                .expect("no writer panics while holding the log")
                .line(message, fraction);
            if let Some(line) = line {
                eprintln!("  {line}");
            }
            return;
        }
        self.note(message);
        if let Some(fraction) = fraction {
            self.measure(PERCENT, (fraction.clamp(0.0, 1.0) * 1000.0) as u64, 1000);
        }
    }

    fn measure(&self, shape: u8, done: u64, total: u64) {
        let Some(bar) = &self.bar else {
            return;
        };
        if self.shape.swap(shape, Ordering::Relaxed) != shape {
            bar.set_style(drawn(shape));
        }
        bar.set_length(total);
        bar.set_position(done);
    }

    /// Hides the step while `print` writes to the terminal, so lines it
    /// prints don't tear the drawn progress.
    pub(crate) fn above(&self, print: impl FnOnce()) {
        match &self.bar {
            Some(bar) => bar.suspend(print),
            None => print(),
        }
    }

    pub(crate) fn done(mut self) {
        self.finish(true, None);
    }

    /// Ends the step with a short result beside it, such as a package's size.
    pub(crate) fn done_with(mut self, note: &str) {
        self.finish(true, Some(note));
    }

    fn finish(&mut self, succeeded: bool, note: Option<&str>) {
        self.finished = true;
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
        let mark = if succeeded {
            style("✓").for_stderr().green()
        } else {
            style("✗").for_stderr().red()
        };
        let note = note.map_or(String::new(), |note| format!("  {note}"));
        eprintln!(
            "  {mark} {}{}  {}",
            self.title,
            style(note).for_stderr().dim(),
            style(super::duration(self.started.elapsed()))
                .for_stderr()
                .dim()
        );
    }
}

impl Drop for Step {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(false, None);
        }
    }
}
