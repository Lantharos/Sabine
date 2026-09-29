use std::{
    fs::File,
    io::{self, Read, Write},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    sync::Arc,
    thread,
    time::Instant,
};

pub(crate) fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

pub(crate) fn wait_readable(fd: &impl AsRawFd, deadline: Instant) -> io::Result<bool> {
    let timeout = deadline.saturating_duration_since(Instant::now());
    let mut poll = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let ready = unsafe {
        libc::poll(
            &mut poll,
            1,
            timeout.as_millis().min(i32::MAX as u128) as i32,
        )
    };
    match ready {
        -1 => Err(io::Error::last_os_error()),
        0 => Ok(false),
        _ => Ok(true),
    }
}

/// Reads a non-blocking pipe to its end, giving up at `deadline`.
pub(crate) fn read_to_end(fd: OwnedFd, deadline: Instant) -> io::Result<Vec<u8>> {
    let mut file = File::from(fd);
    let mut bytes = Vec::new();
    let mut chunk = [0; 64 * 1024];
    loop {
        match file.read(&mut chunk) {
            Ok(0) => return Ok(bytes),
            Ok(read) => bytes.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if !wait_readable(&file, deadline)? {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "the application holding the clipboard did not answer",
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

/// Hands `bytes` to a reader on another thread, so a slow reader never
/// holds up the clipboard.
pub(crate) fn send(fd: OwnedFd, bytes: Arc<[u8]>) {
    thread::spawn(move || {
        let mut file = File::from(fd);
        unsafe {
            let flags = libc::fcntl(file.as_raw_fd(), libc::F_GETFL);
            libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }
        let _ = file.write_all(&bytes);
    });
}
