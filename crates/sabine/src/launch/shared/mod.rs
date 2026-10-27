mod unit;

use std::{
    ffi::{OsStr, OsString},
    io::{BufRead, BufReader, Write},
    os::unix::{fs::DirBuilderExt, net::UnixStream},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use unit::RuntimeUnit;

use crate::launch::browser::BrowserOptions;

const PROTOCOL: &str = "SABINE-RUNTIME 1";
const SOCKET_DIRECTORY: &str = "sabine-runtime";
const OPEN_DEADLINE: Duration = Duration::from_secs(10);
const ANSWER_TIMEOUT: Duration = Duration::from_secs(20);
const RETRY_DELAY: Duration = Duration::from_millis(100);
const SOCKET_DELAY: Duration = Duration::from_millis(5);

/// A window's browser in a shared runtime: the runtime that serves apps with
/// this app's Chromium flags, or this app alone when it isn't installed for
/// every user, and the window's own switches.
pub(crate) struct SharedLaunch {
    unit: RuntimeUnit,
    request: Vec<String>,
}

enum Attempt {
    Unreachable,
    Dropped,
    Refused(String),
}

impl SharedLaunch {
    /// Describes the request, or returns `None` when the window's switches
    /// cannot travel to the runtime.
    pub(crate) fn new(
        host: &Path,
        runtime_dir: &Path,
        binary_dir: &Path,
        options: &BrowserOptions,
        app_id: &str,
        window: &[OsString],
    ) -> Option<Self> {
        let mut request = vec![format!("--sabine-app-id={app_id}")];
        for switch in window {
            let switch = switch.to_str().filter(|switch| !switch.contains('\n'))?;
            request.push(switch.to_string());
        }
        let socket_directory =
            PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join(SOCKET_DIRECTORY);
        let profiles = sabine_service::browser_profiles_root();
        let user_data = sabine_service::shared_browser_profiles_root();
        let mut global = Command::new(host);
        sabine_host::apply_runtime_resource_args(&mut global, runtime_dir);
        crate::apply_browser_launch_args(&mut global, options, false);
        global
            .arg("--sabine-runtime")
            .arg(format!("--root-cache-path={}", user_data.display()))
            .arg(format!("--sabine-profiles={}", profiles.display()))
            .arg(format!(
                "--sabine-data-dir={}",
                sabine_runtime::sabine_data_dir().display()
            ));
        let mut arguments = global
            .get_args()
            .map(OsStr::to_os_string)
            .collect::<Vec<_>>();
        let mut environment = vec![
            format!("LD_LIBRARY_PATH={}", binary_dir.display()),
            format!("AMD_DEBUG={}", crate::launch::browser::chromium_amd_debug()),
        ];
        if let Ok(display) = std::env::var("WAYLAND_DISPLAY") {
            environment.push(format!("WAYLAND_DISPLAY={display}"));
        }
        let expandable = |text: &OsStr| {
            text.as_encoded_bytes()
                .iter()
                .any(|byte| matches!(byte, b'$' | b'%'))
        };
        if arguments.iter().any(|argument| expandable(argument))
            || environment
                .iter()
                .any(|entry| expandable(OsStr::new(entry)))
            || expandable(host.as_os_str())
        {
            return None;
        }
        let owner = (!installed_for_every_user(app_id)).then_some(app_id);
        let key = instance_key(host, &arguments, &environment, binary_dir, owner);
        if runtime_dir.starts_with(sabine_runtime::user_runtime_path()) {
            let leases = runtime_dir.join(".leases");
            std::fs::create_dir_all(&leases).ok()?;
            let lease = leases.join(format!("runtime-{key}.lease"));
            arguments.push(format!("--sabine-runtime-lease={}", lease.display()).into());
        }
        Some(Self {
            unit: RuntimeUnit {
                socket: socket_directory.join(format!("{key}.sock")),
                key,
                program: host.to_path_buf(),
                arguments,
                environment,
                directory: binary_dir.to_path_buf(),
            },
            request,
        })
    }

    /// Asks the shared runtime for the window's browser, starting the
    /// runtime when none is running. A runtime that is quitting drops the
    /// request, and systemd starts it again for the next one.
    pub(crate) fn open(&self) -> Result<(), String> {
        if let Some(directory) = self.unit.socket.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(directory)
                .map_err(|error| error.to_string())?;
        }
        let deadline = Instant::now() + OPEN_DEADLINE;
        loop {
            match self.ask() {
                Ok(()) => return Ok(()),
                Err(Attempt::Refused(reason)) => return Err(reason),
                Err(_) if Instant::now() >= deadline => {
                    return Err("the shared runtime did not answer".to_string());
                }
                Err(Attempt::Unreachable) => {
                    sabine_platform::async_runtime().block_on(self.unit.start())?;
                    std::thread::sleep(SOCKET_DELAY);
                }
                Err(Attempt::Dropped) => std::thread::sleep(RETRY_DELAY),
            }
        }
    }

    fn ask(&self) -> Result<(), Attempt> {
        let mut stream =
            UnixStream::connect(&self.unit.socket).map_err(|_| Attempt::Unreachable)?;
        stream
            .set_read_timeout(Some(ANSWER_TIMEOUT))
            .map_err(|_| Attempt::Dropped)?;
        let mut request = format!("{PROTOCOL}\n");
        for switch in &self.request {
            request.push_str(switch);
            request.push('\n');
        }
        request.push('\n');
        stream
            .write_all(request.as_bytes())
            .map_err(|_| Attempt::Dropped)?;
        let mut answer = String::new();
        BufReader::new(stream)
            .read_line(&mut answer)
            .map_err(|_| Attempt::Dropped)?;
        let answer = answer.trim_end();
        if answer == "ok" {
            return Ok(());
        }
        match answer.split_once('\t') {
            Some(("error", reason)) => Err(Attempt::Refused(reason.to_string())),
            _ => Err(Attempt::Dropped),
        }
    }
}

/// Whether the app is installed for every user, by an administrator or the
/// system image. Only those apps share a runtime with each other; every other
/// app gets a runtime of its own.
fn installed_for_every_user(app_id: &str) -> bool {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .is_ok_and(|executable| executable.starts_with(sabine_runtime::system_dir().join(app_id)))
}

fn instance_key(
    host: &Path,
    arguments: &[OsString],
    environment: &[String],
    directory: &Path,
    owner: Option<&str>,
) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    let parts = std::iter::once(host.as_os_str())
        .chain(arguments.iter().map(OsString::as_os_str))
        .chain(environment.iter().map(OsStr::new))
        .chain(std::iter::once(directory.as_os_str()))
        .chain(owner.map(OsStr::new));
    for part in parts {
        for byte in part.as_encoded_bytes().iter().chain(&[0xff]) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}
