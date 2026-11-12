//! Brings what a person hands a confined app, by dropping it on a window,
//! opening it with the app or picking it in a file dialog, within the app's
//! reach through the document portal, as the desktop portal does itself for
//! an app in a sandbox. The portal shows each file below its mount, which the
//! app reaches, while the rest of the person's files stay out of reach.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use zbus::{Connection, Fd, OwnedVariant, Proxy};

const DOCUMENTS: &str = "org.freedesktop.portal.Documents";
const DOCUMENTS_PATH: &str = "/org/freedesktop/portal/documents";
const ROOT: &str = "/";
const REUSE_EXISTING: u32 = 1;
const FOLDER: u32 = 8;

/// What a person handed the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handed {
    /// A file or folder that exists, which the app opens.
    Existing,
    /// A file the app is to create, such as one picked to save as.
    New,
}

fn denied<T>(opened: &std::io::Result<T>) -> bool {
    matches!(opened, Err(error) if error.kind() == ErrorKind::PermissionDenied)
}

/// Whether Sabine holds this process to its app's `[sandbox]` declaration,
/// under which not even the root folder opens. An app in an app sandbox gets
/// what it's handed from the desktop portal already.
pub fn confined() -> bool {
    !crate::launch::sandbox::in_app_sandbox() && denied(&File::open(ROOT))
}

/// Whether the app can't use what it was handed by itself. A folder always
/// goes through the portal, since nothing tells whether the app may change it
/// without changing it.
fn unreachable(path: &Path, handed: Handed) -> bool {
    match handed {
        Handed::Existing => path.is_dir() || denied(&File::open(path)),
        Handed::New => denied(
            &OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(path),
        ),
    }
}

fn handle(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))
}

struct Portal {
    proxy: Proxy<'static>,
    mount: PathBuf,
}

impl Portal {
    async fn open() -> Result<Self, String> {
        let connection = Connection::session().await.map_err(failed)?;
        let proxy = Proxy::new(&connection, DOCUMENTS, DOCUMENTS_PATH, DOCUMENTS)
            .await
            .map_err(failed)?;
        let mount: Vec<u8> = proxy.call("GetMountPoint", &()).await.map_err(failed)?;
        let mount = mount.strip_suffix(&[0]).unwrap_or(&mount);
        Ok(Self {
            proxy,
            mount: PathBuf::from(OsStr::from_bytes(mount)),
        })
    }

    async fn share(&self, path: &Path, handed: Handed) -> Result<PathBuf, String> {
        let name = path.file_name().ok_or("The file has no name")?;
        let id: String = match handed {
            Handed::New => {
                let folder = handle(path.parent().ok_or("The file has no folder")?)?;
                let mut name = name.as_bytes().to_vec();
                name.push(0);
                self.proxy
                    .call("AddNamed", &(Fd::from(&folder), name, true, false))
                    .await
                    .map_err(failed)?
            }
            Handed::Existing => {
                let shared = handle(path)?;
                let flags = if path.is_dir() {
                    REUSE_EXISTING | FOLDER
                } else {
                    REUSE_EXISTING
                };
                let (ids, _): (Vec<String>, HashMap<String, OwnedVariant>) = self
                    .proxy
                    .call(
                        "AddFull",
                        &(vec![Fd::from(&shared)], flags, "", Vec::<&str>::new()),
                    )
                    .await
                    .map_err(failed)?;
                ids.into_iter()
                    .next()
                    .ok_or("The document portal shared nothing")?
            }
        };
        Ok(self.mount.join(id).join(name))
    }
}

fn failed(error: zbus::Error) -> String {
    error.to_string()
}

/// The `paths` a person handed the app, with every one a confined app can't
/// use by itself replaced by where the document portal shows it. Paths the
/// app can already use stay as they are, and outside its confinement every
/// path does.
pub async fn within_reach(paths: Vec<PathBuf>, handed: Handed) -> Result<Vec<PathBuf>, String> {
    if paths.is_empty() || !confined() {
        return Ok(paths);
    }
    let mut portal = None;
    let mut reachable = Vec::with_capacity(paths.len());
    for path in paths {
        if !unreachable(&path, handed) {
            reachable.push(path);
            continue;
        }
        let portal = match &portal {
            Some(portal) => portal,
            None => portal.insert(Portal::open().await?),
        };
        reachable.push(portal.share(&path, handed).await?);
    }
    Ok(reachable)
}

/// The `urls` the app is opened with, with the files among them brought
/// within reach as [`within_reach`] does.
pub(crate) async fn urls_within_reach(urls: Vec<String>) -> Result<Vec<String>, String> {
    let paths = urls.iter().map(|url| file_path(url)).collect::<Vec<_>>();
    let files = paths.iter().flatten().cloned().collect();
    let mut reached = within_reach(files, Handed::Existing).await?.into_iter();
    Ok(urls
        .into_iter()
        .zip(paths)
        .map(|(url, path)| {
            path.and_then(|_| reached.next())
                .and_then(|path| url::Url::from_file_path(path).ok())
                .map_or(url, Into::into)
        })
        .collect())
}

fn file_path(url: &str) -> Option<PathBuf> {
    url::Url::parse(url).ok()?.to_file_path().ok()
}
