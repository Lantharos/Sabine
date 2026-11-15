use std::{
    io::{self, Write},
    os::fd::{AsFd, FromRawFd, OwnedFd},
};

use wayland_client::{
    QueueHandle,
    protocol::{
        wl_buffer::WlBuffer,
        wl_compositor::WlCompositor,
        wl_shm::{self, WlShm},
        wl_subcompositor::WlSubcompositor,
        wl_subsurface::WlSubsurface,
        wl_surface::WlSurface,
    },
};
use wayland_protocols::wp::{
    single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1,
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

use super::worker::{State, ignore_input};

/// What the window shows beneath the page until its own device draws: its
/// background color, and for a transparent window that color only under
/// its opaque region.
pub(in crate::osr::host) struct Backdrop {
    pub(in crate::osr::host) color: [u8; 4],
    pub(in crate::osr::host) transparent: bool,
    /// The opaque region of a transparent window, in logical pixels.
    pub(in crate::osr::host) rects: Vec<(i32, i32, i32, i32)>,
}

/// Makes the one pixel buffers the backdrop is made of, which viewports
/// stretch over the window and its opaque region.
pub(super) struct Pixels<'a> {
    pub(super) single_pixel: Option<&'a WpSinglePixelBufferManagerV1>,
    pub(super) shm: &'a WlShm,
    pub(super) handle: &'a QueueHandle<State>,
}

impl Pixels<'_> {
    /// A pixel of `color`, which an opaque window shows without its alpha.
    fn buffer(&self, color: [u8; 4], opaque: bool) -> io::Result<WlBuffer> {
        let [red, green, blue, alpha] = color;
        let [red, green, blue] =
            [red, green, blue].map(|channel| (u16::from(channel) * u16::from(alpha) / 255) as u8);
        if let Some(manager) = self.single_pixel {
            let channel = |value: u8| u32::from(value) * 0x0101_0101;
            return Ok(manager.create_u32_rgba_buffer(
                channel(red),
                channel(green),
                channel(blue),
                channel(alpha),
                self.handle,
                (),
            ));
        }
        let fd = unsafe { libc::memfd_create(c"sabine-backdrop".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let pixel = [blue, green, red, alpha];
        std::fs::File::from(fd.try_clone()?).write_all(&pixel)?;
        let format = if opaque {
            wl_shm::Format::Xrgb8888
        } else {
            wl_shm::Format::Argb8888
        };
        let pool = self
            .shm
            .create_pool(fd.as_fd(), pixel.len() as i32, self.handle, ());
        let buffer = pool.create_buffer(0, 1, 1, pixel.len() as i32, format, self.handle, ());
        pool.destroy();
        Ok(buffer)
    }
}

/// The backdrop on screen: a pixel the window's own viewport stretches over
/// it, and for each opaque rectangle a pixel in a subsurface beneath the
/// page.
pub(super) struct BackdropSurfaces {
    window: WlBuffer,
    rects: Vec<RectSurface>,
}

struct RectSurface {
    surface: WlSurface,
    subsurface: WlSubsurface,
    viewport: WpViewport,
    buffer: WlBuffer,
}

impl BackdropSurfaces {
    pub(super) fn new(
        pixels: &Pixels<'_>,
        (compositor, subcompositor, viewporter): (&WlCompositor, &WlSubcompositor, &WpViewporter),
        (parent, page): (&WlSurface, &WlSurface),
        backdrop: &Backdrop,
    ) -> io::Result<Self> {
        let handle = pixels.handle;
        let window = if backdrop.transparent {
            pixels.buffer([0; 4], false)?
        } else {
            pixels.buffer(backdrop.color, true)?
        };
        let mut rects = Vec::with_capacity(backdrop.rects.len());
        for &(x, y, width, height) in &backdrop.rects {
            if width <= 0 || height <= 0 {
                continue;
            }
            let surface = compositor.create_surface(handle, ());
            let subsurface = subcompositor.get_subsurface(&surface, parent, handle, ());
            subsurface.place_below(page);
            subsurface.set_position(x, y);
            ignore_input(compositor, handle, &surface);
            let viewport = viewporter.get_viewport(&surface, handle, ());
            viewport.set_destination(width, height);
            let buffer = pixels.buffer(backdrop.color, false)?;
            surface.attach(Some(&buffer), 0, 0);
            surface.damage_buffer(0, 0, 1, 1);
            surface.commit();
            rects.push(RectSurface {
                surface,
                subsurface,
                viewport,
                buffer,
            });
        }
        Ok(Self { window, rects })
    }

    /// Puts the backdrop beneath the page with the window's next commit.
    pub(super) fn attach_beneath(&self, parent: &WlSurface) {
        parent.attach(Some(&self.window), 0, 0);
        parent.damage_buffer(0, 0, i32::MAX, i32::MAX);
    }

    /// Takes the opaque rectangles down with the window's next commit, once
    /// the window's device draws its own.
    pub(super) fn retire(&self) {
        for rect in &self.rects {
            rect.surface.attach(None, 0, 0);
            rect.surface.commit();
        }
    }

    pub(super) fn destroy(self) {
        for rect in self.rects {
            rect.viewport.destroy();
            rect.subsurface.destroy();
            rect.surface.destroy();
            rect.buffer.destroy();
        }
        self.window.destroy();
    }
}
