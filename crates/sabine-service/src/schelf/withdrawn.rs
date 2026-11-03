//! Apps whose publisher deleted them from schelf.store. Their feeds answer
//! 410 Gone, so they keep running without updates, and Sabine asks again
//! only now and then in case the publisher restores them.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::types::ServiceResult;

const RECHECK: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Whether schelf.store still offers updates for one app, as Sabine last heard.
pub(crate) struct Withdrawal {
    path: PathBuf,
}

impl Withdrawal {
    pub fn of(sabine_root: &Path, id: &str) -> Self {
        Self {
            path: sabine_root
                .join("apps")
                .join(id)
                .join("withdrawn-from-schelf"),
        }
    }

    /// Notes that schelf.store answered Gone at `now`.
    pub fn record(&self, now: u64) -> ServiceResult<()> {
        std::fs::create_dir_all(self.path.parent().expect("apps have a folder"))?;
        std::fs::write(&self.path, now.to_string())?;
        Ok(())
    }

    pub fn clear(&self) -> ServiceResult<()> {
        Ok(sabine_runtime::remove_file_if_present(&self.path)?)
    }

    /// Whether to ask schelf.store about the app again at `now`.
    pub fn due(&self, now: u64) -> bool {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|seen| seen.trim().parse::<u64>().ok())
            .is_none_or(|seen| now >= seen.saturating_add(RECHECK.as_secs()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::unix_timestamp;

    #[test]
    fn an_app_gone_from_the_store_is_asked_about_weekly_until_it_returns() {
        let root = std::env::temp_dir().join(format!(
            "sabine-withdrawn-{}-{}",
            std::process::id(),
            unix_timestamp()
        ));
        let withdrawal = Withdrawal::of(&root, "com.example.notes");
        let now = unix_timestamp();
        assert!(withdrawal.due(now));

        withdrawal.record(now).unwrap();
        assert!(!withdrawal.due(now));
        assert!(!withdrawal.due(now + RECHECK.as_secs() - 60));
        assert!(withdrawal.due(now + RECHECK.as_secs()));

        withdrawal.clear().unwrap();
        assert!(withdrawal.due(now));
        std::fs::remove_dir_all(root).unwrap();
    }
}
