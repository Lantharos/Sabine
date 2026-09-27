use crate::osr::transport::IpcStream;
use std::io::{self, Read};
#[cfg(unix)]
use std::os::fd::AsRawFd;

use super::HEADER_LEN;

#[cfg(unix)]
pub(super) fn read_header(
    reader: &mut IpcStream,
) -> io::Result<Option<([u8; HEADER_LEN], ReceivedFd)>> {
    let mut header = [0_u8; HEADER_LEN];
    let Some((read, fd)) = recv_header_start(reader, &mut header)? else {
        return Ok(None);
    };
    let fd = ReceivedFd(fd);
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
pub(super) struct ReceivedFd(Option<i32>);

#[cfg(unix)]
impl ReceivedFd {
    pub(super) fn take(&mut self) -> Option<i32> {
        self.0.take()
    }
}

#[cfg(unix)]
impl Drop for ReceivedFd {
    fn drop(&mut self) {
        if let Some(fd) = self.0.take() {
            unsafe {
                libc::close(fd);
            }
        }
    }
}

#[cfg(unix)]
fn recv_header_start(
    reader: &IpcStream,
    header: &mut [u8; HEADER_LEN],
) -> io::Result<Option<(usize, Option<i32>)>> {
    let mut iov = libc::iovec {
        iov_base: header.as_mut_ptr().cast(),
        iov_len: HEADER_LEN,
    };
    let mut control = [0_u8; 64];
    let mut message = libc::msghdr {
        msg_name: std::ptr::null_mut(),
        msg_namelen: 0,
        msg_iov: &mut iov,
        msg_iovlen: 1,
        msg_control: control.as_mut_ptr().cast(),
        msg_controllen: control.len() as _,
        msg_flags: 0,
    };
    #[cfg(target_os = "linux")]
    let flags = libc::MSG_CMSG_CLOEXEC;
    #[cfg(not(target_os = "linux"))]
    let flags = 0;
    let result = unsafe { libc::recvmsg(reader.as_raw_fd(), &mut message, flags) };
    if result == 0 {
        return Ok(None);
    }
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = unsafe { received_fd(&message) };
    Ok(Some((result as usize, fd)))
}

#[cfg(unix)]
unsafe fn received_fd(message: &libc::msghdr) -> Option<i32> {
    let mut control = unsafe { libc::CMSG_FIRSTHDR(message) };
    while !control.is_null() {
        let header = unsafe { &*control };
        if header.cmsg_level == libc::SOL_SOCKET && header.cmsg_type == libc::SCM_RIGHTS {
            return Some(unsafe { *(libc::CMSG_DATA(control).cast::<i32>()) });
        }
        control = unsafe { libc::CMSG_NXTHDR(message, control) };
    }
    None
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
