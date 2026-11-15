//! Windows and macOS tell how long ago the last input was, but not when the
//! person comes back, so a watch sleeps until the threshold could have
//! passed and, while the person is away, looks again every second.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use super::{IdleEvents, IdleState};

const AWAY_CHECK: Duration = Duration::from_secs(1);

pub(super) struct Watch {
    _stop: mpsc::Sender<()>,
}

impl Watch {
    pub(super) async fn start(threshold: Duration, events: IdleEvents) -> Result<Self, String> {
        let (stop, stopped) = mpsc::channel::<()>();
        std::thread::Builder::new()
            .name("sabine-idle".into())
            .spawn(move || {
                let mut idle = false;
                loop {
                    let since_input = since_last_input();
                    if idle != (since_input >= threshold) {
                        idle = !idle;
                        events(if idle {
                            IdleState::Idle
                        } else {
                            IdleState::Active
                        });
                    }
                    let wait = if idle {
                        AWAY_CHECK
                    } else {
                        threshold - since_input
                    };
                    if stopped.recv_timeout(wait) != Err(RecvTimeoutError::Timeout) {
                        return;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { _stop: stop })
    }
}

#[cfg(target_os = "windows")]
fn since_last_input() -> Duration {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut input = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if !unsafe { GetLastInputInfo(&mut input) }.as_bool() {
        return Duration::ZERO;
    }
    Duration::from_millis(u64::from(
        unsafe { GetTickCount() }.wrapping_sub(input.dwTime),
    ))
}

#[cfg(target_os = "macos")]
fn since_last_input() -> Duration {
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGEventType};

    const ANY_INPUT: CGEventType = CGEventType(u32::MAX);
    Duration::from_secs_f64(
        CGEventSource::seconds_since_last_event_type(
            CGEventSourceStateID::CombinedSessionState,
            ANY_INPUT,
        )
        .max(0.0),
    )
}
