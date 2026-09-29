use std::{
    ffi::{c_int, c_void},
    sync::OnceLock,
};

use khronos_egl as egl;
use libloading::Library;

use crate::media::gst::{self, Gst, Handle};

type Egl = egl::DynamicInstance<egl::EGL1_5>;

const PLATFORM_WAYLAND: egl::Enum = 0x31D8;

/// The process-wide EGL display on the window's Wayland connection, shared
/// with GStreamer so decoded textures and the presenter share one namespace.
pub(super) struct Display {
    pub(super) egl: Egl,
    pub(super) display: egl::Display,
    pub(super) config: egl::Config,
    pub(super) gst_display: Handle,
    window: WaylandEgl,
}

unsafe impl Send for Display {}
unsafe impl Sync for Display {}

struct WaylandEgl {
    _library: Library,
    create: unsafe extern "C" fn(*mut c_void, c_int, c_int) -> *mut c_void,
    resize: unsafe extern "C" fn(*mut c_void, c_int, c_int, c_int, c_int),
    destroy: unsafe extern "C" fn(*mut c_void),
}

/// An EGL window surface on a video surface.
pub(super) struct WindowSurface {
    pub(super) surface: egl::Surface,
    window: *mut c_void,
    size: (i32, i32),
}

static DISPLAY: OnceLock<Result<Display, String>> = OnceLock::new();

pub(super) fn display(gst: &Gst, wayland: Handle) -> Result<&'static Display, String> {
    DISPLAY
        .get_or_init(|| Display::open(gst, wayland))
        .as_ref()
        .map_err(Clone::clone)
}

/// Releases the EGL display before the Wayland connection it was opened on closes.
pub(in crate::media) fn terminate() {
    if let Some(Ok(display)) = DISPLAY.get() {
        let _ = display.egl.terminate(display.display);
    }
}

impl Display {
    fn open(gst: &Gst, wayland: Handle) -> Result<Self, String> {
        let egl = unsafe { Egl::load_required() }.map_err(|error| error.to_string())?;
        let display =
            unsafe { egl.get_platform_display(PLATFORM_WAYLAND, wayland.0, &[egl::ATTRIB_NONE]) }
                .map_err(|error| format!("could not open the EGL display: {error}"))?;
        egl.initialize(display)
            .map_err(|error| format!("could not initialize EGL: {error}"))?;
        let config = egl
            .choose_first_config(
                display,
                &[
                    egl::RED_SIZE,
                    8,
                    egl::GREEN_SIZE,
                    8,
                    egl::BLUE_SIZE,
                    8,
                    egl::ALPHA_SIZE,
                    8,
                    egl::SURFACE_TYPE,
                    egl::WINDOW_BIT,
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_BIT,
                    egl::NONE,
                ],
            )
            .map_err(|error| error.to_string())?
            .ok_or("no EGL configuration can draw video")?;
        let window = WaylandEgl::load()?;
        let gst_display = unsafe {
            let gst_display = (gst.gst_gl_display_egl_new_with_egl_display)(display.as_ptr());
            (gst.gst_gl_display_filter_gl_api)(gst_display, gst::types::GL_API_OPENGL3);
            Handle(gst_display)
        };
        Ok(Self {
            egl,
            display,
            config,
            gst_display,
            window,
        })
    }

    pub(super) fn create_context(&self) -> Result<egl::Context, String> {
        self.egl
            .bind_api(egl::OPENGL_API)
            .map_err(|error| error.to_string())?;
        self.egl
            .create_context(
                self.display,
                self.config,
                None,
                &[
                    egl::CONTEXT_MAJOR_VERSION,
                    3,
                    egl::CONTEXT_MINOR_VERSION,
                    3,
                    egl::CONTEXT_OPENGL_PROFILE_MASK,
                    egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
                    egl::NONE,
                ],
            )
            .map_err(|error| format!("could not create a video GL context: {error}"))
    }

    pub(super) fn create_surface(
        &self,
        surface: *mut c_void,
        size: (i32, i32),
    ) -> Result<WindowSurface, String> {
        let window = unsafe { (self.window.create)(surface, size.0, size.1) };
        if window.is_null() {
            return Err("could not create a video window".to_string());
        }
        match unsafe {
            self.egl.create_platform_window_surface(
                self.display,
                self.config,
                window,
                &[egl::ATTRIB_NONE],
            )
        } {
            Ok(surface) => Ok(WindowSurface {
                surface,
                window,
                size,
            }),
            Err(error) => {
                unsafe { (self.window.destroy)(window) };
                Err(format!("could not create a video surface: {error}"))
            }
        }
    }

    pub(super) fn resize(&self, surface: &mut WindowSurface, size: (i32, i32)) {
        if surface.size != size {
            unsafe { (self.window.resize)(surface.window, size.0, size.1, 0, 0) };
            surface.size = size;
        }
    }

    pub(super) fn destroy_surface(&self, surface: WindowSurface) {
        let _ = self.egl.destroy_surface(self.display, surface.surface);
        unsafe { (self.window.destroy)(surface.window) };
    }
}

impl WaylandEgl {
    fn load() -> Result<Self, String> {
        let library = unsafe { Library::new("libwayland-egl.so.1") }
            .map_err(|error| format!("libwayland-egl is not installed: {error}"))?;
        unsafe {
            Ok(Self {
                create: *library
                    .get("wl_egl_window_create")
                    .map_err(|error| error.to_string())?,
                resize: *library
                    .get("wl_egl_window_resize")
                    .map_err(|error| error.to_string())?,
                destroy: *library
                    .get("wl_egl_window_destroy")
                    .map_err(|error| error.to_string())?,
                _library: library,
            })
        }
    }
}

/// Wraps the presenter's context for GStreamer, which then creates its own
/// contexts sharing textures with it.
pub(super) fn wrap_context(
    gst: &Gst,
    display: &Display,
    context: egl::Context,
) -> Result<Handle, String> {
    unsafe {
        let wrapped = (gst.gst_gl_context_new_wrapped)(
            display.gst_display.0,
            context.as_ptr() as usize,
            gst::types::GL_PLATFORM_EGL,
            gst::types::GL_API_OPENGL3,
        );
        if wrapped.is_null() {
            return Err("GStreamer could not use the video GL context".to_string());
        }
        (gst.gst_gl_context_activate)(wrapped, gst::types::TRUE);
        let mut error = std::ptr::null_mut();
        if (gst.gst_gl_context_fill_info)(wrapped, &mut error) == gst::types::FALSE {
            let message = gst::take_error(gst, error);
            (gst.gst_object_unref)(wrapped);
            return Err(message);
        }
        Ok(Handle(wrapped))
    }
}
