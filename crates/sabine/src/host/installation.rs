use std::{fs::File, os::unix::fs::MetadataExt};

use sabine_service::SabineService;

use crate::window::config::SabineWindowConfig;

/// The installed program this process runs. Holding it open with a shared
/// lock keeps its installer from removing the files of this version while
/// the process runs, and comparing it with the app's registration tells when
/// a newer copy of the app was installed.
pub(crate) struct Installation {
    app_id: String,
    program: File,
}

impl Installation {
    pub(crate) fn hold(config: &SabineWindowConfig) -> Option<Self> {
        if config.dev_mode() {
            return None;
        }
        let app_id = config.app_id.clone()?;
        let program = File::open("/proc/self/exe").ok()?;
        program.lock_shared().ok()?;
        Some(Self { app_id, program })
    }

    /// Whether the app's registration now launches a program other than the
    /// one this process runs.
    pub(crate) fn superseded(&self) -> bool {
        let Ok(program) = SabineService::default().launched_program(&self.app_id) else {
            return false;
        };
        let (Ok(installed), Ok(running)) = (std::fs::metadata(program), self.program.metadata())
        else {
            return false;
        };
        (installed.dev(), installed.ino()) != (running.dev(), running.ino())
    }
}
