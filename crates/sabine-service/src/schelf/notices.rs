use super::{REVOCATIONS_REFRESH, Trust, revocations::BlockAction};
use crate::{AppUpdateConfig, RegisteredApp, SabineService, ServiceError, ServiceResult};

/// What an app from Schelf shows before it opens, when Schelf has pulled the
/// version that is starting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreNotice {
    /// The version can't start. `removable` says whether Sabine can take the
    /// app off this computer for the person.
    Blocked {
        explanation: String,
        removable: bool,
    },
    /// The version starts once the person has read why Schelf warns about it.
    Warning { explanation: String },
}

impl SabineService {
    /// What Schelf says about starting `version` of `id`, for apps that
    /// update from Schelf or that it installed. Checks a revocation list older
    /// than an hour again first, briefly, and otherwise goes by the last one.
    /// A warning shows only until it's dismissed for that version.
    pub fn store_notice(&self, id: &str, version: &str) -> Option<StoreNotice> {
        let app = self.app(id).ok()?;
        if !follows_schelf(&app) {
            return None;
        }
        let revocation = Trust::at_launch(&self.root)
            .revocations(REVOCATIONS_REFRESH)
            .unwrap_or_else(|error| {
                sabine_runtime::report_error("schelf", &error);
                None
            })?
            .find(id, version)?;
        match revocation.action {
            BlockAction::Block => Some(StoreNotice::Blocked {
                explanation: revocation.explanation,
                removable: self.removable(&app),
            }),
            BlockAction::Warn if self.warning_dismissed(id, version) => None,
            BlockAction::Warn => Some(StoreNotice::Warning {
                explanation: revocation.explanation,
            }),
        }
    }

    /// Keeps the warning about `version` of `id` from showing again.
    pub fn dismiss_store_warning(&self, id: &str, version: &str) -> ServiceResult<()> {
        let path = self.dismissed_warning_path(id)?;
        std::fs::create_dir_all(path.parent().expect("app folders have a parent"))?;
        std::fs::write(path, version)?;
        Ok(())
    }

    /// Fetches Schelf's revocation list when it's an hour old and an app here
    /// comes from Schelf.
    pub fn refresh_store_revocations(&self) -> ServiceResult<()> {
        if self.apps()?.iter().any(follows_schelf) {
            Trust::new(&self.root).revocations(REVOCATIONS_REFRESH)?;
        }
        Ok(())
    }

    /// Refuses `version` of `id` when Schelf has blocked it. Without a list,
    /// as on a computer that has been offline since it was set up, nothing is
    /// refused.
    pub(crate) fn refuse_blocked_release(&self, id: &str, version: &str) -> ServiceResult<()> {
        let blocked = Trust::new(&self.root)
            .revocations(REVOCATIONS_REFRESH)
            .unwrap_or_else(|error| {
                sabine_runtime::report_error("schelf", &error);
                None
            })
            .and_then(|list| list.find(id, version))
            .filter(|revocation| revocation.action == BlockAction::Block);
        match blocked {
            Some(revocation) => Err(ServiceError::Update(format!(
                "Schelf has blocked {id} {version}: {}",
                revocation.explanation
            ))),
            None => Ok(()),
        }
    }

    fn warning_dismissed(&self, id: &str, version: &str) -> bool {
        self.dismissed_warning_path(id)
            .and_then(|path| Ok(std::fs::read_to_string(path)?))
            .is_ok_and(|dismissed| dismissed == version)
    }

    fn dismissed_warning_path(&self, id: &str) -> ServiceResult<std::path::PathBuf> {
        if !crate::valid_app_id(id) {
            return Err(ServiceError::Update("invalid app identifier".into()));
        }
        Ok(self
            .root
            .join("apps")
            .join(id)
            .join("dismissed-store-warning"))
    }

    fn removable(&self, app: &RegisteredApp) -> bool {
        cfg!(target_os = "windows") || installed_by_store(app)
    }
}

fn follows_schelf(app: &RegisteredApp) -> bool {
    app.manifest
        .update
        .as_ref()
        .is_some_and(AppUpdateConfig::follows_schelf)
}

/// Whether a store installed `app`, and so removes and updates it.
pub fn installed_by_store(app: &RegisteredApp) -> bool {
    app.manifest
        .update
        .as_ref()
        .is_some_and(|update| update.install_mode == crate::AppInstallMode::Store)
}
