//! Process groups of the app's children, which end with the app. Asked to end
//! with SIGINT or SIGTERM, the app asks every group to end too and gives
//! Chromium a few seconds to write cookies and web storage to disk before it
//! kills what is left, keeping any handler the app installed itself.

use std::{
    io,
    os::fd::{FromRawFd, OwnedFd, RawFd},
    process::Command,
    sync::{
        Once,
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const GROUP_CAPACITY: usize = 128;
const SIGNALS: [libc::c_int; 2] = [libc::SIGINT, libc::SIGTERM];
/// How long the app's children have to end by themselves once it is asked
/// to end.
const GRACE: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

static PROCESS_GROUPS: [AtomicI32; GROUP_CAPACITY] = [const { AtomicI32::new(0) }; GROUP_CAPACITY];
static WAKE: AtomicI32 = AtomicI32::new(-1);
static PREVIOUS: [Previous; SIGNALS.len()] = [const { Previous::new() }; SIGNALS.len()];

/// The handler a signal had before Sabine's, which Sabine's calls after its
/// own.
struct Previous {
    handler: AtomicUsize,
    takes_info: AtomicBool,
}

impl Previous {
    const fn new() -> Self {
        Self {
            handler: AtomicUsize::new(libc::SIG_DFL),
            takes_info: AtomicBool::new(false),
        }
    }
}

pub(super) struct ProcessGroup {
    id: u32,
    active: AtomicBool,
}

impl ProcessGroup {
    pub(super) fn register(id: u32) -> io::Result<Self> {
        install_signal_cleanup();
        if let Ok(id) = i32::try_from(id)
            && id > 0
        {
            for slot in &PROCESS_GROUPS {
                if slot
                    .compare_exchange(0, id, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    break;
                }
            }
        }
        Ok(Self {
            id,
            active: AtomicBool::new(true),
        })
    }

    pub(super) fn terminate(&self) {
        if self.active.load(Ordering::SeqCst) {
            send(self.id, libc::SIGTERM);
        }
    }

    pub(super) fn kill(&self) {
        if self.active.load(Ordering::SeqCst) {
            send(self.id, libc::SIGKILL);
        }
    }

    pub(super) fn unregister(&self) {
        if !self.active.swap(false, Ordering::SeqCst) {
            return;
        }
        let Ok(id) = i32::try_from(self.id) else {
            return;
        };
        for slot in &PROCESS_GROUPS {
            let _ = slot.compare_exchange(id, 0, Ordering::SeqCst, Ordering::SeqCst);
        }
    }
}

pub(super) fn prepare_child_command(command: &mut Command, die_with_parent: bool) {
    use std::os::unix::process::CommandExt;

    install_signal_cleanup();
    command.process_group(0);
    #[cfg(target_os = "linux")]
    if die_with_parent {
        terminate_with_parent(command);
    }
    #[cfg(target_os = "macos")]
    let _ = die_with_parent;
}

/// Window hosts must not outlive a crashed app, which they also notice when
/// the app's end of their input closes. The shared browser process is started
/// without this, so closing one window leaves it to the others. A child whose
/// parent ended before it could ask to end with it doesn't start.
#[cfg(target_os = "linux")]
fn terminate_with_parent(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    let parent = std::process::id() as libc::pid_t;
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                return Err(io::Error::from_raw_os_error(libc::ESRCH));
            }
            Ok(())
        });
    }
}

fn install_signal_cleanup() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let woken = match self_pipe() {
            Ok(woken) => woken,
            Err(error) => {
                sabine_runtime::report_error(
                    "process",
                    format!("could not watch for the app being asked to end: {error}"),
                );
                return;
            }
        };
        std::thread::Builder::new()
            .name("sabine-signals".into())
            .spawn(move || watch(woken))
            .expect("the signal watcher thread starts");
        for (index, signal) in SIGNALS.into_iter().enumerate() {
            chain(index, signal);
        }
    });
}

/// The pipe the signal handler wakes the watcher through: the handler keeps
/// the write end in [`WAKE`], the watcher reads the returned end.
fn self_pipe() -> io::Result<OwnedFd> {
    let mut ends = [0 as RawFd; 2];
    if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let [read, write] = ends;
    let woken = unsafe { OwnedFd::from_raw_fd(read) };
    let configured = unsafe {
        libc::fcntl(read, libc::F_SETFD, libc::FD_CLOEXEC) == 0
            && libc::fcntl(write, libc::F_SETFD, libc::FD_CLOEXEC) == 0
            && libc::fcntl(write, libc::F_SETFL, libc::O_NONBLOCK) == 0
    };
    if !configured {
        let error = io::Error::last_os_error();
        unsafe { libc::close(write) };
        return Err(error);
    }
    WAKE.store(write, Ordering::SeqCst);
    Ok(woken)
}

/// Puts Sabine's handler in front of the one the signal had, unless the
/// process ignores the signal, as a shell does for programs it runs in the
/// background.
fn chain(index: usize, signal: libc::c_int) {
    unsafe {
        let mut previous: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(signal, std::ptr::null(), &mut previous) != 0
            || previous.sa_sigaction == libc::SIG_IGN
        {
            return;
        }
        PREVIOUS[index]
            .handler
            .store(previous.sa_sigaction, Ordering::SeqCst);
        PREVIOUS[index]
            .takes_info
            .store(previous.sa_flags & libc::SA_SIGINFO != 0, Ordering::SeqCst);
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = handle_signal
            as extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void)
            as libc::sighandler_t;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(signal, &action, std::ptr::null_mut());
    }
}

/// The `errno` of the thread the handler interrupted, which it leaves as it
/// found it.
#[cfg(target_os = "linux")]
unsafe fn errno() -> *mut libc::c_int {
    unsafe { libc::__errno_location() }
}

#[cfg(target_os = "macos")]
unsafe fn errno() -> *mut libc::c_int {
    unsafe { libc::__error() }
}

fn slot(signal: libc::c_int) -> Option<usize> {
    SIGNALS.iter().position(|known| *known == signal)
}

extern "C" fn handle_signal(
    signal: libc::c_int,
    info: *mut libc::siginfo_t,
    context: *mut libc::c_void,
) {
    let interrupted = unsafe { *errno() };
    signal_groups(libc::SIGTERM);
    let wake = WAKE.load(Ordering::SeqCst);
    let byte = signal as u8;
    unsafe {
        libc::write(wake, (&raw const byte).cast(), 1);
        *errno() = interrupted;
    }
    let Some(previous) = slot(signal).map(|index| &PREVIOUS[index]) else {
        return;
    };
    let handler = previous.handler.load(Ordering::SeqCst);
    if handler == libc::SIG_DFL {
        return;
    }
    unsafe {
        if previous.takes_info.load(Ordering::SeqCst) {
            let handler: extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) =
                std::mem::transmute(handler);
            handler(signal, info, context);
        } else {
            let handler: extern "C" fn(libc::c_int) = std::mem::transmute(handler);
            handler(signal);
        }
    }
}

/// Waits for the signal handler, then kills the groups that haven't ended
/// within [`GRACE`]. When the app left the signal's default in place, it
/// then ends the app as that signal does.
fn watch(woken: OwnedFd) {
    use std::io::Read;

    let mut woken = std::fs::File::from(woken);
    let mut signal = [0_u8; 1];
    while woken.read_exact(&mut signal).is_ok() {
        let signal = libc::c_int::from(signal[0]);
        let deadline = Instant::now() + GRACE;
        while groups_alive() && Instant::now() < deadline {
            std::thread::sleep(POLL_INTERVAL);
        }
        signal_groups(libc::SIGKILL);
        let ends_app = slot(signal)
            .is_some_and(|index| PREVIOUS[index].handler.load(Ordering::SeqCst) == libc::SIG_DFL);
        if ends_app {
            unsafe {
                libc::signal(signal, libc::SIG_DFL);
                libc::raise(signal);
            }
        }
    }
}

fn groups_alive() -> bool {
    PROCESS_GROUPS.iter().any(|slot| {
        let id = slot.load(Ordering::SeqCst);
        id > 0 && unsafe { libc::kill(-id, 0) } == 0
    })
}

fn signal_groups(signal: libc::c_int) {
    for slot in &PROCESS_GROUPS {
        let id = slot.load(Ordering::SeqCst);
        if id > 0 {
            unsafe {
                libc::kill(-id, signal);
            }
        }
    }
}

fn send(id: u32, signal: libc::c_int) {
    if let Ok(id) = i32::try_from(id)
        && id > 0
    {
        unsafe {
            libc::kill(-id, signal);
        }
    }
}
