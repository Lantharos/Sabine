use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use log::{Level, LevelFilter, Log, Metadata, Record};
use serde::Deserialize;

use crate::{AppEnvironment, BridgeError, BridgeResponse, SabineWindow};

const FILE: &str = "app";
const EXTENSION: &str = "log";
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// The log file and the ones before it that the app keeps.
const KEPT_FILES: usize = 3;
/// Sent by the browser for each console message of the app's pages.
const CONSOLE_COMMAND: &str = "sabine.log.console";
const PAGE_TARGET: &str = "page";

/// Where the app's `log` records go: a file in the app's log folder that
/// starts over at 4 MB, keeping the two before it, and in development the
/// terminal too.
#[derive(Clone, Debug)]
pub struct Logging {
    level: LevelFilter,
    page_console: bool,
}

impl Default for Logging {
    fn default() -> Self {
        Self {
            level: LevelFilter::Info,
            page_console: false,
        }
    }
}

impl Logging {
    pub fn new() -> Self {
        Self::default()
    }

    /// The least severe records the log keeps. `Info` unless set.
    pub fn level(mut self, level: LevelFilter) -> Self {
        self.level = level;
        self
    }

    /// Also keeps what the app's pages write to their console, under the
    /// `page` target.
    pub fn page_console(mut self, enabled: bool) -> Self {
        self.page_console = enabled;
        self
    }

    pub(crate) fn forwards_page_console(&self) -> bool {
        self.page_console
    }

    /// Makes this the destination of the `log` crate's records for the app
    /// with `app_id`.
    pub(crate) fn install(&self, app_id: &str, echo: bool) {
        let folder = AppEnvironment::current().app_log_dir(app_id);
        let installed = LogFile::open(folder.clone()).and_then(|file| {
            log::set_boxed_logger(Box::new(FileLog {
                level: self.level,
                echo,
                file: Mutex::new(file),
            }))
            .map_err(std::io::Error::other)
        });
        match installed {
            Ok(()) => log::set_max_level(self.level),
            Err(error) => sabine_runtime::report_error(
                "log",
                format!("could not log to {}: {error}", folder.display()),
            ),
        }
    }
}

struct FileLog {
    level: LevelFilter,
    echo: bool,
    file: Mutex<LogFile>,
}

impl Log for FileLog {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{:.3} {:<5} {}: {}\n",
            jiff::Timestamp::now(),
            record.level(),
            record.target(),
            record.args()
        );
        if self.echo {
            eprint!("{line}");
        }
        self.file().write(line.as_bytes());
    }

    fn flush(&self) {
        let _ = self.file().file.flush();
    }
}

impl FileLog {
    fn file(&self) -> MutexGuard<'_, LogFile> {
        self.file.lock().unwrap_or_else(|error| error.into_inner())
    }
}

struct LogFile {
    folder: PathBuf,
    file: File,
    size: u64,
}

impl LogFile {
    fn open(folder: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&folder)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(numbered(&folder, 0))?;
        let size = file.metadata()?.len();
        Ok(Self { folder, file, size })
    }

    fn write(&mut self, line: &[u8]) {
        if self.size + line.len() as u64 > MAX_FILE_BYTES && self.start_over().is_err() {
            return;
        }
        if self.file.write_all(line).is_ok() {
            self.size += line.len() as u64;
        }
    }

    /// Moves each kept file one number up, dropping the oldest, and starts an
    /// empty log.
    fn start_over(&mut self) -> std::io::Result<()> {
        for number in (1..KEPT_FILES).rev() {
            let older = numbered(&self.folder, number - 1);
            if older.exists() {
                std::fs::rename(older, numbered(&self.folder, number))?;
            }
        }
        *self = Self::open(self.folder.clone())?;
        Ok(())
    }
}

/// `app.log`, and `app.1.log` and on for the files before it.
fn numbered(folder: &Path, number: usize) -> PathBuf {
    match number {
        0 => folder.join(format!("{FILE}.{EXTENSION}")),
        number => folder.join(format!("{FILE}.{number}.{EXTENSION}")),
    }
}

#[derive(Deserialize)]
struct ConsoleMessage {
    level: String,
    message: String,
    source: String,
    line: u32,
}

impl SabineWindow {
    /// Takes the console messages the browser forwards from the app's pages.
    pub(crate) fn page_console_command(self) -> Self {
        self.bridge_handler(CONSOLE_COMMAND, |command| {
            let message: ConsoleMessage = serde_json::from_value(command.params)
                .map_err(|error| BridgeError::new(error.to_string()))?;
            let level = match message.level.as_str() {
                "error" => Level::Error,
                "warn" => Level::Warn,
                "debug" => Level::Debug,
                _ => Level::Info,
            };
            let place = if message.source.is_empty() {
                String::new()
            } else {
                format!(" ({}:{})", message.source, message.line)
            };
            log::logger().log(
                &Record::builder()
                    .level(level)
                    .target(PAGE_TARGET)
                    .args(format_args!("{}{place}", message.message))
                    .build(),
            );
            Ok(BridgeResponse::json(serde_json::Value::Null))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{KEPT_FILES, LogFile, MAX_FILE_BYTES, numbered};

    #[test]
    fn starts_over_keeping_the_files_before() {
        let folder = std::env::temp_dir().join(format!("sabine-log-{}", std::process::id()));
        let mut log = LogFile::open(folder.clone()).unwrap();
        let line = vec![b'x'; (MAX_FILE_BYTES / 2) as usize];
        for _ in 0..8 {
            log.write(&line);
        }
        log.file.flush().unwrap();
        for number in 0..KEPT_FILES {
            assert!(numbered(&folder, number).is_file());
        }
        assert!(!numbered(&folder, KEPT_FILES).exists());
        assert_eq!(
            std::fs::metadata(numbered(&folder, 0)).unwrap().len(),
            MAX_FILE_BYTES
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
}
