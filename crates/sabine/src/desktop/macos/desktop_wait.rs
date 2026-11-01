//! macOS runs the app's tray icon, hotkeys and URL events on the main
//! thread's event loop, so waiting on the main thread runs that loop.

use std::{io, thread};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, run_on_demand::EventLoopExtRunOnDemand},
    window::WindowId,
};

use super::SabineProcess;

pub(super) fn wait(process: &mut SabineProcess) -> io::Result<()> {
    let mut event_loop = EventLoop::new().map_err(io::Error::other)?;
    let proxy = event_loop.create_proxy();
    let (sender, receiver) = crossbeam_channel::unbounded();
    let commands = std::mem::replace(&mut process.command_receiver, receiver);
    let exits = process.child_exit_receiver.clone();
    let (stop, stopped) = crossbeam_channel::bounded::<()>(1);
    let worker = thread::spawn(move || {
        loop {
            crossbeam_channel::select! {
                recv(stopped) -> _ => break,
                recv(exits) -> result => {
                    proxy.wake_up();
                    if result.is_err() { break; }
                },
                recv(commands) -> command => {
                    let Ok(command) = command else { break };
                    if sender.send(command).is_err() { break; }
                    proxy.wake_up();
                },
            }
        }
    });
    let mut app = DesktopWait {
        process,
        result: Ok(()),
    };
    let result = event_loop
        .run_app_on_demand(&mut app)
        .map_err(io::Error::other);
    drop(stop);
    let _ = worker.join();
    result.and(app.result)
}

struct DesktopWait<'a> {
    process: &'a mut SabineProcess,
    result: io::Result<()>,
}

impl DesktopWait<'_> {
    fn dispatch(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(command) = self.process.command_receiver.try_recv() {
            self.process.handle_command(command);
        }
        match self.process.collect_exited_windows() {
            Ok(true) => event_loop.exit(),
            Ok(false) => {}
            Err(error) => {
                self.result = Err(error);
                event_loop.exit();
            }
        }
    }
}

impl ApplicationHandler for DesktopWait<'_> {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        self.dispatch(event_loop);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.dispatch(event_loop);
    }

    fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
