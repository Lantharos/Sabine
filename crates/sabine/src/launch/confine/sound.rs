//! The sound server a confined app plays through when it doesn't record:
//! its own `pipewire-pulse` in front of the person's PipeWire, which refuses
//! every record stream, monitors included, whatever the app says about
//! itself. The person's own PulseAudio socket would let it record.

use std::{
    fs::File,
    io,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const SERVER: &str = "pipewire-pulse";
const SOCKET: &str = "pulse-native";
const CONFIG: &str = "pulse.conf";
const ADDRESS: &str = "PULSE_SERVER";
const NO_SERVER: &str = "unix:/dev/null";
const STARTED: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);
/// Runs the server until the app closes its end of the server's input.
const SUPERVISOR: &str = "\"$1\" -c \"$2\" & read -r _; kill $!";

/// Keeps the app's sound server running: it ends once the app closes its
/// end of the pipe they share.
pub(crate) struct PlaybackServer {
    _running: File,
}

fn config(socket: &Path) -> String {
    format!(
        r#"context.spa-libs = {{
    audio.convert.* = audioconvert/libspa-audioconvert
    support.* = support/libspa-support
}}
context.modules = [
    {{ name = libpipewire-module-protocol-native }}
    {{ name = libpipewire-module-client-node }}
    {{ name = libpipewire-module-adapter }}
    {{ name = libpipewire-module-metadata }}
    {{ name = libpipewire-module-protocol-pulse
        args = {{
            server.dbus-name = ""
            server.address = [ "unix:{}" ]
        }}
    }}
]
pulse.rules = [
    {{
        matches = [
            {{ pulse.server.type = "unix" }}
            {{ pulse.server.type = "!unix" }}
            {{ pulse.server.type = null }}
        ]
        actions = {{ quirks = [ block-record-stream ] }}
    }}
]
"#,
        socket.display()
    )
}

/// Starts the app's playback-only sound server in its own runtime folder and
/// points the app and everything it starts at it. Without `pipewire-pulse`
/// the app plays no sound.
pub(super) fn playback(folder: &Path) -> Option<PlaybackServer> {
    let (server, address) = match start(folder) {
        Ok((server, socket)) => (Some(server), format!("unix:{}", socket.display())),
        Err(error) => {
            sabine_runtime::report_error(
                "sandbox",
                format!("the app plays no sound, since it can't be kept from recording: {error}"),
            );
            (None, NO_SERVER.to_string())
        }
    };
    // SAFETY: the threads running before the app starts its windows read the
    // environment through the standard library, which serializes with this.
    unsafe { std::env::set_var(ADDRESS, address) };
    server
}

fn start(folder: &Path) -> io::Result<(PlaybackServer, PathBuf)> {
    let socket = folder.join(SOCKET);
    let config_path = folder.join(CONFIG);
    let _ = std::fs::remove_file(&socket);
    std::fs::write(&config_path, config(&socket))?;
    let (input, running) = super::bus::pipe()?;
    Command::new("sh")
        .arg("-c")
        .arg(SUPERVISOR)
        .arg("sh")
        .arg(SERVER)
        .arg(&config_path)
        .stdin(Stdio::from(input))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    let deadline = Instant::now() + STARTED;
    while !socket.exists() {
        if Instant::now() > deadline {
            return Err(io::Error::other(format!("{SERVER} didn't start")));
        }
        thread::sleep(POLL);
    }
    Ok((
        PlaybackServer {
            _running: File::from(running),
        },
        socket,
    ))
}
