use sabine_runtime::ProgressLog;
use std::io::Write;

const PROGRESS_LINES_ARG: &str = "--sabine-install-progress-lines";
#[cfg(windows)]
const PROGRESS_BAR_ARG: &str = "--sabine-install-progress-bar";
const PROGRESS_LINE_PREFIX: &str = "sabine-progress ";

pub(super) struct SetupProgress {
    log: ProgressLog,
    lines: bool,
    #[cfg(windows)]
    bar: Option<bar::ProgressBar>,
    permille: Option<u32>,
}

impl SetupProgress {
    pub(super) fn from_args(args: &[String]) -> Self {
        Self {
            log: ProgressLog::default(),
            lines: args.iter().any(|arg| arg == PROGRESS_LINES_ARG),
            #[cfg(windows)]
            bar: super::option(args, PROGRESS_BAR_ARG).and_then(bar::ProgressBar::attach),
            permille: None,
        }
    }

    pub(super) fn report(&mut self, message: &str, fraction: Option<f32>) {
        let mut stdout = std::io::stdout().lock();
        if let Some(fraction) = fraction {
            let permille = (fraction.clamp(0.0, 1.0) * 1000.0).round() as u32;
            if self.permille != Some(permille) {
                self.permille = Some(permille);
                if self.lines {
                    let _ = writeln!(stdout, "{PROGRESS_LINE_PREFIX}{permille}");
                }
                #[cfg(windows)]
                if let Some(bar) = &self.bar {
                    bar.show(permille);
                }
            }
        }
        if let Some(line) = self.log.line(message, fraction) {
            let _ = writeln!(stdout, "{line}");
        }
        let _ = stdout.flush();
    }
}

#[cfg(windows)]
mod bar {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::Controls::{PBM_GETPOS, PBM_GETRANGE, PBM_SETPOS};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SendMessageW};

    pub(super) struct ProgressBar {
        window: HWND,
        start: i32,
        span: i32,
    }

    impl ProgressBar {
        pub(super) fn attach(handle: &str) -> Option<Self> {
            let window = HWND(handle.parse::<isize>().ok().filter(|handle| *handle != 0)? as _);
            let (start, end) = unsafe {
                (
                    SendMessageW(window, PBM_GETPOS, None, None).0 as i32,
                    SendMessageW(window, PBM_GETRANGE, Some(WPARAM(0)), None).0 as i32,
                )
            };
            Some(Self {
                window,
                start,
                span: (end - start).max(0),
            })
        }

        pub(super) fn show(&self, permille: u32) {
            let position = self.start + self.span * permille as i32 / 1000;
            let _ = unsafe {
                PostMessageW(
                    Some(self.window),
                    PBM_SETPOS,
                    WPARAM(position as usize),
                    LPARAM(0),
                )
            };
        }
    }
}
