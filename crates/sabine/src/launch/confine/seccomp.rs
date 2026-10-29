use libc::{
    BPF_ABS, BPF_JEQ, BPF_JGE, BPF_JMP, BPF_K, BPF_LD, BPF_RET, BPF_W, SECCOMP_RET_ALLOW,
    SECCOMP_RET_ERRNO, sock_filter, sock_fprog,
};

const ARCHITECTURE: u32 = 4;
const NUMBER: u32 = 0;
const SECOND_ARGUMENT: u32 = 24;
const REFUSE: u32 = SECCOMP_RET_ERRNO | libc::EPERM as u32;
const X32_SYSCALLS: u32 = 0x4000_0000;

#[cfg(target_arch = "x86_64")]
const NATIVE: u32 = 0xc000_003e;
#[cfg(target_arch = "aarch64")]
const NATIVE: u32 = 0xc000_00b7;

/// Calls no app needs that reach past its own processes: the kernel
/// keyrings shared with your session, the kernel log, other processes'
/// memory and memory placement, and accounting.
const REFUSED: [libc::c_long; 13] = [
    libc::SYS_syslog,
    libc::SYS_acct,
    libc::SYS_quotactl,
    libc::SYS_add_key,
    libc::SYS_keyctl,
    libc::SYS_request_key,
    libc::SYS_move_pages,
    libc::SYS_mbind,
    libc::SYS_get_mempolicy,
    libc::SYS_set_mempolicy,
    libc::SYS_migrate_pages,
    libc::SYS_perf_event_open,
    libc::SYS_ptrace,
];

/// Terminal requests that type into, or take over, the terminal the app was
/// started from.
const REFUSED_IOCTLS: [libc::c_ulong; 2] = [libc::TIOCSTI, libc::TIOCLINUX];

const fn statement(code: u32, k: u32) -> sock_filter {
    sock_filter {
        code: code as u16,
        jt: 0,
        jf: 0,
        k,
    }
}

const fn jump(code: u32, k: u32, jt: u8, jf: u8) -> sock_filter {
    sock_filter {
        code: code as u16,
        jt,
        jf,
        k,
    }
}

const fn refuse_when_equal(value: u32) -> [sock_filter; 2] {
    [
        jump(BPF_JMP | BPF_JEQ | BPF_K, value, 0, 1),
        statement(BPF_RET | BPF_K, REFUSE),
    ]
}

fn program() -> Vec<sock_filter> {
    let mut program = vec![
        statement(BPF_LD | BPF_W | BPF_ABS, ARCHITECTURE),
        jump(BPF_JMP | BPF_JEQ | BPF_K, NATIVE, 1, 0),
        statement(BPF_RET | BPF_K, REFUSE),
        statement(BPF_LD | BPF_W | BPF_ABS, NUMBER),
        jump(BPF_JMP | BPF_JGE | BPF_K, X32_SYSCALLS, 0, 1),
        statement(BPF_RET | BPF_K, REFUSE),
    ];
    for call in REFUSED {
        program.extend(refuse_when_equal(call as u32));
    }
    program.push(jump(
        BPF_JMP | BPF_JEQ | BPF_K,
        libc::SYS_ioctl as u32,
        0,
        1 + 2 * REFUSED_IOCTLS.len() as u8,
    ));
    program.push(statement(BPF_LD | BPF_W | BPF_ABS, SECOND_ARGUMENT));
    for request in REFUSED_IOCTLS {
        program.extend(refuse_when_equal(request as u32));
    }
    program.push(statement(BPF_RET | BPF_K, SECCOMP_RET_ALLOW));
    program
}

/// Filters the system calls of every thread of the app and of everything it
/// starts. Landlock has already set `no_new_privs`, which this requires.
pub(super) fn apply() -> std::io::Result<()> {
    let mut program = program();
    let filter = sock_fprog {
        len: program.len() as u16,
        filter: program.as_mut_ptr(),
    };
    let applied = unsafe {
        libc::syscall(
            libc::SYS_seccomp,
            libc::SECCOMP_SET_MODE_FILTER,
            libc::SECCOMP_FILTER_FLAG_TSYNC,
            &raw const filter,
        )
    };
    if applied != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
