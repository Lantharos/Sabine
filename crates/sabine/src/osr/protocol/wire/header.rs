use crate::osr::transport::IpcStream;
use std::io::{self, Read};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use super::HEADER_LEN;

/// Room for more descriptors than any message carries, so extra ones arrive
/// to be closed rather than truncate the message.
#[cfg(unix)]
const CONTROL_LEN: usize = 128;

#[cfg(unix)]
#[repr(C, align(8))]
struct Control([u8; CONTROL_LEN]);

#[cfg(unix)]
pub(super) fn read_header(
    reader: &mut IpcStream,
) -> io::Result<Option<([u8; HEADER_LEN], Option<OwnedFd>)>> {
    let mut header = [0_u8; HEADER_LEN];
    let Some((read, fd)) = recv_header_start(reader, &mut header)? else {
        return Ok(None);
    };
    match reader.read_exact(&mut header[read.min(HEADER_LEN)..]) {
        Ok(()) => Ok(Some((header, fd))),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
pub(super) fn read_header(reader: &mut IpcStream) -> io::Result<Option<[u8; HEADER_LEN]>> {
    let mut header = [0_u8; HEADER_LEN];
    match reader.read_exact(&mut header) {
        Ok(()) => Ok(Some(header)),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn recv_header_start(
    reader: &IpcStream,
    header: &mut [u8; HEADER_LEN],
) -> io::Result<Option<(usize, Option<OwnedFd>)>> {
    let mut iov = libc::iovec {
        iov_base: header.as_mut_ptr().cast(),
        iov_len: HEADER_LEN,
    };
    let mut control = Control([0; CONTROL_LEN]);
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.0.as_mut_ptr().cast();
    message.msg_controllen = CONTROL_LEN as _;
    #[cfg(target_os = "linux")]
    let flags = libc::MSG_CMSG_CLOEXEC;
    #[cfg(not(target_os = "linux"))]
    let flags = 0;
    let result = unsafe { libc::recvmsg(reader.as_raw_fd(), &mut message, flags) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut fds = unsafe { received_fds(&message) }.into_iter();
    if message.msg_flags & libc::MSG_CTRUNC != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "an OSR message carried more descriptors than it can hold",
        ));
    }
    if result == 0 {
        return Ok(None);
    }
    Ok(Some((result as usize, fds.next())))
}

/// Takes ownership of every descriptor `message` carried.
#[cfg(unix)]
unsafe fn received_fds(message: &libc::msghdr) -> Vec<OwnedFd> {
    let mut fds = Vec::new();
    let mut control = unsafe { libc::CMSG_FIRSTHDR(message) };
    while !control.is_null() {
        let header = unsafe { &*control };
        if header.cmsg_level == libc::SOL_SOCKET && header.cmsg_type == libc::SCM_RIGHTS {
            let data = unsafe { libc::CMSG_DATA(control) }.cast::<i32>();
            let data_len = header.cmsg_len as usize - unsafe { libc::CMSG_LEN(0) } as usize;
            for index in 0..data_len / size_of::<i32>() {
                let fd = unsafe { data.add(index).read_unaligned() };
                #[cfg(not(target_os = "linux"))]
                unsafe {
                    libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                }
                fds.push(unsafe { OwnedFd::from_raw_fd(fd) });
            }
        }
        control = unsafe { libc::CMSG_NXTHDR(message, control) };
    }
    fds
}

pub(super) fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("slice length checked"))
}

pub(super) fn read_i32(bytes: &[u8]) -> i32 {
    i32::from_le_bytes(bytes.try_into().expect("slice length checked"))
}

pub(super) fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("slice length checked"))
}
