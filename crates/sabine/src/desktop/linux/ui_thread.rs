use std::thread::{self, JoinHandle};

type Task = Box<dyn FnOnce() + Send>;

/// The thread that owns the tray icon. Its menus are not thread-safe, so
/// every change to them runs here, in order.
pub(in crate::desktop) struct UiThread {
    queue: UiQueue,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub(in crate::desktop) struct UiQueue(crossbeam_channel::Sender<Option<Task>>);

impl UiThread {
    pub(in crate::desktop) fn start() -> Result<Self, String> {
        let (sender, tasks) = crossbeam_channel::unbounded::<Option<Task>>();
        let thread = thread::Builder::new()
            .name("sabine-desktop".into())
            .spawn(move || {
                while let Ok(Some(task)) = tasks.recv() {
                    task();
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            queue: UiQueue(sender),
            thread: Some(thread),
        })
    }

    pub(in crate::desktop) fn queue(&self) -> UiQueue {
        self.queue.clone()
    }
}

impl Drop for UiThread {
    fn drop(&mut self) {
        let _ = self.queue.0.send(None);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl UiQueue {
    pub(in crate::desktop) fn run(&self, task: impl FnOnce() + Send + 'static) {
        let _ = self.0.send(Some(Box::new(task)));
    }
}
