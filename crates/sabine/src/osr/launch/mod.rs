mod browser;
mod window_host;

pub(crate) use browser::{CefViewport, browser_launch};
pub(crate) use window_host::{
    WindowContext, attach_open_window, launch_process, open_primary_window,
};

use std::ffi::OsString;

use crate::window::config::SabineWindowConfig;
use crate::{SabineError, SabineResult};

pub(crate) const OSR_HOST_ARG: &str = "--sabine-osr-host";

/// Whether the app was started as one of its own window hosts: with the
/// window host switch alone, which only the app's own launch passes. Anything
/// else on the command line makes it an ordinary launch.
pub(crate) fn starts_window_host(args: &[OsString]) -> bool {
    matches!(args, [_, mode] if mode == OSR_HOST_ARG)
}

pub(crate) fn run_window_host() {
    if let Err(error) = crate::osr::host::run() {
        crate::launch::bootstrap::show_failure("The application could not continue", &error);
        std::process::exit(1);
    }
}

pub(crate) fn require_app_id(config: &SabineWindowConfig) -> SabineResult<&str> {
    let app_id = config.app_id.as_deref().map(str::trim).unwrap_or_default();
    sabine_service::valid_app_id(app_id)
        .then_some(app_id)
        .ok_or_else(|| SabineError::CreationFailed {
            message: "app_id is required and may contain only letters, digits, dots, hyphens, and underscores"
                .to_string(),
        })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{OSR_HOST_ARG, starts_window_host};

    fn launch(args: &[&str]) -> bool {
        starts_window_host(&args.iter().map(OsString::from).collect::<Vec<_>>())
    }

    #[test]
    fn only_the_app_s_own_launch_starts_a_window_host() {
        assert!(launch(&["notes", OSR_HOST_ARG]));
        assert!(!launch(&["notes"]));
        assert!(!launch(&["notes", OSR_HOST_ARG, "/tmp/window.json"]));
        assert!(!launch(&["notes", "report.txt", OSR_HOST_ARG]));
    }
}
