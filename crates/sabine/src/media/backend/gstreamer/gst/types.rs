use std::ffi::{c_char, c_int, c_void};

pub(in crate::media) type GType = usize;
pub(in crate::media) type BusSyncHandler =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> c_int;
pub(in crate::media) type DestroyNotify = unsafe extern "C" fn(*mut c_void);
pub(in crate::media) type AppSinkCallback = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;

pub(in crate::media) const TRUE: c_int = 1;
pub(in crate::media) const FALSE: c_int = 0;
pub(in crate::media) const CLOCK_TIME_NONE: u64 = u64::MAX;
pub(in crate::media) const SECOND: f64 = 1_000_000_000.0;

pub(in crate::media) const STATE_VOID_PENDING: c_int = 0;
pub(in crate::media) const STATE_NULL: c_int = 1;
pub(in crate::media) const STATE_PAUSED: c_int = 3;
pub(in crate::media) const STATE_PLAYING: c_int = 4;

pub(in crate::media) const FORMAT_TIME: c_int = 3;
pub(in crate::media) const SEEK_TYPE_NONE: c_int = 0;
pub(in crate::media) const SEEK_TYPE_SET: c_int = 1;
pub(in crate::media) const SEEK_FLAG_FLUSH: c_int = 1 << 0;
pub(in crate::media) const SEEK_FLAG_ACCURATE: c_int = 1 << 1;
pub(in crate::media) const SEEK_FLAG_KEY_UNIT: c_int = 1 << 2;
pub(in crate::media) const SEEK_FLAG_SNAP_NEAREST: c_int = (1 << 5) | (1 << 6);
pub(in crate::media) const SEEK_FLAG_INSTANT_RATE_CHANGE: c_int = 1 << 10;

pub(in crate::media) const BUS_DROP: c_int = 0;
pub(in crate::media) const FLOW_OK: c_int = 0;
pub(in crate::media) const MAP_READ: c_int = 1;

pub(in crate::media) const MESSAGE_EOS: u32 = 1 << 0;
pub(in crate::media) const MESSAGE_ERROR: u32 = 1 << 1;
pub(in crate::media) const MESSAGE_BUFFERING: u32 = 1 << 5;
pub(in crate::media) const MESSAGE_STATE_CHANGED: u32 = 1 << 6;
pub(in crate::media) const MESSAGE_DURATION_CHANGED: u32 = 1 << 18;
pub(in crate::media) const MESSAGE_ASYNC_DONE: u32 = 1 << 21;
pub(in crate::media) const MESSAGE_NEED_CONTEXT: u32 = 1 << 29;
pub(in crate::media) const MESSAGE_STREAM_COLLECTION: u32 = (1 << 31) + 4;
pub(in crate::media) const MESSAGE_STREAMS_SELECTED: u32 = (1 << 31) + 5;

pub(in crate::media) const STREAM_TYPE_AUDIO: u32 = 1 << 1;
pub(in crate::media) const STREAM_TYPE_VIDEO: u32 = 1 << 2;
pub(in crate::media) const STREAM_TYPE_TEXT: u32 = 1 << 4;
pub(in crate::media) const STREAM_FLAG_SELECT: u32 = 1 << 1;

pub(in crate::media) const GL_PLATFORM_EGL: u32 = 1 << 0;
pub(in crate::media) const GL_API_OPENGL3: u32 = 1 << 1;

#[repr(C)]
pub(in crate::media) struct GError {
    pub(in crate::media) domain: u32,
    pub(in crate::media) code: c_int,
    pub(in crate::media) message: *mut c_char,
}

#[repr(C)]
#[derive(Default)]
pub(in crate::media) struct GValue {
    g_type: GType,
    data: [u64; 2],
}

const MINI_OBJECT_SIZE: usize = 64;

#[repr(C)]
pub(in crate::media) struct MessageHeader {
    _mini_object: [u8; MINI_OBJECT_SIZE],
    pub(in crate::media) kind: u32,
    _timestamp: u64,
    pub(in crate::media) source: *mut c_void,
}

#[repr(C)]
pub(in crate::media) struct BufferHeader {
    _mini_object: [u8; MINI_OBJECT_SIZE],
    _pool: *mut c_void,
    pub(in crate::media) pts: u64,
    _dts: u64,
    pub(in crate::media) duration: u64,
}

#[repr(C)]
pub(in crate::media) struct MapInfo {
    _memory: *mut c_void,
    _flags: c_int,
    pub(in crate::media) data: *mut u8,
    pub(in crate::media) size: usize,
    _maxsize: usize,
    _user_data: [*mut c_void; 4],
    _reserved: [*mut c_void; 4],
}

impl Default for MapInfo {
    fn default() -> Self {
        Self {
            _memory: std::ptr::null_mut(),
            _flags: 0,
            data: std::ptr::null_mut(),
            size: 0,
            _maxsize: 0,
            _user_data: [std::ptr::null_mut(); 4],
            _reserved: [std::ptr::null_mut(); 4],
        }
    }
}

#[repr(C)]
pub(in crate::media) struct AppSinkCallbacks {
    pub(in crate::media) eos: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    pub(in crate::media) new_preroll: Option<AppSinkCallback>,
    pub(in crate::media) new_sample: Option<AppSinkCallback>,
    pub(in crate::media) new_event: Option<AppSinkCallback>,
    pub(in crate::media) propose_allocation:
        Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> c_int>,
    _reserved: [*mut c_void; 2],
}

impl AppSinkCallbacks {
    pub(in crate::media) fn samples(
        new_preroll: Option<AppSinkCallback>,
        new_sample: AppSinkCallback,
    ) -> Self {
        Self {
            eos: None,
            new_preroll,
            new_sample: Some(new_sample),
            new_event: None,
            propose_allocation: None,
            _reserved: [std::ptr::null_mut(); 2],
        }
    }
}
