mod app;
pub(crate) mod bindings;
pub(crate) mod process_tree;
mod watch;
pub(crate) mod web_detect;

use std::{
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{ExitCode, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub use bindings::run_bindings;

use crate::{project::environment::BuildEnvironment, runtime, util::shell_command};
use process_tree::ProcessTree;

pub struct DevOptions {
    pub source: PathBuf,
    pub release: bool,
    pub no_install: bool,
    pub web_only: bool,
    pub no_runtime_prepare: bool,
    pub command: Option<String>,
    pub args: Vec<String>,
    /// The id a development install runs the app as.
    pub app_id: Option<String>,
}

pub fn run_dev(options: DevOptions) -> Result<ExitCode, String> {
    let project = web_detect::resolve_dev_project(&options.source)?;
    if !options.no_runtime_prepare && !options.web_only {
        let _ = runtime::ensure_runtime_ready()?;
    }

    let environment = BuildEnvironment::load(
        &project.source,
        project
            .frontend
            .as_ref()
            .map(|frontend| frontend.root.as_path()),
        &[],
        sabine_service::AppEnvironment::Development,
    )?;
    let interrupted = interrupt_flag()?;
    let bindings = (!options.web_only && options.command.is_none()).then(|| {
        let source = project.source.clone();
        let environment = environment.clone();
        let release = options.release;
        thread::spawn(move || bindings::refresh(&source, &environment, release))
    });
    let mut frontend = None;
    if let Some(dev_frontend) = &project.frontend {
        if !options.no_install
            && let Some(package_manager) = dev_frontend.package_manager
        {
            web_detect::install_dependencies(&dev_frontend.root, package_manager)?;
        }
        let url = &dev_frontend.url;
        match (&dev_frontend.command, dev_frontend.port) {
            (_, Some(port)) if port_is_ready(port) => {
                eprintln!("sabine: reusing the server at {url}");
            }
            (Some(command), port) => {
                eprintln!("sabine: frontend {url} ({command})");
                let mut child = spawn_frontend(&dev_frontend.root, command, &environment)?;
                if let Some(port) = port
                    && !wait_for_port(port, url, Some(&mut child), &interrupted)?
                {
                    return Ok(ExitCode::from(130));
                }
                frontend = Some(child);
            }
            (None, Some(port)) => {
                eprintln!("sabine: waiting for the server at {url}");
                if !wait_for_port(port, url, None, &interrupted)? {
                    return Ok(ExitCode::from(130));
                }
            }
            (None, None) => eprintln!("sabine: frontend {url}"),
        }
        if options.web_only {
            eprintln!("sabine: web-only mode; Ctrl+C to stop");
            let code = app::wait_frontend(frontend.as_mut(), &interrupted)?;
            return Ok(code);
        }
    } else {
        eprintln!("sabine: static web assets (no frontend dev server)");
    }
    if let Some(bindings) = bindings {
        bindings
            .join()
            .map_err(|_| "writing the bindings panicked".to_string())??;
    }

    eprintln!(
        "sabine: cargo run --manifest-path {}",
        project.cargo_manifest.display()
    );
    let result = app::run(app::Session {
        project: &project,
        options: &options,
        environment: &environment,
        frontend: frontend.as_mut(),
        interrupted: &interrupted,
    });
    drop(frontend);
    result
}

fn interrupt_flag() -> Result<Arc<AtomicBool>, String> {
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&interrupted);
    ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst))
        .map_err(|error| format!("failed to install Ctrl+C handler: {error}"))?;
    Ok(interrupted)
}

fn spawn_frontend(
    root: &Path,
    command: &str,
    environment: &BuildEnvironment,
) -> Result<ProcessTree, String> {
    let mut process = shell_command(command);
    environment.apply(&mut process);
    process
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    ProcessTree::spawn(&mut process)
        .map_err(|error| format!("failed to start frontend `{command}`: {error}"))
}

fn port_is_ready(port: u16) -> bool {
    [
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
    ]
    .into_iter()
    .any(|ip| {
        TcpStream::connect_timeout(
            &std::net::SocketAddr::new(ip, port),
            Duration::from_millis(100),
        )
        .is_ok()
    })
}

fn wait_for_port(
    port: u16,
    url: &str,
    mut child: Option<&mut ProcessTree>,
    interrupted: &AtomicBool,
) -> Result<bool, String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last_error = None;
    while Instant::now() < deadline {
        if interrupted.load(Ordering::SeqCst) {
            return Ok(false);
        }
        if let Some(child) = child.as_mut()
            && let Ok(Some(status)) = child.try_wait()
        {
            return Err(format!(
                "frontend exited before `{url}` became available: {status}"
            ));
        }
        for host in ["127.0.0.1", "localhost"] {
            match (host, port).to_socket_addrs() {
                Ok(addresses) => {
                    for socket in addresses {
                        match TcpStream::connect_timeout(&socket, Duration::from_millis(150)) {
                            Ok(_) => return Ok(true),
                            Err(error) => last_error = Some(error),
                        }
                    }
                }
                Err(error) => last_error = Some(error),
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!(
        "timed out waiting for frontend `{url}`{}",
        last_error
            .map(|error| format!(": {error}"))
            .unwrap_or_default()
    ))
}
