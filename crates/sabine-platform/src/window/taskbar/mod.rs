// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The taskbar draws a window's badge and progress on its button, which
// Explorer creates again whenever the window is shown after being hidden or
// Explorer restarts, forgetting both. It says so with the registered
// `TaskbarButtonCreated` message, which a subclass of the window hears, and
// the taskbar thread shows both again. ITaskbarList3 calls go to Explorer,
// so they run on that thread instead of the window's, which never waits on
// the shell.

mod badge;

use std::sync::mpsc;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Shell::{
    DefSubclassProc, ITaskbarList3, RemoveWindowSubclass, SetWindowSubclass, TBPF_ERROR,
    TBPF_INDETERMINATE, TBPF_NOPROGRESS, TBPF_NORMAL, TBPF_PAUSED, TaskbarList,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON, RegisterWindowMessageW};
use windows::core::{HSTRING, w};
use winit::window::Window;

use crate::{AppBadge, AppProgress};

const SUBCLASS_ID: usize = 0x5ab1_7a5b;
const PROGRESS_TOTAL: u64 = 10_000;

enum Command {
    Badge(Option<String>),
    Progress(AppProgress),
    ButtonCreated,
}

/// The badge and progress bar on one window's taskbar button.
pub struct Taskbar {
    window: HWND,
    subclass: Box<Subclass>,
}

struct Subclass {
    button_created: u32,
    commands: mpsc::Sender<Command>,
}

impl Taskbar {
    pub fn new(window: &dyn Window) -> Result<Self, String> {
        let RawWindowHandle::Win32(handle) = window
            .window_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("the window has no Win32 handle".to_string());
        };
        let window = HWND(handle.hwnd.get() as *mut std::ffi::c_void);
        let (commands, received) = mpsc::channel();
        let hwnd = window.0 as isize;
        std::thread::Builder::new()
            .name("sabine-taskbar".to_string())
            .spawn(move || run(HWND(hwnd as *mut std::ffi::c_void), received))
            .map_err(|error| error.to_string())?;
        let subclass = Box::new(Subclass {
            button_created: unsafe { RegisterWindowMessageW(w!("TaskbarButtonCreated")) },
            commands,
        });
        let subclassed = unsafe {
            SetWindowSubclass(
                window,
                Some(listen),
                SUBCLASS_ID,
                &raw const *subclass as usize,
            )
        };
        if !subclassed.as_bool() {
            return Err("could not follow the window's taskbar button".to_string());
        }
        Ok(Self { window, subclass })
    }

    pub fn set_badge(&self, badge: Option<&AppBadge>) {
        let label = badge.and_then(AppBadge::label);
        let _ = self.subclass.commands.send(Command::Badge(label));
    }

    pub fn set_progress(&self, progress: AppProgress) {
        let _ = self.subclass.commands.send(Command::Progress(progress));
    }
}

impl Drop for Taskbar {
    fn drop(&mut self) {
        unsafe {
            let _ = RemoveWindowSubclass(self.window, Some(listen), SUBCLASS_ID);
        }
    }
}

unsafe extern "system" fn listen(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    subclass: usize,
) -> LRESULT {
    let subclass = unsafe { &*(subclass as *const Subclass) };
    if message == subclass.button_created {
        let _ = subclass.commands.send(Command::ButtonCreated);
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

/// What the button shows, kept to show it again on a new button.
struct Shown {
    window: HWND,
    list: ITaskbarList3,
    badge: Option<String>,
    icon: Option<HICON>,
    progress: AppProgress,
}

fn run(window: HWND, commands: mpsc::Receiver<Command>) {
    let list = unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .and_then(|()| {
                CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER)
            })
            .and_then(|list| list.HrInit().map(|()| list))
    };
    let list = match list {
        Ok(list) => list,
        Err(error) => {
            sabine_runtime::report_error("window", format!("the taskbar is unavailable: {error}"));
            return;
        }
    };
    let mut shown = Shown {
        window,
        list,
        badge: None,
        icon: None,
        progress: AppProgress::Hidden,
    };
    for command in commands {
        match command {
            Command::Badge(label) if label != shown.badge => {
                shown.badge = label;
                shown.draw_badge();
                shown.show_badge();
            }
            Command::Badge(_) => {}
            Command::Progress(progress) => {
                shown.progress = progress;
                shown.show_progress();
            }
            Command::ButtonCreated => {
                shown.show_badge();
                shown.show_progress();
            }
        }
    }
    shown.forget_icon();
}

impl Shown {
    fn draw_badge(&mut self) {
        self.forget_icon();
        self.icon = self
            .badge
            .as_deref()
            .and_then(|label| badge::icon(self.window, label));
    }

    /// Calls fail while the button doesn't exist yet; it shows the badge
    /// once it does.
    fn show_badge(&self) {
        let description = HSTRING::from(self.badge.as_deref().unwrap_or_default());
        unsafe {
            let _ =
                self.list
                    .SetOverlayIcon(self.window, self.icon.unwrap_or_default(), &description);
        }
    }

    fn show_progress(&self) {
        let state = match self.progress {
            AppProgress::Hidden => TBPF_NOPROGRESS,
            AppProgress::Indeterminate => TBPF_INDETERMINATE,
            AppProgress::Normal(_) => TBPF_NORMAL,
            AppProgress::Paused(_) => TBPF_PAUSED,
            AppProgress::Error(_) => TBPF_ERROR,
        };
        unsafe {
            if let Some(value) = self.progress.value() {
                let _ = self.list.SetProgressValue(
                    self.window,
                    (value * PROGRESS_TOTAL as f64).round() as u64,
                    PROGRESS_TOTAL,
                );
            }
            let _ = self.list.SetProgressState(self.window, state);
        }
    }

    fn forget_icon(&mut self) {
        if let Some(icon) = self.icon.take() {
            unsafe {
                let _ = DestroyIcon(icon);
            }
        }
    }
}
