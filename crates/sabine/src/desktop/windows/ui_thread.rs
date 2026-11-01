// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Work reaches the thread as messages to a message-only window. Modal loops,
// such as the one running while the tray menu is open, dispatch window
// messages but drop thread messages, so posting to the thread would stall
// every task until the menu closes.

use std::{
    cell::RefCell,
    thread::{self, JoinHandle},
};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
            HWND_MESSAGE, MSG, PostMessageW, PostQuitMessage, RegisterClassW, TranslateMessage,
            WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WNDCLASSW,
        },
    },
    core::{PCWSTR, w},
};

type Task = Box<dyn FnOnce() + Send>;

const RUN_TASKS: u32 = WM_APP;
const QUIT: u32 = WM_APP + 1;

thread_local! {
    static TASKS: RefCell<Option<crossbeam_channel::Receiver<Task>>> = const { RefCell::new(None) };
}

/// A thread with its own message loop for the tray icon and hotkeys, whose
/// hidden windows need one. It runs beside whatever loop the app runs.
pub(in crate::desktop) struct UiThread {
    queue: UiQueue,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub(in crate::desktop) struct UiQueue {
    tasks: crossbeam_channel::Sender<Task>,
    window: isize,
}

impl UiThread {
    pub(in crate::desktop) fn start() -> Result<Self, String> {
        let (sender, tasks) = crossbeam_channel::unbounded::<Task>();
        let (started, window) = crossbeam_channel::bounded(1);
        let thread = thread::Builder::new()
            .name("sabine-desktop".into())
            .spawn(move || {
                let window = match create_window() {
                    Ok(window) => window,
                    Err(error) => {
                        let _ = started.send(Err(error.to_string()));
                        return;
                    }
                };
                TASKS.with(|current| *current.borrow_mut() = Some(tasks));
                let _ = started.send(Ok(window.0 as isize));
                let mut message = MSG::default();
                while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
                    unsafe {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                unsafe {
                    let _ = DestroyWindow(window);
                }
            })
            .map_err(|error| error.to_string())?;
        let window = window.recv().map_err(|error| error.to_string())??;
        Ok(Self {
            queue: UiQueue {
                tasks: sender,
                window,
            },
            thread: Some(thread),
        })
    }

    pub(in crate::desktop) fn queue(&self) -> UiQueue {
        self.queue.clone()
    }
}

impl Drop for UiThread {
    fn drop(&mut self) {
        self.queue.post(QUIT);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl UiQueue {
    pub(in crate::desktop) fn run(&self, task: impl FnOnce() + Send + 'static) {
        if self.tasks.send(Box::new(task)).is_ok() {
            self.post(RUN_TASKS);
        }
    }

    fn post(&self, message: u32) {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(self.window as *mut _)),
                message,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

fn create_window() -> windows::core::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_procedure),
            hInstance: instance,
            lpszClassName: w!("SabineDesktopTasks"),
            ..WNDCLASSW::default()
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("SabineDesktopTasks"),
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
    match message {
        RUN_TASKS => {
            TASKS.with(|tasks| {
                if let Some(tasks) = tasks.borrow().as_ref() {
                    tasks.try_iter().for_each(|task| task());
                }
            });
            LRESULT(0)
        }
        QUIT => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}
