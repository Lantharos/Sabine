mod egl;
mod shader;

pub(super) use egl::terminate;

use glow::HasContext;
use khronos_egl as khronos;

use super::geometry::Frame;
use super::gst::{self, Gst, Handle, types};
use super::wayland::Target;
use shader::{Picture, VideoProgram};

/// Draws a player's newest decoded picture into its video surface on the
/// player thread, sampling GStreamer's texture directly.
pub(super) struct Presenter {
    gst: &'static Gst,
    display: &'static egl::Display,
    context: khronos::Context,
    gst_context: Handle,
    gl: glow::Context,
    program: VideoProgram,
    target: Option<Target>,
    surface: Option<egl::WindowSurface>,
    current: Option<*mut std::ffi::c_void>,
    frame: Option<Frame>,
    occluded: bool,
    shown: bool,
    sample: Option<Sample>,
}

struct Sample {
    gst: &'static Gst,
    handle: Handle,
    picture: Picture,
    sync: Handle,
}

impl Presenter {
    pub(super) fn new(gst: &'static Gst, wayland: Handle) -> Result<Self, String> {
        let display = egl::display(gst, wayland)?;
        let context = display.create_context()?;
        display
            .egl
            .make_current(display.display, None, None, Some(context))
            .map_err(|error| format!("could not use the video GL context: {error}"))?;
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                display
                    .egl
                    .get_proc_address(name)
                    .map_or(std::ptr::null(), |function| function as *const _)
            })
        };
        let (program, gst_context) = VideoProgram::new(&gl)
            .and_then(|program| Ok((program, egl::wrap_context(gst, display, context)?)))
            .inspect_err(|_| {
                let _ = display.egl.make_current(display.display, None, None, None);
                let _ = display.egl.destroy_context(display.display, context);
            })?;
        Ok(Self {
            gst,
            display,
            context,
            gst_context,
            gl,
            program,
            target: None,
            surface: None,
            current: None,
            frame: None,
            occluded: false,
            shown: false,
            sample: None,
        })
    }

    pub(super) fn gst_display(&self) -> Handle {
        self.display.gst_display
    }

    pub(super) fn gst_context(&self) -> Handle {
        self.gst_context
    }

    pub(super) fn set_target(&mut self, target: Option<Target>) {
        self.release_surface();
        self.target = target;
        self.frame = None;
        self.shown = false;
        self.draw();
    }

    pub(super) fn set_frame(&mut self, frame: Option<Frame>) {
        self.frame = frame;
        self.draw();
    }

    pub(super) fn set_occluded(&mut self, occluded: bool) {
        self.occluded = occluded;
        self.draw();
    }

    /// Takes the newest picture from `sink`, returning its display size when
    /// that changed.
    pub(super) fn take_picture(&mut self, sink: Handle) -> Option<(f64, f64)> {
        let gst = self.gst;
        let handle = unsafe {
            let sample = (gst.gst_app_sink_try_pull_sample)(sink.0, 0);
            if sample.is_null() {
                (gst.gst_app_sink_try_pull_preroll)(sink.0, 0)
            } else {
                sample
            }
        };
        if handle.is_null() {
            return None;
        }
        let sample = unsafe { self.read_sample(Handle(handle)) };
        let previous = self
            .sample
            .replace(sample)
            .map(|previous| previous.picture.display_size);
        let size = self.sample.as_ref()?.picture.display_size;
        (previous != Some(size)).then_some(size)
    }

    unsafe fn read_sample(&self, handle: Handle) -> Sample {
        let gst = self.gst;
        let (width_name, height_name, aspect_name) = (
            gst::text("width"),
            gst::text("height"),
            gst::text("pixel-aspect-ratio"),
        );
        let (mut width, mut height) = (0, 0);
        let (mut aspect_numerator, mut aspect_denominator) = (1, 1);
        unsafe {
            let buffer = (gst.gst_sample_get_buffer)(handle.0);
            let structure = (gst.gst_caps_get_structure)((gst.gst_sample_get_caps)(handle.0), 0);
            (gst.gst_structure_get_int)(structure, width_name.as_ptr(), &mut width);
            (gst.gst_structure_get_int)(structure, height_name.as_ptr(), &mut height);
            (gst.gst_structure_get_fraction)(
                structure,
                aspect_name.as_ptr(),
                &mut aspect_numerator,
                &mut aspect_denominator,
            );
            Sample {
                gst,
                handle,
                picture: Picture {
                    texture: (gst.gst_gl_memory_get_texture_id)((gst.gst_buffer_peek_memory)(
                        buffer, 0,
                    )),
                    texture_size: (width, height),
                    display_size: (
                        f64::from(width) * f64::from(aspect_numerator)
                            / f64::from(aspect_denominator),
                        f64::from(height),
                    ),
                },
                sync: Handle((gst.gst_buffer_get_meta)(
                    buffer,
                    (gst.gst_gl_sync_meta_api_get_type)(),
                )),
            }
        }
    }

    pub(super) fn draw(&mut self) {
        if self.occluded {
            return;
        }
        let Some(frame) = self.frame else {
            self.hide();
            return;
        };
        let Some(target) = &self.target else {
            return;
        };
        let surface = match self.surface.take() {
            Some(mut surface) => {
                self.display.resize(&mut surface, frame.buffer);
                surface
            }
            None => match self
                .display
                .create_surface(target.surface_pointer(), frame.buffer)
            {
                Ok(surface) => surface,
                Err(error) => {
                    eprintln!("Sabine media: {error}");
                    return;
                }
            },
        };
        let surface = self.surface.insert(surface);
        let egl = &self.display.egl;
        if self.current != Some(surface.surface.as_ptr()) {
            if let Err(error) = egl.make_current(
                self.display.display,
                Some(surface.surface),
                Some(surface.surface),
                Some(self.context),
            ) {
                eprintln!("Sabine media: could not draw video: {error}");
                return;
            }
            let _ = egl.swap_interval(self.display.display, 0);
            unsafe { self.gl.draw_buffer(glow::BACK) };
            self.current = Some(surface.surface.as_ptr());
        }
        target
            .viewport
            .set_destination(frame.destination.0, frame.destination.1);
        let sample = self.sample.as_ref();
        if let Some(sample) = sample.filter(|sample| !sample.sync.0.is_null()) {
            unsafe { (self.gst.gst_gl_sync_meta_wait)(sample.sync.0, self.gst_context.0) };
        }
        self.program
            .draw(&self.gl, &frame, sample.map(|sample| sample.picture));
        if let Some(sample) = sample.filter(|sample| !sample.sync.0.is_null()) {
            unsafe {
                (self.gst.gst_gl_sync_meta_set_sync_point)(sample.sync.0, self.gst_context.0)
            };
        }
        if let Err(error) = egl.swap_buffers(self.display.display, surface.surface) {
            eprintln!("Sabine media: could not present video: {error}");
            return;
        }
        self.shown = true;
    }

    fn hide(&mut self) {
        if let Some(target) = self.target.as_ref().filter(|_| self.shown) {
            target.hide();
            self.shown = false;
        }
    }

    fn release_surface(&mut self) {
        if let Some(surface) = self.surface.take() {
            let _ =
                self.display
                    .egl
                    .make_current(self.display.display, None, None, Some(self.context));
            self.current = None;
            self.display.destroy_surface(surface);
        }
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        self.sample = None;
        self.release_surface();
        self.target = None;
        unsafe {
            (self.gst.gst_gl_context_activate)(self.gst_context.0, types::FALSE);
            (self.gst.gst_object_unref)(self.gst_context.0);
        }
        let _ = self
            .display
            .egl
            .make_current(self.display.display, None, None, None);
        let _ = self
            .display
            .egl
            .destroy_context(self.display.display, self.context);
    }
}

impl Drop for Sample {
    fn drop(&mut self) {
        unsafe { (self.gst.gst_mini_object_unref)(self.handle.0) };
    }
}
