use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    process::Child,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;
const KEPT_LOG_BYTES: u64 = MAX_LOG_BYTES / 2;

pub fn diagnostic_path(component: &str) -> PathBuf {
    let name: String = component
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    crate::sabine_data_dir()
        .join("logs")
        .join(format!("{name}.jsonl"))
}

pub fn record_diagnostic(component: &str, message: &str) -> io::Result<()> {
    let path = diagnostic_path(component);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true).read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.lock()?;
    let mut remaining = message;
    loop {
        let mut end = remaining.len().min(16 * 1024);
        while !remaining.is_char_boundary(end) {
            end -= 1;
        }
        let entry = serde_json::json!({
            "time": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            "pid": std::process::id(),
            "component": component,
            "message": &remaining[..end],
        });
        let mut bytes = serde_json::to_vec(&entry)?;
        bytes.push(b'\n');
        if file.metadata()?.len().saturating_add(bytes.len() as u64) > MAX_LOG_BYTES {
            discard_oldest_entries(&mut file)?;
        }
        file.write_all(&bytes)?;
        remaining = &remaining[end..];
        if remaining.is_empty() {
            return Ok(());
        }
    }
}

/// Keeps the newest entries that fit in [`KEPT_LOG_BYTES`], so a log that
/// reached its limit still holds what led up to the next entry.
fn discard_oldest_entries(file: &mut File) -> io::Result<()> {
    let length = file.metadata()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(KEPT_LOG_BYTES)))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    let first_whole_entry = tail
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(tail.len(), |end| end + 1);
    file.set_len(0)?;
    file.write_all(&tail[first_whole_entry..])
}

pub fn report_error(component: &str, error: impl std::fmt::Display) {
    let message = error.to_string();
    eprintln!("{component}: {message}");
    if let Err(error) = record_diagnostic(component, &message) {
        eprintln!("could not save diagnostic: {error}");
    }
}

pub fn capture_diagnostics(child: &mut Child, component: &'static str) {
    let Some(mut stderr) = child.stderr.take() else {
        return;
    };
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let message = String::from_utf8_lossy(&buffer[..count]);
                    let _ = io::stderr().write_all(&buffer[..count]);
                    let _ = record_diagnostic(component, &message);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    report_error(component, error);
                    break;
                }
            }
        }
    });
}
