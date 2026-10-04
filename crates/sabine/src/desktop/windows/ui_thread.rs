use std::thread::{self, JoinHandle};

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
        TranslateMessage, WM_APP, WM_QUIT,
    },
};

type Task = Box<dyn FnOnce() + Send>;

const RUN_TASKS: u32 = WM_APP;

/// A thread with its own message loop for the tray icon and hotkeys, whose
/// hidden windows need one. It runs beside whatever loop the app runs.
pub(in crate::desktop) struct UiThread {
    queue: UiQueue,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub(in crate::desktop) struct UiQueue {
    tasks: crossbeam_channel::Sender<Task>,
    thread_id: u32,
}

impl UiThread {
    pub(in crate::desktop) fn start() -> Result<Self, String> {
        let (sender, tasks) = crossbeam_channel::unbounded::<Task>();
        let (started, thread_id) = crossbeam_channel::bounded(1);
        let thread = thread::Builder::new()
            .name("sabine-desktop".into())
            .spawn(move || {
                let mut message = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                }
                let _ = started.send(unsafe { GetCurrentThreadId() });
                while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
                    if message.hwnd.is_invalid() && message.message == RUN_TASKS {
                        tasks.try_iter().for_each(|task| task());
                        continue;
                    }
                    unsafe {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        let thread_id = thread_id.recv().map_err(|error| error.to_string())?;
        Ok(Self {
            queue: UiQueue {
                tasks: sender,
                thread_id,
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
        self.queue.post(WM_QUIT);
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
            let _ = PostThreadMessageW(self.thread_id, message, WPARAM(0), LPARAM(0));
        }
    }
}
