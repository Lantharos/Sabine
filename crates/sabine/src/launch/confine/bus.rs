//! The session bus a confined app reaches: `xdg-dbus-proxy` in front of the
//! person's bus, passing on only the names and objects the app declares in
//! `talk` and the names Sabine itself talks to, so the app can't ask the
//! service manager or anything else on the bus to run a program outside its
//! confinement.

use super::accessibility;
use std::{
    fs::File,
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const PROXY: &str = "xdg-dbus-proxy";
const ADDRESS: &str = "DBUS_SESSION_BUS_ADDRESS";
const DEFAULT_BUS: &str = "bus";
/// An address no bus answers on, for an app whose bus can't be filtered.
const NO_BUS: &str = "unix:path=/dev/null";
/// The desktop portal and document portal, desktop notifications, the
/// tray's watcher and GNOME's idle monitor.
const SABINE_NAMES: [&str; 5] = [
    "org.freedesktop.portal.Desktop",
    "org.freedesktop.portal.Documents",
    "org.freedesktop.Notifications",
    "org.kde.StatusNotifierWatcher",
    "org.gnome.Mutter.IdleMonitor",
];

/// The proxy's rules for a `talk` entry: the whole name, or only calls to and
/// signals from one of its objects.
fn rules(entry: &str) -> Vec<String> {
    match sabine_service::talk_target(entry) {
        (name, None) => vec![format!("--talk={name}")],
        (name, Some(path)) => vec![
            format!("--call={name}=*@{path}"),
            format!("--broadcast={name}=*@{path}"),
        ],
    }
}

/// Keeps the app's filtered bus running: the proxy ends, and removes its
/// socket, once the app closes its end of the pipe they share.
pub(crate) struct SessionBus {
    _running: File,
}

/// Starts the app's filtered session and accessibility buses in its own
/// runtime folder and points the app and everything it starts at them.
/// Without `xdg-dbus-proxy`, the app and its children reach neither.
pub(super) fn filter(talk: &[String], folder: &Path) -> Option<SessionBus> {
    let (bus, address) = match start(talk, folder) {
        Ok((bus, socket, accessibility)) => {
            if let Some(accessibility) = accessibility {
                accessibility::point_at(&accessibility);
            }
            (Some(bus), format!("unix:path={}", socket.display()))
        }
        Err(error) => {
            sabine_runtime::report_error(
                "sandbox",
                format!("the app gets no session bus, since it can't be filtered: {error}"),
            );
            (None, NO_BUS.to_string())
        }
    };
    // SAFETY: the threads running before the app starts its windows read the
    // environment through the standard library, which serializes with this.
    unsafe { std::env::set_var(ADDRESS, address) };
    bus
}

fn start(talk: &[String], folder: &Path) -> io::Result<(SessionBus, PathBuf, Option<PathBuf>)> {
    let upstream = std::env::var_os(ADDRESS)
        .filter(|address| !address.is_empty())
        .or_else(|| {
            let bus = dirs::runtime_dir()?.join(DEFAULT_BUS);
            Some(format!("unix:path={}", bus.display()).into())
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "there is no session bus"))?;
    let socket = folder.join(format!("bus-{}", std::process::id()));
    let accessibility_socket = folder.join(format!("a11y-bus-{}", std::process::id()));
    for socket in [&socket, &accessibility_socket] {
        let _ = std::fs::remove_file(socket);
    }
    let accessibility = accessibility::filtered(&accessibility_socket);
    let (running, ready) = pipe()?;
    let ready_fd = ready.as_raw_fd();
    let mut command = Command::new(PROXY);
    command
        .arg(format!("--fd={ready_fd}"))
        .arg(upstream)
        .arg(&socket)
        .arg("--filter")
        .args(SABINE_NAMES.iter().map(|name| format!("--talk={name}")))
        .args(talk.iter().flat_map(|entry| rules(entry)))
        .args(accessibility::STATUS_RULES)
        .args(accessibility.iter().flatten())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .process_group(0);
    unsafe {
        command.pre_exec(move || inherit(ready_fd));
    }
    let mut proxy = command.spawn()?;
    drop(ready);
    let mut running = File::from(running);
    let mut answer = [0_u8; 1];
    if running.read(&mut answer)? == 0 {
        let status = proxy.wait()?;
        return Err(io::Error::other(format!("{PROXY} ended: {status}")));
    }
    std::thread::spawn(move || proxy.wait());
    Ok((
        SessionBus { _running: running },
        socket,
        accessibility.map(|_| accessibility_socket),
    ))
}

/// A pipe whose read end the app keeps and whose write end the proxy gets,
/// to say it's ready and to notice when the app ends.
fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut ends = [0 as RawFd; 2];
    if unsafe { libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(ends[0]), OwnedFd::from_raw_fd(ends[1])) })
}

fn inherit(fd: RawFd) -> io::Result<()> {
    if unsafe { libc::fcntl(fd, libc::F_SETFD, 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
