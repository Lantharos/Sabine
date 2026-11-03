use std::process::Stdio;

use sabine_runtime::background_command;
use sabine_service::{SabineService, StoreNotice};

use super::{BLOCKED_ARG, NOTICE_ARG, icons};
use crate::window::config::SabineWindowConfig;

/// Keeps a version Schelf has blocked from opening, offering to remove the
/// app, and shows Schelf's warning about a version once before it opens.
pub(super) fn check_store_standing(config: &SabineWindowConfig) {
    if config.dev_mode() {
        return;
    }
    let (Some(id), Some(version)) = (config.app_id.as_deref(), config.app_version.as_deref())
    else {
        return;
    };
    let service = SabineService::default();
    let name = &config.title;
    match service.store_notice(id, version) {
        None => {}
        Some(StoreNotice::Warning { explanation }) => {
            ask(&[
                NOTICE_ARG,
                &format!("Before you use {name}"),
                &format!("Schelf has a warning about this version of {name}: {explanation}"),
            ]);
            let _ = service.dismiss_store_warning(id, version);
        }
        Some(StoreNotice::Blocked {
            explanation,
            removable,
        }) => {
            let title = format!("{name} can't open");
            let stopped = format!("Schelf stopped this version of {name}: {explanation}");
            if !removable {
                ask(&[
                    NOTICE_ARG,
                    &title,
                    &format!(
                        "{stopped}\n\nRemove it the way you installed it. Everything you made with it stays on this computer."
                    ),
                ]);
            } else if ask(&[
                BLOCKED_ARG,
                &title,
                &format!("{stopped}\n\nRemoving it keeps everything you made with it."),
            ]) {
                remove_after_exit(id);
            }
            std::process::exit(1);
        }
    }
}

/// Asks the person to remove a blocked app, in a process of its own.
pub(super) fn confirm_removal(title: &str, body: &str) -> Result<bool, String> {
    sabine_dialog::confirm(sabine_dialog::Prompt {
        title: title.into(),
        body: body.into(),
        cancel: "Close".into(),
        confirm: "Remove".into(),
        cancel_by_default: true,
        icon: icons::app,
    })
}

/// Shows a dialog in a process of its own, and whether the person agreed.
fn ask(args: &[&str]) -> bool {
    std::env::current_exe().ok().is_some_and(|executable| {
        background_command(executable)
            .args(args)
            .status()
            .is_ok_and(|status| status.success())
    })
}

fn remove_after_exit(id: &str) {
    let Ok(service) = sabine_service::resolve_service_executable() else {
        return;
    };
    let _ = background_command(service)
        .arg("remove")
        .arg(id)
        .arg("--wait-pid")
        .arg(std::process::id().to_string())
        .stdin(Stdio::null())
        .spawn();
}
