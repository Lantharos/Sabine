use std::{
    ffi::{c_int, c_uint, c_void},
    sync::OnceLock,
};

use libloading::Library;

use super::{Gst, Handle, types};

pub(in crate::media) const CONTEXT_TYPE: &str = "gst.cuda.context";
const SCHEDULE_BLOCKING_SYNC: c_uint = 0x4;
const UINT_TYPE: types::GType = 7 << 2;

type Device = c_int;

struct Cuda {
    _library: Library,
    load_library: unsafe extern "C" fn() -> c_int,
    init: unsafe extern "C" fn(c_uint) -> c_int,
    device_get: unsafe extern "C" fn(*mut Device, c_int) -> c_int,
    context_create: unsafe extern "C" fn(*mut *mut c_void, c_uint, Device) -> c_int,
    context_pop: unsafe extern "C" fn(*mut *mut c_void) -> c_int,
    context_destroy: unsafe extern "C" fn(*mut c_void) -> c_int,
    context_get_type: unsafe extern "C" fn() -> types::GType,
    context_new_wrapped: unsafe extern "C" fn(*mut c_void, Device) -> *mut c_void,
}

/// A CUDA context for NVDEC that sleeps on blocking sync. NVDEC's default
/// context spins a CPU core while it waits for the GPU.
pub(in crate::media) struct CudaContext {
    gst: &'static Gst,
    cuda: &'static Cuda,
    pub(in crate::media) context: Handle,
    handle: Handle,
}

impl Cuda {
    fn get() -> Option<&'static Self> {
        static CUDA: OnceLock<Option<Cuda>> = OnceLock::new();
        CUDA.get_or_init(Self::load).as_ref()
    }

    fn load() -> Option<Self> {
        let library = unsafe { Library::new("libgstcuda-1.0.so.0") }.ok()?;
        let cuda = unsafe {
            Self {
                load_library: *library.get("gst_cuda_load_library").ok()?,
                init: *library.get("CuInit").ok()?,
                device_get: *library.get("CuDeviceGet").ok()?,
                context_create: *library.get("CuCtxCreate").ok()?,
                context_pop: *library.get("CuCtxPopCurrent").ok()?,
                context_destroy: *library.get("CuCtxDestroy").ok()?,
                context_get_type: *library.get("gst_cuda_context_get_type").ok()?,
                context_new_wrapped: *library.get("gst_cuda_context_new_wrapped").ok()?,
                _library: library,
            }
        };
        (unsafe { (cuda.load_library)() } != types::FALSE && unsafe { (cuda.init)(0) } == 0)
            .then_some(cuda)
    }
}

impl CudaContext {
    pub(in crate::media) fn create(gst: &'static Gst) -> Option<Self> {
        let cuda = Cuda::get()?;
        let mut device = 0;
        let mut handle = std::ptr::null_mut();
        unsafe {
            if (cuda.device_get)(&mut device, 0) != 0
                || (cuda.context_create)(&mut handle, SCHEDULE_BLOCKING_SYNC, device) != 0
            {
                return None;
            }
            (cuda.context_pop)(std::ptr::null_mut());
            let shared = (cuda.context_new_wrapped)(handle, device);
            let name = super::text(CONTEXT_TYPE);
            let context = (gst.gst_context_new)(name.as_ptr(), types::TRUE);
            let structure = (gst.gst_context_writable_structure)(context);
            let mut value = types::GValue::default();
            (gst.g_value_init)(&mut value, (cuda.context_get_type)());
            (gst.g_value_set_object)(&mut value, shared);
            (gst.gst_structure_set_value)(structure, name.as_ptr(), &value);
            (gst.g_value_unset)(&mut value);
            (gst.gst_object_unref)(shared);
            let device_field = super::text("cuda-device-id");
            (gst.g_value_init)(&mut value, UINT_TYPE);
            (gst.g_value_set_uint)(&mut value, device as c_uint);
            (gst.gst_structure_set_value)(structure, device_field.as_ptr(), &value);
            (gst.g_value_unset)(&mut value);
            Some(Self {
                gst,
                cuda,
                context: Handle(context),
                handle: Handle(handle),
            })
        }
    }
}

impl Drop for CudaContext {
    fn drop(&mut self) {
        unsafe {
            (self.gst.gst_mini_object_unref)(self.context.0);
            (self.cuda.context_destroy)(self.handle.0);
        }
    }
}
