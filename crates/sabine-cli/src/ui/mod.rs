mod output;
mod step;

pub(crate) use output::{run, run_cargo, tool};
pub(crate) use step::Step;

use std::{io::IsTerminal, path::Path, sync::LazyLock, time::Duration};

use console::style;

/// Whether progress is drawn in place. Pipes, CI logs and dumb terminals get
/// plain lines and the tools' own output instead.
static LIVE: LazyLock<bool> = LazyLock::new(|| {
    std::io::stderr().is_terminal()
        && std::env::var_os("CI").is_none()
        && std::env::var("TERM").map_or(true, |term| term != "dumb")
});

pub(crate) fn live() -> bool {
    *LIVE
}

pub(crate) fn duration(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs_f64();
    if seconds < 10.0 {
        format!("{seconds:.1}s")
    } else if seconds < 60.0 {
        format!("{}s", seconds.round())
    } else {
        let whole = elapsed.as_secs();
        format!("{}m {:02}s", whole / 60, whole % 60)
    }
}

pub(crate) fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    if value < 1000.0 {
        return format!("{bytes} B");
    }
    let mut unit = 0;
    value /= 1000.0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// `path` from the working directory when it's inside it, as people type it.
pub(crate) fn shown(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|here| path.strip_prefix(here).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

pub(crate) fn error(message: &str) {
    let mut lines = message.lines();
    if let Some(first) = lines.next() {
        eprintln!("{} {first}", style("error:").for_stderr().red().bold());
    }
    for line in lines {
        eprintln!("       {line}");
    }
}

/// A line about what Sabine is doing, among the output of what it runs.
pub(crate) fn status(message: &str) {
    eprintln!("{} {message}", style("›").for_stderr().cyan().bold());
}

pub(crate) fn warning(message: &str) {
    eprintln!(
        "{} {message}",
        style("warning:").for_stderr().yellow().bold()
    );
}

pub(crate) fn hint(message: &str) {
    eprintln!("  {}", style(message).for_stderr().dim());
}

/// A finished result, such as a package written, with what's worth knowing
/// about it lined up beside it.
pub(crate) fn result(label: &str, note: &str) {
    eprintln!(
        "  {}  {}",
        style(label).for_stderr().bold(),
        style(note).for_stderr().dim()
    );
}

pub(crate) fn summary(message: &str) {
    eprintln!("\n{}", style(message).for_stderr().green().bold());
}
