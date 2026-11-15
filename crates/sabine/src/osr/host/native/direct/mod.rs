mod backdrop;
mod buffers;
mod dispatch;
mod page;
mod timing;
mod worker;

pub(in crate::osr::host) use backdrop::Backdrop;
pub(in crate::osr::host) use page::PageRoute;

use std::{
    ffi::c_void,
    io::Write,
    os::unix::net::UnixStream,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

use crossbeam_channel::Sender;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::{Connection, Proxy, protocol::wl_surface::WlSurface};
use winit::window::Window;

use crate::osr::accel::Dmabuf;
use crate::osr::control::SlotRelease;
use crate::osr::host::input::{InputLatency, InputStamp};
use crate::osr::protocol::TextureRect;

use worker::State;

/// A page frame shown straight from the browser's dma-buf. Dropping it
/// gives the browser its slot back.
pub(in crate::osr::host) struct DirectFrame {
    pub(in crate::osr::host) dmabuf: Arc<Dmabuf>,
    pub(in crate::osr::host) resource_id: u64,
    pub(in crate::osr::host) format: u32,
    pub(in crate::osr::host) source: (u32, u32, u32, u32),
    pub(in crate::osr::host) destination: (i32, i32),
    pub(in crate::osr::host) damage: TextureRect,
    pub(in crate::osr::host) sequence: u64,
    pub(in crate::osr::host) release: SlotRelease,
    pub(in crate::osr::host) inputs: Vec<InputStamp>,
}

/// What became of the frames handed to the compositor.
pub(in crate::osr::host) enum DirectOutcome {
    Shown,
    /// The subsurface takes the page down with the window's next commit.
    Hidden(u64),
    /// The subsurface shows the page again from the window's next commit.
    Revealed(u64),
    Failed(String),
}

enum Command {
    Present(DirectFrame),
    Map(Backdrop),
    Hide(u64),
    Reveal(DirectFrame, u64),
    Desync,
    RetireBackdrop,
}

/// Which of the window and the subsurface shows the page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// The subsurface shows the page, which the window leaves out.
    Subsurface,
    /// The subsurface takes the page down, and the window holds its next
    /// frame back until that is under way.
    Hiding(u64),
    /// The window's device draws the page.
    Window,
    /// The subsurface shows the page again once the compositor has its
    /// frame, while the window's device keeps drawing the one it holds.
    Revealing(u64),
}

/// Shows the page's frames in a subsurface of the window, imported by the
/// compositor itself. The window's first frame shows this way before its
/// own graphics device has started, as Chromium and the window would
/// otherwise start their GPU drivers at the same time. Afterwards the page
/// stays in the subsurface whenever the window draws nothing over it, so a
/// frame costs the window no drawing of its own.
pub(in crate::osr::host) struct DirectPresenter {
    connection: Connection,
    commands: Sender<Command>,
    wake: UnixStream,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    placement: Placement,
    transitions: u64,
    backdrop_retired: bool,
    failed: bool,
}

impl DirectPresenter {
    /// Starts presenting on `window`'s Wayland connection, or returns `None`
    /// when the compositor cannot import dma-bufs into a scaled subsurface.
    pub(in crate::osr::host) fn start(
        window: &dyn Window,
        transparent: bool,
        latency: Option<InputLatency>,
        report: impl Fn(DirectOutcome) + Send + 'static,
    ) -> Option<Self> {
        let (Ok(RawDisplayHandle::Wayland(display)), Ok(RawWindowHandle::Wayland(surface))) = (
            window.display_handle().map(|handle| handle.as_raw()),
            window.window_handle().map(|handle| handle.as_raw()),
        ) else {
            return None;
        };
        unsafe {
            Self::connect(
                display.display.as_ptr(),
                surface.surface.as_ptr(),
                transparent,
                latency,
                report,
            )
        }
        .map_err(|error| {
            sabine_runtime::report_error(
                "render",
                format!("the page's frames go through the window's graphics device: {error}"),
            );
        })
        .ok()
    }

    unsafe fn connect(
        display: *mut c_void,
        surface: *mut c_void,
        transparent: bool,
        latency: Option<InputLatency>,
        report: impl Fn(DirectOutcome) + Send + 'static,
    ) -> Result<Self, String> {
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let connection = Connection::from_backend(backend);
        let surface_id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }
            .map_err(|error| error.to_string())?;
        let parent =
            WlSurface::from_id(&connection, surface_id).map_err(|error| error.to_string())?;
        let (wake, wake_reader) = UnixStream::pair().map_err(|error| error.to_string())?;
        for socket in [&wake, &wake_reader] {
            socket
                .set_nonblocking(true)
                .map_err(|error| error.to_string())?;
        }
        let (queue, state) =
            State::connect(&connection, parent, transparent, latency, Box::new(report))?;
        let (commands, receiver) = crossbeam_channel::unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("sabine-direct-present".to_string())
            .spawn(move || worker::run(queue, state, receiver, wake_reader, worker_stop))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            connection,
            commands,
            wake,
            stop,
            worker: Some(worker),
            placement: Placement::Subsurface,
            transitions: 0,
            backdrop_retired: false,
            failed: false,
        })
    }

    /// Whether the subsurface shows the page, so the window leaves it out.
    pub(in crate::osr::host) fn shows_page(&self) -> bool {
        self.placement == Placement::Subsurface
    }

    /// Whether the subsurface shows the page or is about to, so the window
    /// takes it back only with a frame newer than the subsurface's.
    pub(in crate::osr::host) fn holds_page(&self) -> bool {
        matches!(
            self.placement,
            Placement::Subsurface | Placement::Revealing(_)
        )
    }

    /// Whether the window holds its frames back until the subsurface took
    /// the page down.
    pub(in crate::osr::host) fn hiding(&self) -> bool {
        matches!(self.placement, Placement::Hiding(_))
    }

    pub(in crate::osr::host) fn failed(&self) -> bool {
        self.failed
    }

    /// Stops showing frames here after the compositor could not. A page the
    /// subsurface was taking down is the window's from now on.
    pub(in crate::osr::host) fn note_failure(&mut self) {
        self.failed = true;
        if self.hiding() {
            self.placement = Placement::Window;
        }
    }

    pub(in crate::osr::host) fn present(&self, frame: DirectFrame) -> bool {
        self.send(Command::Present(frame))
    }

    /// Puts the window on screen with the page's latest frame.
    pub(in crate::osr::host) fn map(&self, backdrop: Backdrop) {
        self.send(Command::Map(backdrop));
    }

    /// Takes the backdrop the window showed before its device started down
    /// with the window's next commit, which draws its own.
    pub(in crate::osr::host) fn retire_backdrop(&mut self) {
        if !std::mem::replace(&mut self.backdrop_retired, true) {
            self.send(Command::RetireBackdrop);
        }
    }

    /// Takes the page's subsurface down with the window's next commit. The
    /// window draws again once the compositor connection carries that.
    pub(in crate::osr::host) fn hide_with_next_commit(&mut self) {
        self.transitions += 1;
        self.placement = if self.send(Command::Hide(self.transitions)) {
            Placement::Hiding(self.transitions)
        } else {
            Placement::Window
        };
    }

    /// Shows `frame` in the subsurface again, from the window commit that
    /// follows the compositor taking it. Returns whether it was handed on.
    pub(in crate::osr::host) fn reveal_with_next_commit(&mut self, frame: DirectFrame) -> bool {
        self.transitions += 1;
        let sent = self.send(Command::Reveal(frame, self.transitions));
        if sent {
            self.placement = Placement::Revealing(self.transitions);
        }
        sent
    }

    /// Takes on the transition the compositor connection now carries, unless
    /// a later one replaced it. Returns whether it did.
    pub(in crate::osr::host) fn settle(&mut self, transition: u64) -> bool {
        self.placement = match self.placement {
            Placement::Hiding(pending) if pending == transition => Placement::Window,
            Placement::Revealing(pending) if pending == transition => Placement::Subsurface,
            _ => return false,
        };
        true
    }

    /// Lets the subsurface show frames on its own again, after the window's
    /// commit that revealed it.
    pub(in crate::osr::host) fn desync(&self) {
        self.send(Command::Desync);
    }

    fn send(&self, command: Command) -> bool {
        let sent = self.commands.send(command).is_ok();
        if sent {
            let _ = (&self.wake).write(&[1]);
        }
        sent
    }
}

impl Drop for DirectPresenter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = (&self.wake).write(&[1]);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = self.connection.flush();
    }
}
