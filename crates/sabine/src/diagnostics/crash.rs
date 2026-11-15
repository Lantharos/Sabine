use std::{
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::AppEnvironment;

const FOLDER: &str = "crashes";
/// Where Chromium's crash database keeps finished reports: waiting for
/// upload and uploaded on Linux and macOS, and all of them on Windows.
const REPORT_FOLDERS: [&str; 3] = ["pending", "completed", "reports"];
const ATTACHMENTS: &str = "attachments";
const DUMP_EXTENSION: &str = "dmp";
const MAX_REPORTS: usize = 10;
const MAX_BYTES: u64 = 20 * 1024 * 1024;

/// A minidump Chromium wrote when one of the app's browser, page, GPU or
/// utility processes crashed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrashReport {
    pub id: String,
    pub path: PathBuf,
    pub created: SystemTime,
    pub size: u64,
}

/// The crash reports of an app, kept in the `crashes` folder of its data
/// folder. Sabine keeps the newest ten, up to 20 MB.
#[derive(Clone, Debug)]
pub struct CrashReports {
    folder: PathBuf,
}

impl CrashReports {
    /// The crash reports of the app with `app_id`, as `Sabine.toml` names it.
    /// Development runs keep theirs under the development app id.
    pub fn for_app(app_id: &str) -> Self {
        Self::at(AppEnvironment::current().app_data_dir(app_id).join(FOLDER))
    }

    pub(crate) fn at(folder: PathBuf) -> Self {
        Self { folder }
    }

    /// The folder Chromium writes the reports in.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Every report, newest first.
    pub fn list(&self) -> io::Result<Vec<CrashReport>> {
        let mut reports = Vec::new();
        for folder in REPORT_FOLDERS {
            let entries = match std::fs::read_dir(self.folder.join(folder)) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            for entry in entries {
                let path = entry?.path();
                if path
                    .extension()
                    .is_none_or(|extension| extension != DUMP_EXTENSION)
                {
                    continue;
                }
                let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                let metadata = std::fs::metadata(&path)?;
                reports.push(CrashReport {
                    id: id.to_string(),
                    created: metadata.modified()?,
                    size: metadata.len(),
                    path,
                });
            }
        }
        reports.sort_by_key(|report| std::cmp::Reverse(report.created));
        Ok(reports)
    }

    /// Deletes the report with `id`. Returns whether there was one.
    pub fn delete(&self, id: &str) -> io::Result<bool> {
        let Some(report) = self.list()?.into_iter().find(|report| report.id == id) else {
            return Ok(false);
        };
        self.remove(&report)?;
        Ok(true)
    }

    /// Deletes every report.
    pub fn clear(&self) -> io::Result<()> {
        self.list()?
            .iter()
            .try_for_each(|report| self.remove(report))
    }

    /// Deletes all but the newest reports that fit in the limits.
    pub(crate) fn prune(&self) -> io::Result<()> {
        let mut kept_bytes = 0;
        for (index, report) in self.list()?.iter().enumerate() {
            kept_bytes += report.size;
            if index >= MAX_REPORTS || kept_bytes > MAX_BYTES {
                self.remove(report)?;
            }
        }
        Ok(())
    }

    fn remove(&self, report: &CrashReport) -> io::Result<()> {
        std::fs::remove_file(&report.path)?;
        let _ = std::fs::remove_file(report.path.with_extension("meta"));
        let _ = std::fs::remove_dir_all(self.folder.join(ATTACHMENTS).join(&report.id));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::CrashReports;

    #[test]
    fn keeps_the_newest_reports() {
        let folder =
            std::env::temp_dir().join(format!("sabine-crash-reports-{}", std::process::id()));
        let pending = folder.join("pending");
        std::fs::create_dir_all(&pending).unwrap();
        let start = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        for index in 0..12_u64 {
            let dump = pending.join(format!("report-{index:02}.dmp"));
            std::fs::write(&dump, [0_u8; 16]).unwrap();
            std::fs::write(dump.with_extension("meta"), []).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&dump)
                .unwrap()
                .set_modified(start + std::time::Duration::from_secs(index))
                .unwrap();
        }
        let reports = CrashReports::at(folder.clone());
        reports.prune().unwrap();
        let kept = reports.list().unwrap();
        assert_eq!(kept.len(), 10);
        assert_eq!(kept[0].id, "report-11");
        assert!(!pending.join("report-01.meta").exists());
        assert!(reports.delete("report-11").unwrap());
        assert!(!reports.delete("report-11").unwrap());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
