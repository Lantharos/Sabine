use std::{
    ffi::OsString,
    io::{Read, Write},
    net::Shutdown,
    os::unix::{ffi::OsStrExt, net::UnixStream},
    path::PathBuf,
    time::Duration,
};

const SOCKET: &str = "SingletonSocket";
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);
const START: &[u8] = b"START";
const ACKNOWLEDGED: &[u8] = b"ACK";

/// A window for the Chromium that already holds the app's profile, sent over
/// Chromium's process singleton the way a second Chromium of the app would
/// send it, so no second Chromium has to start.
pub(crate) struct Handover {
    pub(crate) profile: PathBuf,
    pub(crate) program: PathBuf,
    pub(crate) switches: Vec<OsString>,
}

impl Handover {
    /// Whether a running Chromium took the window.
    pub(crate) fn reach(&self) -> bool {
        let Ok(mut stream) = UnixStream::connect(self.profile.join(SOCKET)) else {
            return false;
        };
        let directory = self.program.parent().unwrap_or(&self.program);
        let mut message = START.to_vec();
        for part in [directory.as_os_str(), self.program.as_os_str()]
            .into_iter()
            .chain(self.switches.iter().map(OsString::as_os_str))
        {
            message.push(0);
            message.extend_from_slice(part.as_bytes());
        }
        let mut answer = Vec::new();
        stream.set_read_timeout(Some(ANSWER_TIMEOUT)).is_ok()
            && stream.write_all(&message).is_ok()
            && stream.shutdown(Shutdown::Write).is_ok()
            && stream
                .take(ACKNOWLEDGED.len() as u64)
                .read_to_end(&mut answer)
                .is_ok()
            && answer == ACKNOWLEDGED
    }
}
