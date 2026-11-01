//! A lock file and a Unix socket beside it in the user's runtime directory.

use std::{
    env,
    fs::{self, File, OpenOptions},
    io,
    os::fd::AsRawFd,
    os::unix::{
        fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
};

use serde_json::Value;

use super::Claim;

pub(super) type Stream = UnixStream;

/// Holds the instance lock and the socket other launches reach it through.
pub(super) struct Owner {
    _lock: File,
    socket: PathBuf,
}

impl Owner {
    /// Wakes the listener so it notices the instance is ending.
    pub(super) fn wake(&self) {
        let _ = UnixStream::connect(&self.socket);
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket);
    }
}

pub(super) struct Listener(UnixListener);

impl Listener {
    pub(super) fn accept(&self) -> io::Result<Stream> {
        self.0.accept().map(|(stream, _)| stream)
    }

    pub(super) fn gate(&self) -> Gate {
        Gate
    }
}

/// Admits launches by the same user.
#[derive(Clone)]
pub(super) struct Gate;

impl Gate {
    pub(super) fn admits(&self, stream: &Stream, _activation: &Value) -> bool {
        crate::osr::transport::authenticate_peer(stream).is_ok()
    }
}

pub(super) struct Peer(PathBuf);

pub(super) fn claim(id: &str) -> io::Result<Claim> {
    let directory = runtime_directory()?;
    prepare_socket_directory(&directory)?;
    let socket = directory.join(format!("{id}.sock"));
    let lock = open_lock(&socket.with_extension("lock"))?;
    if !try_lock(&lock)? {
        return Ok(Claim::Running(Peer(socket)));
    }
    remove_stale_socket(&socket)?;
    let listener = bind_listener(&socket)?;
    Ok(Claim::Owner(
        Owner {
            _lock: lock,
            socket,
        },
        Listener(listener),
    ))
}

pub(super) fn connect(peer: &Peer, activation: &mut Value) -> io::Result<Stream> {
    #[cfg(target_os = "linux")]
    if let Some(token) = startup_activation_token() {
        activation["activationToken"] = token.into();
    }
    #[cfg(target_os = "macos")]
    let _ = activation;
    UnixStream::connect(&peer.0)
}

/// The token the launcher handed this process, which lets its first window
/// take focus unless the person moved on to something else meanwhile. A
/// launch handed to the running instance passes it on, so that instance can
/// raise its window on Wayland.
#[cfg(target_os = "linux")]
pub(crate) fn startup_activation_token() -> Option<String> {
    env::var("XDG_ACTIVATION_TOKEN")
        .or_else(|_| env::var("DESKTOP_STARTUP_ID"))
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

#[cfg(target_os = "linux")]
fn runtime_directory() -> io::Result<PathBuf> {
    Ok(env::var_os("XDG_RUNTIME_DIR")
        .map(|path| PathBuf::from(path).join("sabine"))
        .unwrap_or_else(|| env::temp_dir().join(format!("sabine-{}", current_uid()))))
}

#[cfg(target_os = "macos")]
fn runtime_directory() -> io::Result<PathBuf> {
    let home = env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HOME is required for macOS desktop integration",
        )
    })?;
    Ok(PathBuf::from(home).join("Library/Caches/sabine"))
}

fn prepare_socket_directory(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "single-instance directory must not be a symlink",
        ));
    }
    fs::create_dir_all(path)?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir() || metadata.uid() != current_uid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "single-instance directory is not owned by this user",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

fn bind_listener(path: &Path) -> io::Result<UnixListener> {
    let listener = UnixListener::bind(path)?;
    if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        drop(listener);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(listener)
}

fn remove_stale_socket(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_socket() || metadata.uid() != current_uid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to replace an untrusted single-instance endpoint",
        ));
    }
    fs::remove_file(path)
}

fn open_lock(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.uid() != current_uid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "single-instance lock is not an owned regular file",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

fn try_lock(file: &File) -> io::Result<bool> {
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Ok(false)
    } else {
        Err(error)
    }
}

fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}
