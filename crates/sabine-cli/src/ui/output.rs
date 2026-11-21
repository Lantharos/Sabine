use std::{
    io::{BufRead, BufReader, Read},
    process::{Command, ExitStatus, Stdio},
    sync::Mutex,
};

use super::Step;

/// The words Cargo starts its status lines with; everything else it writes
/// to stderr is a diagnostic worth showing.
const CARGO_STATUS: [&str; 18] = [
    "Adding",
    "Blocking",
    "Building",
    "Checking",
    "Compiling",
    "Documenting",
    "Downloaded",
    "Downloading",
    "Finished",
    "Fresh",
    "Locking",
    "Packaging",
    "Removing",
    "Running",
    "Unpacking",
    "Updating",
    "Upgrading",
    "Unchanged",
];

/// Calls `each` with every line `source` writes, including the ones a tool
/// redraws in place with carriage returns.
fn lines(source: impl Read, mut each: impl FnMut(&str)) {
    let mut reader = BufReader::new(source);
    let mut line = Vec::new();
    let mut chunk = [0; 8192];
    while let Ok(read) = reader.read(&mut chunk) {
        if read == 0 {
            break;
        }
        for &byte in &chunk[..read] {
            if byte == b'\n' || byte == b'\r' {
                each(&String::from_utf8_lossy(&line));
                line.clear();
            } else {
                line.push(byte);
            }
        }
    }
    if !line.is_empty() {
        each(&String::from_utf8_lossy(&line));
    }
}

fn plain(line: &str) -> String {
    console::strip_ansi_codes(line).trim().to_owned()
}

/// Lines kept to show later, with runs of blank lines folded into one.
#[derive(Default)]
struct Kept(Mutex<Vec<String>>);

impl Kept {
    fn push(&self, line: &str) {
        let mut lines = self
            .0
            .lock()
            .expect("no writer panics while holding the lines");
        let blank = plain(line).is_empty();
        if blank && lines.last().is_none_or(|last| plain(last).is_empty()) {
            return;
        }
        lines.push(line.trim_end().to_owned());
    }

    fn show(self, step: &Step) {
        let mut lines = self.0.into_inner().expect("the readers have finished");
        while lines.last().is_some_and(|last| plain(last).is_empty()) {
            lines.pop();
        }
        if !lines.is_empty() {
            step.above(|| lines.iter().for_each(|line| eprintln!("{line}")));
        }
    }
}

fn checked(status: std::io::Result<ExitStatus>, what: &str) -> Result<(), String> {
    let status = status.map_err(|error| format!("could not run {what}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{what} failed ({status})"))
    }
}

/// Runs `command` as part of `step`, showing its latest line beside the step
/// and its whole output only if it fails.
pub(crate) fn run(step: &Step, command: &mut Command, what: &str) -> Result<(), String> {
    command.stdin(Stdio::null());
    if !super::live() {
        return checked(command.status(), what);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run {what}: {error}"))?;
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let kept = Kept::default();
    let record = |line: &str| {
        let shown = plain(line);
        if !shown.is_empty() {
            step.note(&shown);
        }
        kept.push(line);
    };
    std::thread::scope(|scope| {
        if let Some(out) = out {
            scope.spawn(|| lines(out, record));
        }
        if let Some(err) = err {
            lines(err, record);
        }
    });
    let result = checked(child.wait(), what);
    if result.is_err() {
        kept.show(step);
    }
    result
}

/// Runs a tool that must be installed, such as a packager or signer. While
/// progress is drawn its output is kept, and shown only if it fails.
pub(crate) fn tool(command: &mut Command) -> Result<(), String> {
    let tool = command.get_program().to_string_lossy().into_owned();
    let missing = |error: std::io::Error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("{tool} is required to build this package; install it and retry")
        } else {
            format!("could not run {tool}: {error}")
        }
    };
    command.stdin(Stdio::null());
    if !super::live() {
        return checked(Ok(command.status().map_err(missing)?), &tool);
    }
    let output = command.output().map_err(missing)?;
    if output.status.success() {
        return Ok(());
    }
    let said = [output.stdout, output.stderr]
        .iter()
        .map(|bytes| String::from_utf8_lossy(bytes).trim().to_owned())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!("{tool} failed ({})\n{said}", output.status)
        .trim_end()
        .to_owned())
}

/// Runs a Cargo build as part of `step`: its progress becomes the step's bar,
/// its diagnostics are shown once it's done, and every line it writes to
/// stdout goes to `message`.
pub(crate) fn run_cargo(
    step: &Step,
    command: &mut Command,
    mut message: impl FnMut(&str),
) -> Result<(), String> {
    let live = super::live();
    command.stdin(Stdio::null()).stdout(Stdio::piped());
    if live {
        command
            .env("CARGO_TERM_PROGRESS_WHEN", "always")
            .env("CARGO_TERM_PROGRESS_WIDTH", "1000")
            .env("CARGO_TERM_COLOR", "always")
            .stderr(Stdio::piped());
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not run cargo build: {error}"))?;
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let diagnostics = Kept::default();
    let read = std::thread::scope(|scope| {
        if let Some(err) = err {
            scope.spawn(|| lines(err, |line| cargo_line(step, &diagnostics, line)));
        }
        let Some(out) = out else {
            return Ok(());
        };
        for line in BufReader::new(out).lines() {
            message(&line.map_err(|error| error.to_string())?);
        }
        Ok::<_, String>(())
    });
    let result = checked(child.wait(), "cargo build");
    diagnostics.show(step);
    read.and(result)
}

fn cargo_line(step: &Step, diagnostics: &Kept, line: &str) {
    let shown = plain(line);
    if let Some(progress) = shown.strip_prefix("Building [") {
        if let Some((done, total)) = counts(progress) {
            step.count(done, total);
        }
        if let Some((_, crates)) = progress.split_once(": ") {
            step.note(crates);
        }
        return;
    }
    if shown
        .split_whitespace()
        .next()
        .is_some_and(|word| CARGO_STATUS.contains(&word))
    {
        if !shown.starts_with("Finished") {
            step.note(&shown);
        }
        return;
    }
    diagnostics.push(line);
}

/// The units built and to build in Cargo's progress line, such as `312/524`
/// in `Building [=====>   ] 312/524: serde, syn`.
fn counts(progress: &str) -> Option<(u64, u64)> {
    let (_, after) = progress.split_once("] ")?;
    let (done, total) = after.split(':').next()?.split_once('/')?;
    Some((done.trim().parse().ok()?, total.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::counts;

    #[test]
    fn reads_cargos_progress_line() {
        assert_eq!(
            counts("=========>         ] 312/524: serde, syn(build)"),
            Some((312, 524))
        );
        assert_eq!(counts("=>  ] 3/9"), Some((3, 9)));
    }
}
