use sabine_dialog::{ProgressOutcome, Prompt, WindowAttributes};

use super::apps;
use crate::{SabineService, ServiceError, ServiceResult};

/// Asks before uninstalling Sabine, naming the apps that run on it and would
/// be uninstalled with it, then uninstalls them and Sabine with their progress
/// on screen. App data stays on the computer.
pub fn uninstall_with_prompt() -> ServiceResult<()> {
    let apps = SabineService::default().apps()?;
    let prompt = if apps.is_empty() {
        Prompt {
            title: "Uninstall Sabine".into(),
            body: "Apps that run on Sabine set it up again when you install one.".into(),
            cancel: "Cancel".into(),
            confirm: "Uninstall".into(),
            cancel_by_default: true,
            icon: sabine_icon,
        }
    } else {
        let names = apps
            .iter()
            .map(apps::display_name)
            .collect::<Vec<_>>()
            .join("\n");
        Prompt {
            title: "Uninstall Sabine and its apps".into(),
            body: format!(
                "These apps run on Sabine and stop working without it, so they're uninstalled too:\n\n{names}\n\nTheir data stays on this computer."
            ),
            cancel: "Cancel".into(),
            confirm: "Uninstall all".into(),
            cancel_by_default: true,
            icon: sabine_icon,
        }
    };
    if !sabine_dialog::confirm(prompt).map_err(ServiceError::Update)? {
        return Ok(());
    }
    let outcome = sabine_dialog::run_progress_window(
        "Uninstalling Sabine",
        sabine_icon,
        move |state, proxy| {
            let steps = (apps.len() + 1) as f32;
            let result = (|| -> ServiceResult<()> {
                let service = SabineService::default();
                for (index, app) in apps.iter().enumerate() {
                    let message = format!("Uninstalling {}", apps::display_name(app));
                    sabine_dialog::set_progress(
                        &state,
                        &proxy,
                        message,
                        Some(index as f32 / steps),
                    );
                    apps::remove(&service, app)?;
                }
                sabine_dialog::set_progress(
                    &state,
                    &proxy,
                    "Removing Sabine",
                    Some((steps - 1.0) / steps),
                );
                super::uninstall_system(false).map(|_| ())
            })();
            sabine_dialog::finish(&state, &proxy, result.map_err(|error| error.to_string()));
        },
    )
    .map_err(ServiceError::Update)?;
    if let ProgressOutcome::Failed(error) = outcome {
        sabine_dialog::show_notice("Sabine couldn't be uninstalled", &error, sabine_icon)
            .map_err(ServiceError::Update)?;
    }
    Ok(())
}

fn sabine_icon(attributes: WindowAttributes) -> WindowAttributes {
    sabine_dialog::with_sabine_icon(attributes, crate::icon_png)
}
