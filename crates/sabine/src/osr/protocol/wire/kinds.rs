//! Kinds of the messages the browser host sends its window, matching
//! `osr/transport/message_kinds.h` and `osr/accelerated/protocol.h`.

pub(super) const KIND_POPUP_HIDDEN: u32 = 3;
pub(super) const KIND_CURSOR: u32 = 4;
pub(super) const KIND_CLOSE_REQUESTED: u32 = 5;
pub(super) const KIND_START_DRAG_REQUESTED: u32 = 6;
pub(super) const KIND_MINIMIZE_REQUESTED: u32 = 7;
pub(super) const KIND_TOGGLE_MAXIMIZE_REQUESTED: u32 = 8;
pub(super) const KIND_SHOW_REQUESTED: u32 = 9;
pub(super) const KIND_HIDE_REQUESTED: u32 = 10;
pub(super) const KIND_FOCUS_REQUESTED: u32 = 11;
pub(super) const KIND_MAIN_BATCH: u32 = 12;
pub(super) const KIND_POPUP_BATCH: u32 = 13;
pub(super) const KIND_MAIN_SHARED_BATCH: u32 = 14;
pub(super) const KIND_POPUP_SHARED_BATCH: u32 = 15;
pub(super) const KIND_DRAG_STARTED: u32 = 16;
pub(super) const KIND_GUEST_BATCH: u32 = 18;
pub(super) const KIND_GUEST_SHARED_BATCH: u32 = 19;
pub(super) const KIND_GUEST_HIDDEN: u32 = 20;
pub(super) const KIND_DRAGGABLE_REGIONS_CHANGED: u32 = 21;
pub(super) const KIND_GUEST_CAPTURE_REQUESTED: u32 = 22;
pub(super) const KIND_BRIDGE_REQUEST: u32 = 23;
pub(super) const KIND_MAIN_ACCEL: u32 = 24;
pub(super) const KIND_POPUP_ACCEL: u32 = 25;
pub(super) const KIND_GUEST_ACCEL: u32 = 26;
pub(super) const KIND_FULLSCREEN_REQUESTED: u32 = 27;
pub(super) const KIND_EXIT_FULLSCREEN_REQUESTED: u32 = 28;
pub(super) const KIND_MAIN_LOAD_STARTED: u32 = 29;
pub(super) const KIND_MAIN_LOAD_READY: u32 = 30;
pub(super) const KIND_IME_STATE_CHANGED: u32 = 31;
pub(super) const KIND_IME_CURSOR_AREA_CHANGED: u32 = 32;
pub(super) const KIND_TOOLTIP_CHANGED: u32 = 33;
pub(super) const KIND_IME_SURROUNDING_CHANGED: u32 = 34;
pub(super) const KIND_MAXIMIZE_REQUESTED: u32 = 35;
pub(super) const KIND_RESTORE_REQUESTED: u32 = 36;
pub(super) const KIND_FATAL_ERROR: u32 = 37;
pub(super) const KIND_HOST_HELLO: u32 = 38;
#[cfg(any(windows, target_os = "linux"))]
pub(super) const KIND_ACCEL_RETIRE: u32 = 39;
pub(super) const KIND_DRAG_OPERATION: u32 = 40;
pub(super) const KIND_CONTEXT_MENU: u32 = 41;
pub(super) const KIND_CONTEXT_MENU_DISMISSED: u32 = 42;
pub(super) const KIND_FILE_DIALOG: u32 = 43;
pub(super) const KIND_MEDIA_PLAYING: u32 = 44;
#[cfg(target_os = "linux")]
pub(super) const KIND_ACCEL_DMABUF: u32 = 45;
pub(super) const KIND_ACCEL_UNAVAILABLE: u32 = 46;
#[cfg(target_os = "linux")]
pub(super) const KIND_PRINT_DIALOG: u32 = 47;
#[cfg(target_os = "linux")]
pub(super) const KIND_PRINT_JOB: u32 = 48;
pub(super) const KIND_PRINT_DOCUMENT: u32 = 49;
pub(super) const KIND_GUEST_ORDER: u32 = 50;

pub(super) fn is_paint_kind(kind: u32) -> bool {
    matches!(
        kind,
        KIND_MAIN_ACCEL
            | KIND_POPUP_ACCEL
            | KIND_GUEST_ACCEL
            | KIND_MAIN_BATCH
            | KIND_POPUP_BATCH
            | KIND_MAIN_SHARED_BATCH
            | KIND_POPUP_SHARED_BATCH
            | KIND_GUEST_BATCH
            | KIND_GUEST_SHARED_BATCH
    )
}
