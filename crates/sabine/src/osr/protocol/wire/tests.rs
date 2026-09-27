use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use super::{
    HEADER_LEN, KIND_HOST_HELLO, KIND_IME_CURSOR_AREA_CHANGED, KIND_IME_STATE_CHANGED,
    KIND_IME_SURROUNDING_CHANGED, KIND_MAIN_BATCH, KIND_MAIN_LOAD_READY, KIND_MAIN_SHARED_BATCH,
    KIND_TOOLTIP_CHANGED, MAGIC, PaintSlots, WireReader,
};
use crate::osr::control::ControlWriter;
use crate::osr::protocol::{OsrMessage, OsrPaintBatch, OsrSurface};

struct Connection {
    wire: WireReader,
    producer: UnixStream,
    controls: UnixStream,
}

fn connect() -> Connection {
    let (reader, producer) = UnixStream::pair().expect("paint socket pair");
    let (control, controls) = UnixStream::pair().expect("control socket pair");
    let control = Arc::new(ControlWriter::start(control).expect("control writer"));
    Connection {
        wire: WireReader::new(
            reader,
            PaintSlots::new(control),
            #[cfg(target_os = "macos")]
            Default::default(),
        ),
        producer,
        controls,
    }
}

fn header(kind: u32, size: (u32, u32), origin: (i32, i32), payload_len: usize) -> [u8; HEADER_LEN] {
    let mut header = [0_u8; HEADER_LEN];
    header[0..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&kind.to_le_bytes());
    header[8..12].copy_from_slice(&size.0.to_le_bytes());
    header[12..16].copy_from_slice(&size.1.to_le_bytes());
    header[16..20].copy_from_slice(&origin.0.to_le_bytes());
    header[20..24].copy_from_slice(&origin.1.to_le_bytes());
    header[24..28].copy_from_slice(&(payload_len as u32).to_le_bytes());
    header
}

fn paint_entries(rects: &[(i32, i32, u32, u32, u64)]) -> Vec<u8> {
    let mut payload = (rects.len() as u32).to_le_bytes().to_vec();
    for (x, y, width, height, offset) in rects {
        payload.extend_from_slice(&x.to_le_bytes());
        payload.extend_from_slice(&y.to_le_bytes());
        payload.extend_from_slice(&width.to_le_bytes());
        payload.extend_from_slice(&height.to_le_bytes());
        payload.extend_from_slice(&offset.to_le_bytes());
        payload.extend_from_slice(&(width * height * 4).to_le_bytes());
    }
    payload
}

fn send(
    connection: &mut Connection,
    kind: u32,
    size: (u32, u32),
    origin: (i32, i32),
    payload: &[u8],
) {
    connection
        .producer
        .write_all(&header(kind, size, origin, payload.len()))
        .expect("header");
    connection.producer.write_all(payload).expect("payload");
}

fn send_with_fd(connection: &Connection, header: &[u8], fd: i32) {
    let mut iov = libc::iovec {
        iov_base: header.as_ptr().cast_mut().cast(),
        iov_len: header.len(),
    };
    let mut control = [0_u64; 4];
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = unsafe { libc::CMSG_SPACE(size_of::<i32>() as u32) } as _;
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&message);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(size_of::<i32>() as u32) as _;
        libc::CMSG_DATA(cmsg).cast::<i32>().write(fd);
        assert_eq!(
            libc::sendmsg(connection.producer.as_raw_fd(), &message, 0),
            header.len() as isize
        );
    }
}

fn paint(connection: &mut Connection) -> OsrPaintBatch {
    match connection.wire.read().expect("read").expect("message") {
        OsrMessage::PaintBatch(batch) => batch,
        other => panic!("expected paint batch, got {other:?}"),
    }
}

#[test]
fn host_protocol_must_be_announced_first() {
    let mut announced = connect();
    send(&mut announced, KIND_HOST_HELLO, (0, 0), (0, 0), b"4");
    assert_eq!(
        announced
            .wire
            .read_host_protocol()
            .expect("hello")
            .as_deref(),
        Some("4")
    );

    let mut silent = connect();
    send(&mut silent, KIND_MAIN_LOAD_READY, (0, 0), (0, 0), &[]);
    assert!(silent.wire.read_host_protocol().is_err());
}

#[test]
fn inline_paint_batch_parses_multiple_rects() {
    let mut connection = connect();
    let mut payload = paint_entries(&[(0, 0, 1, 1, 0), (2, 1, 1, 1, 4)]);
    payload.extend_from_slice(&[1, 1, 1, 255, 2, 2, 2, 255]);
    send(&mut connection, KIND_MAIN_BATCH, (3, 2), (0, 0), &payload);

    let batch = paint(&mut connection);
    assert_eq!(batch.surface, OsrSurface::Main);
    assert_eq!((batch.width, batch.height), (3, 2));
    assert_eq!(batch.rects.len(), 2);
    assert_eq!((batch.rects[1].x, batch.rects[1].y), (2, 1));
    assert_eq!(batch.rects[1].bytes(), &[2, 2, 2, 255]);
}

#[test]
fn shared_slot_is_mapped_once_and_released_after_use() {
    let mut connection = connect();
    let path = std::env::temp_dir().join(format!("sabine-wire-slot-{}", std::process::id()));
    std::fs::write(&path, [7, 7, 7, 255, 9, 9, 9, 255]).expect("slot contents");
    let slot = std::fs::File::open(&path).expect("slot file");
    std::fs::remove_file(&path).expect("unlink slot");

    let mut metadata = Vec::new();
    metadata.extend_from_slice(&3_u32.to_le_bytes());
    metadata.extend_from_slice(&1_u32.to_le_bytes());
    metadata.extend(paint_entries(&[(0, 0, 1, 1, 0)]));
    send_with_fd(
        &connection,
        &header(KIND_MAIN_SHARED_BATCH, (2, 1), (0, 0), metadata.len()),
        slot.as_raw_fd(),
    );
    connection.producer.write_all(&metadata).expect("metadata");
    let first = paint(&mut connection);
    assert_eq!(first.rects[0].bytes(), &[7, 7, 7, 255]);
    drop(first);

    let mut metadata = Vec::new();
    metadata.extend_from_slice(&3_u32.to_le_bytes());
    metadata.extend_from_slice(&1_u32.to_le_bytes());
    metadata.extend(paint_entries(&[(1, 0, 1, 1, 4)]));
    send(
        &mut connection,
        KIND_MAIN_SHARED_BATCH,
        (2, 1),
        (0, 0),
        &metadata,
    );
    let second = paint(&mut connection);
    assert_eq!(second.rects[0].bytes(), &[9, 9, 9, 255]);
    drop(second);

    let expected = "paint_release\t3\t1\npaint_release\t3\t1\n";
    let mut released = vec![0_u8; expected.len()];
    connection
        .controls
        .read_exact(&mut released)
        .expect("release lines");
    assert_eq!(String::from_utf8(released).unwrap(), expected);
}

#[test]
fn shared_batch_for_unknown_slot_generation_is_rejected() {
    let mut connection = connect();
    let mut metadata = Vec::new();
    metadata.extend_from_slice(&0_u32.to_le_bytes());
    metadata.extend_from_slice(&2_u32.to_le_bytes());
    metadata.extend(paint_entries(&[(0, 0, 1, 1, 0)]));
    send(
        &mut connection,
        KIND_MAIN_SHARED_BATCH,
        (1, 1),
        (0, 0),
        &metadata,
    );

    let error = connection.wire.read().expect_err("slot must be announced");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn oversized_surface_is_rejected_before_payload_allocation() {
    let mut connection = connect();
    connection
        .producer
        .write_all(&header(KIND_MAIN_BATCH, (16_384, 16_384), (0, 0), 0))
        .expect("header");

    let error = connection
        .wire
        .read()
        .expect_err("surface must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn state_messages_decode_header_fields() {
    let mut connection = connect();
    send(&mut connection, KIND_MAIN_LOAD_READY, (0, 0), (0, 0), &[]);
    send(&mut connection, KIND_IME_STATE_CHANGED, (5, 0), (0, 0), &[]);
    send(
        &mut connection,
        KIND_IME_CURSOR_AREA_CHANGED,
        (2, 24),
        (120, 64),
        &[],
    );

    assert!(matches!(
        connection.wire.read().expect("read"),
        Some(OsrMessage::MainLoadReady)
    ));
    assert!(matches!(
        connection.wire.read().expect("read"),
        Some(OsrMessage::ImeStateChanged(5))
    ));
    assert!(matches!(
        connection.wire.read().expect("read"),
        Some(OsrMessage::ImeCursorAreaChanged {
            x: 120,
            y: 64,
            width: 2,
            height: 24,
        })
    ));
}

#[test]
fn tooltip_and_ime_surrounding_payloads_decode() {
    let mut connection = connect();
    send(
        &mut connection,
        KIND_TOOLTIP_CHANGED,
        (0, 0),
        (0, 0),
        b"Save",
    );
    send(
        &mut connection,
        KIND_IME_SURROUNDING_CHANGED,
        (0, 0),
        (0, 0),
        br#"{"text":"a duck","cursor":6,"anchor":2,"base":11}"#,
    );

    assert!(matches!(
        connection.wire.read().expect("read"),
        Some(OsrMessage::TooltipChanged(text)) if text == "Save"
    ));
    assert!(matches!(
        connection.wire.read().expect("read"),
        Some(OsrMessage::ImeSurroundingChanged {
            text,
            cursor_utf16: 6,
            anchor_utf16: 2,
            base_utf16: 11,
        }) if text == "a duck"
    ));
}
