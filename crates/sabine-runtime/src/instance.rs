//! Where a running app's single instance listens, and how the app's newly
//! installed copy asks it to step down: a launch of the new copy that will
//! take the instance over, or the installer that put the copy there.

use std::{
    env,
    io::{self, Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

/// Marks a launch that takes the instance over once it steps down.
pub const SUCCESSOR: &str = "successor";
/// Tells the instance that a newer copy of its app was installed.
pub const UPDATED: &str = "updated";
/// What the instance answers when it steps down.
pub const STEPPING_DOWN: &[u8] = b"stepping down";

/// The socket the instance named `id` listens on. Its lock file sits beside
/// it as `<id>.lock`, held for as long as the instance runs.
pub fn instance_socket(id: &str) -> io::Result<PathBuf> {
    Ok(instance_directory()?.join(format!("{id}.sock")))
}

#[cfg(target_os = "linux")]
pub fn instance_directory() -> io::Result<PathBuf> {
    Ok(env::var_os("XDG_RUNTIME_DIR")
        .map(|path| PathBuf::from(path).join("sabine"))
        .unwrap_or_else(|| env::temp_dir().join(format!("sabine-{}", unsafe { libc::getuid() }))))
}

#[cfg(target_os = "macos")]
pub fn instance_directory() -> io::Result<PathBuf> {
    let home = env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HOME is required for macOS desktop integration",
        )
    })?;
    Ok(PathBuf::from(home).join("Library/Caches/sabine"))
}

/// Hands `message` to the instance on `stream` and reports whether it
/// answered that it steps down. An instance that only takes the message, as
/// earlier versions do, closes without answering.
pub fn deliver(mut stream: UnixStream, message: &[u8], timeout: Duration) -> io::Result<bool> {
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(message)?;
    stream.shutdown(Shutdown::Write)?;
    stream.set_read_timeout(Some(timeout))?;
    let mut answer = Vec::new();
    let _ = stream
        .take(STEPPING_DOWN.len() as u64)
        .read_to_end(&mut answer);
    Ok(answer == STEPPING_DOWN)
}
