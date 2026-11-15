// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Windows reports sleep, the power source and the session's lock to a
// window that registered for each, on the thread that owns the window. The
// watcher keeps a message-only window on a thread of its own, which
// receives only what it registered for and no broadcasts, so nothing
// arrives twice.

use std::cell::RefCell;
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{
    GetSystemPowerStatus, HPOWERNOTIFY, RegisterPowerSettingNotification,
    RegisterSuspendResumeNotification, SYSTEM_POWER_STATUS, UnregisterPowerSettingNotification,
    UnregisterSuspendResumeNotification,
};
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::System::SystemServices::{
    GUID_ACDC_POWER_SOURCE, GUID_BATTERY_PERCENTAGE_REMAINING,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, HWND_MESSAGE, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE,
    PostMessageW, PostQuitMessage, RegisterClassW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
    WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSW, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};
use windows::core::{PCWSTR, w};

use super::{PowerSource, Reporter};

const QUIT: u32 = WM_APP;
const ON_MAINS: u8 = 1;
const OFF_MAINS: u8 = 0;
const NO_BATTERY: u8 = 128;
const UNKNOWN_LEVEL: u8 = 255;

thread_local! {
    static REPORTER: RefCell<Option<Reporter>> = const { RefCell::new(None) };
}

pub(super) struct Watcher {
    window: Option<isize>,
    thread: Option<JoinHandle<()>>,
}

impl Watcher {
    pub(super) fn start(reporter: Reporter) -> Self {
        let (started, window) = std::sync::mpsc::channel();
        let thread = thread::Builder::new()
            .name("sabine-power".into())
            .spawn(move || {
                let window = match create_window() {
                    Ok(window) => window,
                    Err(error) => {
                        sabine_runtime::report_error(
                            "power",
                            format!("could not watch power and session changes: {error}"),
                        );
                        let _ = started.send(None);
                        return;
                    }
                };
                report_source(&reporter);
                REPORTER.with(|current| *current.borrow_mut() = Some(reporter));
                let _ = started.send(Some(window.0 as isize));
                let registrations = Registrations::register(window);
                let mut message = MSG::default();
                while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
                    unsafe { DispatchMessageW(&message) };
                }
                drop(registrations);
                unsafe {
                    let _ = DestroyWindow(window);
                }
            })
            .ok();
        Self {
            window: window.recv().ok().flatten(),
            thread,
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if let Some(window) = self.window {
            unsafe {
                let _ = PostMessageW(Some(HWND(window as *mut _)), QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What the window registered for, which it unregisters before it goes.
struct Registrations {
    window: HWND,
    notifications: Vec<HPOWERNOTIFY>,
    suspend: Option<HPOWERNOTIFY>,
    session: bool,
}

impl Registrations {
    fn register(window: HWND) -> Self {
        let recipient = HANDLE(window.0);
        unsafe {
            Self {
                window,
                notifications: [GUID_ACDC_POWER_SOURCE, GUID_BATTERY_PERCENTAGE_REMAINING]
                    .iter()
                    .filter_map(|setting| {
                        RegisterPowerSettingNotification(
                            recipient,
                            setting,
                            DEVICE_NOTIFY_WINDOW_HANDLE,
                        )
                        .ok()
                    })
                    .collect(),
                suspend: RegisterSuspendResumeNotification(recipient, DEVICE_NOTIFY_WINDOW_HANDLE)
                    .ok(),
                session: WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION).is_ok(),
            }
        }
    }
}

impl Drop for Registrations {
    fn drop(&mut self) {
        unsafe {
            for notification in self.notifications.drain(..) {
                let _ = UnregisterPowerSettingNotification(notification);
            }
            if let Some(suspend) = self.suspend.take() {
                let _ = UnregisterSuspendResumeNotification(suspend);
            }
            if self.session {
                let _ = WTSUnRegisterSessionNotification(self.window);
            }
        }
    }
}

fn report_source(reporter: &Reporter) {
    let mut status = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err()
        || !matches!(status.ACLineStatus, ON_MAINS | OFF_MAINS)
    {
        return;
    }
    let has_battery = status.BatteryFlag & NO_BATTERY == 0 && status.BatteryFlag != UNKNOWN_LEVEL;
    reporter.source(PowerSource {
        on_battery: status.ACLineStatus == OFF_MAINS,
        battery_level: (has_battery && status.BatteryLifePercent <= 100)
            .then_some(status.BatteryLifePercent),
    });
}

fn create_window() -> windows::core::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_procedure),
            hInstance: instance,
            lpszClassName: w!("SabinePowerEvents"),
            ..WNDCLASSW::default()
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("SabinePowerEvents"),
            PCWSTR::null(),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        )
    }
}

unsafe extern "system" fn window_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let report = |report: fn(&Reporter)| {
        REPORTER.with(|reporter| {
            if let Some(reporter) = reporter.borrow().as_ref() {
                report(reporter);
            }
        });
    };
    match (message, wparam.0 as u32) {
        (WM_POWERBROADCAST, PBT_APMSUSPEND) => report(Reporter::suspending),
        (WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC) => report(Reporter::resumed),
        (WM_POWERBROADCAST, PBT_POWERSETTINGCHANGE) => report(report_source),
        (WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK) => report(|reporter| reporter.locked(true)),
        (WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK) => report(|reporter| reporter.locked(false)),
        (QUIT, _) => unsafe { PostQuitMessage(0) },
        _ => return unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
    LRESULT(1)
}
