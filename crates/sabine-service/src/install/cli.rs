use std::process::{Command, Stdio};

use sabine_runtime::{sabine_data_dir, system_dir};

use super::*;

/// Set when a `sabine` must stay the version it is, such as in a build that
/// pins Sabine, instead of handing over to a newer installed CLI.
pub const CLI_PINNED_ENV: &str = "SABINE_CLI_PINNED";

/// The CLI of the newest Sabine on this machine, the user's or the machine's,
/// when that Sabine is newer than this CLI. A version whose CLI was left out
/// of its update leaves the one before it in charge.
pub fn newer_installed_cli() -> Option<PathBuf> {
    newest_cli_in(&[sabine_data_dir(), system_dir()], crate::SABINE_VERSION)
}

fn newest_cli_in(roots: &[PathBuf], running: &str) -> Option<PathBuf> {
    roots
        .iter()
        .filter_map(|root| {
            let state = read_installation_state_in(root)?;
            let versions = state::versions_dir_in(root);
            [Some(state.active), state.previous]
                .into_iter()
                .flatten()
                .find_map(|version| {
                    let cli = versions.join(&version).join(cli_binary_name());
                    (cli.is_file() && crate::types::version_is_newer(&version, running))
                        .then_some((version, cli))
                })
        })
        .reduce(|newest, other| {
            if crate::types::version_is_newer(&other.0, &newest.0) {
                other
            } else {
                newest
            }
        })
        .map(|(_, cli)| cli)
}

/// Leaves out of an update a CLI that doesn't start on this machine, which
/// lacks what a developer's machine has. Sabine updates without it and the
/// CLI already installed keeps working.
pub(super) fn keep_cli_if_it_runs(cli: &Path, version: &str) -> ServiceResult<()> {
    if !cli.is_file() {
        return Ok(());
    }
    sabine_runtime::make_executable(cli)?;
    if cli_reports_version(cli, version) {
        return Ok(());
    }
    sabine_runtime::report_error(
        "maintenance",
        format!("the CLI of Sabine {version} doesn't start here; keeping the installed one"),
    );
    Ok(fs::remove_file(cli)?)
}

fn cli_reports_version(cli: &Path, version: &str) -> bool {
    let mut command = Command::new(cli);
    sabine_runtime::configure_background_command(&mut command);
    command
        .arg("--version")
        .env(CLI_PINNED_ENV, "1")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line == format!("Sabine CLI: {version}"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(root: &Path, version: &str, with_cli: bool) {
        let directory = state::versions_dir_in(root).join(version);
        fs::create_dir_all(&directory).unwrap();
        if with_cli {
            fs::write(directory.join(cli_binary_name()), "").unwrap();
        }
        state::write_installation_state_in(
            root,
            &SystemInstallationState {
                schema: 1,
                active: version.to_string(),
                previous: None,
                compatibility: state::compatibility_for_version(version).unwrap(),
                previous_compatibility: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn the_newest_installed_cli_wins_over_an_older_one() {
        let base = tempfile::tempdir().unwrap();
        let (user, machine) = (base.path().join("user"), base.path().join("machine"));
        install(&user, "0.80", true);
        install(&machine, "0.79", true);
        let roots = [user.clone(), machine.clone()];
        assert_eq!(
            newest_cli_in(&roots, "0.78"),
            Some(
                state::versions_dir_in(&user)
                    .join("0.80")
                    .join(cli_binary_name())
            )
        );
        assert_eq!(newest_cli_in(&roots, "0.80"), None);
    }

    #[test]
    fn a_version_without_a_cli_leaves_the_running_one() {
        let base = tempfile::tempdir().unwrap();
        install(base.path(), "0.80", false);
        assert_eq!(newest_cli_in(&[base.path().to_path_buf()], "0.78"), None);
    }
}
