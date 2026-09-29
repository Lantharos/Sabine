mod api;
mod cuda;
pub(super) mod types;

use std::{
    ffi::{CStr, CString, c_void},
    sync::OnceLock,
};

pub(super) use api::Gst;
pub(super) use cuda::{CONTEXT_TYPE as CUDA_CONTEXT_TYPE, CudaContext};

/// A native object pointer handed between threads its library declares safe.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Handle(pub(super) *mut c_void);

unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

pub(super) fn gst() -> Result<&'static Gst, String> {
    static GST: OnceLock<Result<Gst, String>> = OnceLock::new();
    GST.get_or_init(|| {
        let gst = Gst::load()?;
        let mut error = std::ptr::null_mut();
        if unsafe { (gst.gst_init_check)(std::ptr::null_mut(), std::ptr::null_mut(), &mut error) }
            == types::FALSE
        {
            return Err(unsafe { take_error(&gst, error) });
        }
        Ok(gst)
    })
    .as_ref()
    .map_err(Clone::clone)
}

pub(super) fn text(value: &str) -> CString {
    CString::new(value).unwrap_or_default()
}

/// Reads and frees a `GError`.
///
/// # Safety
/// `error` must be null or a `GError` the caller owns.
pub(super) unsafe fn take_error(gst: &Gst, error: *mut types::GError) -> String {
    if error.is_null() {
        return "GStreamer reported an unknown error".to_string();
    }
    let message = unsafe { CStr::from_ptr((*error).message) }
        .to_string_lossy()
        .into_owned();
    unsafe { (gst.g_error_free)(error) };
    message
}

/// Sets a property from its serialized form, as `gst-launch` does.
///
/// # Safety
/// `object` must be a live `GObject`.
pub(super) unsafe fn set_property(gst: &Gst, object: *mut c_void, name: &str, value: &str) {
    let (name, value) = (text(name), text(value));
    unsafe { (gst.gst_util_set_object_arg)(object, name.as_ptr(), value.as_ptr()) };
}

/// Sets an object-valued property.
///
/// # Safety
/// `object` and `value` must be live `GObject`s and `value_type` the property's type.
pub(super) unsafe fn set_object_property(
    gst: &Gst,
    object: *mut c_void,
    name: &str,
    value_type: types::GType,
    value: *mut c_void,
) {
    let name = text(name);
    let mut holder = types::GValue::default();
    unsafe {
        (gst.g_value_init)(&mut holder, value_type);
        (gst.g_value_set_object)(&mut holder, value);
        (gst.g_object_set_property)(object, name.as_ptr(), &holder);
        (gst.g_value_unset)(&mut holder);
    }
}

/// Takes ownership of a string GLib allocated.
///
/// # Safety
/// `value` must be null or a string the caller owns.
pub(super) unsafe fn take_string(gst: &Gst, value: *mut std::ffi::c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let owned = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();
    unsafe { (gst.g_free)(value.cast()) };
    Some(owned)
}
