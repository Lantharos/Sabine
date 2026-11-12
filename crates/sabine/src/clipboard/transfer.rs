//! Files a window copies or drags, handed to apps in a sandbox through the
//! document portal's file transfer, which toolkits look for next to
//! `text/uri-list`. A key is registered only when a receiver asks for one,
//! and each key works once, so nothing lingers. Apps outside a sandbox get
//! the files' own paths back for it, or keep reading `text/uri-list`.

use std::collections::HashMap;
use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use tokio::sync::OnceCell;
use zbus::Variant;
use zbus::{Connection, Fd, Proxy};

/// The type toolkits read the key from, and the name GTK 4.6 used for it.
pub(crate) const TYPES: [&str; 2] = [
    "application/vnd.portal.filetransfer",
    "application/vnd.portal.files",
];

const DOCUMENTS: &str = "org.freedesktop.portal.Documents";
const DOCUMENTS_PATH: &str = "/org/freedesktop/portal/documents";
const FILE_TRANSFER: &str = "org.freedesktop.portal.FileTransfer";

pub(crate) fn carries(mime: &str) -> bool {
    TYPES.contains(&mime)
}

/// The files among `text/uri-list` lines.
pub(crate) fn files_in(uri_list: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(uri_list)
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| url::Url::parse(line).ok()?.to_file_path().ok())
        .collect()
}

fn writable(path: &Path) -> bool {
    CString::new(path.as_os_str().as_bytes())
        .is_ok_and(|path| unsafe { libc::access(path.as_ptr(), libc::W_OK) } == 0)
}

fn handle(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_CLOEXEC)
        .open(path)
}

/// The window's own connection to the portal, which keeps every transfer
/// it starts until a receiver takes it: the portal drops a sender's
/// transfers when its connection closes.
async fn portal() -> Option<&'static Proxy<'static>> {
    static PORTAL: OnceCell<Option<Proxy<'static>>> = OnceCell::const_new();
    PORTAL
        .get_or_init(|| async {
            let connection = Connection::session().await.ok()?;
            Proxy::new(&connection, DOCUMENTS, DOCUMENTS_PATH, FILE_TRANSFER)
                .await
                .ok()
        })
        .await
        .as_ref()
}

async fn start(paths: &[PathBuf]) -> Option<String> {
    let portal = portal().await?;
    let handles = paths
        .iter()
        .map(|path| handle(path))
        .collect::<std::io::Result<Vec<_>>>()
        .ok()?;
    let options = HashMap::from([
        (
            "writable",
            Variant::from(paths.iter().all(|path| writable(path))),
        ),
        ("autostop", Variant::from(true)),
    ]);
    let key: String = portal.call("StartTransfer", &(options,)).await.ok()?;
    let fds = handles.iter().map(Fd::from).collect::<Vec<_>>();
    let added: zbus::Result<()> = portal
        .call("AddFiles", &(&key, fds, HashMap::<&str, Variant>::new()))
        .await;
    if added.is_err() {
        let _: zbus::Result<()> = portal.call("StopTransfer", &(&key,)).await;
        return None;
    }
    Some(key)
}

/// A key for `paths`, which the receiver trades with the portal for them.
/// It waits on the portal, so it never runs on a window's thread.
pub(crate) fn key(paths: &[PathBuf]) -> Option<Vec<u8>> {
    if paths.is_empty() {
        return None;
    }
    sabine_platform::async_runtime()
        .block_on(start(paths))
        .map(String::into_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_read_from_their_uris() {
        let list = b"# copied\r\nfile:///home/mara/Trip%20notes.md\r\nhttps://example.com/\r\nfile:///tmp/a\r\n";
        assert_eq!(
            files_in(list),
            [
                PathBuf::from("/home/mara/Trip notes.md"),
                PathBuf::from("/tmp/a")
            ]
        );
    }
}
