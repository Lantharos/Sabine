use std::ffi::{c_char, c_int, c_uint, c_void};

use libloading::Library;

use super::types::{
    AppSinkCallbacks, BusSyncHandler, DestroyNotify, GError, GType, GValue, MapInfo,
};

type Object = *mut c_void;

macro_rules! libraries {
    ($($file:literal { $(fn $name:ident($($argument:ty),*) $(-> $output:ty)?;)* })*) => {
        pub(in crate::media) struct Gst {
            _libraries: Vec<Library>,
            $($(pub(in crate::media) $name: unsafe extern "C" fn($($argument),*) $(-> $output)?,)*)*
        }

        impl Gst {
            pub(super) fn load() -> Result<Self, String> {
                let mut libraries = Vec::new();
                $(
                    let library = unsafe { Library::new($file) }
                        .map_err(|error| format!("{} is not installed: {error}", $file))?;
                    $(
                        let $name = unsafe {
                            *library
                                .get::<unsafe extern "C" fn($($argument),*) $(-> $output)?>(stringify!($name))
                                .map_err(|error| error.to_string())?
                        };
                    )*
                    libraries.push(library);
                )*
                Ok(Self { _libraries: libraries, $($($name,)*)* })
            }
        }
    };
}

libraries! {
    "libglib-2.0.so.0" {
        fn g_error_free(*mut GError);
        fn g_free(*mut c_void);
        fn g_list_append(*mut c_void, *mut c_void) -> *mut c_void;
        fn g_list_free(*mut c_void);
    }
    "libgobject-2.0.so.0" {
        fn g_object_set_property(Object, *const c_char, *const GValue);
        fn g_value_init(*mut GValue, GType) -> *mut GValue;
        fn g_value_set_object(*mut GValue, Object);
        fn g_value_set_uint(*mut GValue, c_uint);
        fn g_value_unset(*mut GValue);
    }
    "libgstreamer-1.0.so.0" {
        fn gst_init_check(*mut c_int, *mut c_void, *mut *mut GError) -> c_int;
        fn gst_element_get_type() -> GType;
        fn gst_element_factory_make(*const c_char, *const c_char) -> Object;
        fn gst_parse_bin_from_description(*const c_char, c_int, *mut *mut GError) -> Object;
        fn gst_bin_get_by_name(Object, *const c_char) -> Object;
        fn gst_element_set_state(Object, c_int) -> c_int;
        fn gst_element_get_bus(Object) -> Object;
        fn gst_bus_set_sync_handler(Object, Option<BusSyncHandler>, *mut c_void, Option<DestroyNotify>);
        fn gst_mini_object_unref(Object);
        fn gst_object_unref(Object);
        fn gst_message_parse_error(Object, *mut *mut GError, *mut *mut c_char);
        fn gst_message_parse_state_changed(Object, *mut c_int, *mut c_int, *mut c_int);
        fn gst_message_parse_stream_collection(Object, *mut Object);
        fn gst_message_streams_selected_get_size(Object) -> c_uint;
        fn gst_message_streams_selected_get_stream(Object, c_uint) -> Object;
        fn gst_message_parse_context_type(Object, *mut *const c_char) -> c_int;
        fn gst_message_parse_buffering(Object, *mut c_int);
        fn gst_stream_collection_get_size(Object) -> c_uint;
        fn gst_stream_collection_get_stream(Object, c_uint) -> Object;
        fn gst_stream_get_stream_id(Object) -> *const c_char;
        fn gst_stream_get_stream_type(Object) -> c_uint;
        fn gst_stream_get_stream_flags(Object) -> c_uint;
        fn gst_stream_get_tags(Object) -> Object;
        fn gst_tag_list_get_string(Object, *const c_char, *mut *mut c_char) -> c_int;
        fn gst_element_query_position(Object, c_int, *mut i64) -> c_int;
        fn gst_element_query_duration(Object, c_int, *mut i64) -> c_int;
        fn gst_element_seek(Object, f64, c_int, c_int, c_int, i64, c_int, i64) -> c_int;
        fn gst_element_send_event(Object, Object) -> c_int;
        fn gst_event_new_select_streams(*mut c_void) -> Object;
        fn gst_context_new(*const c_char, c_int) -> Object;
        fn gst_context_writable_structure(Object) -> Object;
        fn gst_structure_set_value(Object, *const c_char, *const GValue);
        fn gst_element_set_context(Object, Object);
        fn gst_util_set_object_arg(Object, *const c_char, *const c_char);
        fn gst_sample_get_buffer(Object) -> Object;
        fn gst_sample_get_caps(Object) -> Object;
        fn gst_caps_get_structure(Object, c_uint) -> Object;
        fn gst_structure_get_string(Object, *const c_char) -> *const c_char;
        fn gst_structure_get_int(Object, *const c_char, *mut c_int) -> c_int;
        fn gst_structure_get_fraction(Object, *const c_char, *mut c_int, *mut c_int) -> c_int;
        fn gst_buffer_peek_memory(Object, c_uint) -> Object;
        fn gst_buffer_get_meta(Object, GType) -> Object;
        fn gst_buffer_map(Object, *mut MapInfo, c_int) -> c_int;
        fn gst_buffer_unmap(Object, *mut MapInfo);
    }
    "libgstapp-1.0.so.0" {
        fn gst_app_sink_set_callbacks(Object, *mut AppSinkCallbacks, *mut c_void, Option<DestroyNotify>);
        fn gst_app_sink_try_pull_sample(Object, u64) -> Object;
        fn gst_app_sink_try_pull_preroll(Object, u64) -> Object;
    }
    "libgstgl-1.0.so.0" {
        fn gst_gl_display_egl_new_with_egl_display(*mut c_void) -> Object;
        fn gst_gl_display_filter_gl_api(Object, c_uint);
        fn gst_context_set_gl_display(Object, Object);
        fn gst_gl_context_new_wrapped(Object, usize, c_uint, c_uint) -> Object;
        fn gst_gl_context_activate(Object, c_int) -> c_int;
        fn gst_gl_context_fill_info(Object, *mut *mut GError) -> c_int;
        fn gst_gl_context_get_type() -> GType;
        fn gst_gl_memory_get_texture_id(Object) -> c_uint;
        fn gst_gl_sync_meta_api_get_type() -> GType;
        fn gst_gl_sync_meta_wait(Object, Object);
        fn gst_gl_sync_meta_set_sync_point(Object, Object);
    }
}
