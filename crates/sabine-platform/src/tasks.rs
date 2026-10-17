use std::sync::OnceLock;

use tokio::runtime::{Builder, Handle, Runtime};

const WORKERS: usize = 2;

/// The Tokio runtime that awaits async bridge handlers and runs Sabine's
/// desktop services, such as notifications, the tray icon and portal
/// requests. Apps spawn their own async work on it too, so a process keeps
/// one small pool of async threads. It starts the first time it is used.
pub fn async_runtime() -> &'static Handle {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            Builder::new_multi_thread()
                .worker_threads(WORKERS)
                .thread_name("sabine-async")
                .enable_all()
                .build()
                .expect("Sabine's async runtime starts")
        })
        .handle()
}
