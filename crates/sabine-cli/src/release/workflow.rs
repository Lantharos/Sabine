use std::{fs, io, path::Path, process::Command};

const PATH: &str = ".github/workflows/release.yml";
const REPOSITORY: &str = "https://github.com/Lantharos/Sabine";
const REUSABLE_WORKFLOW: &str = "Lantharos/Sabine/.github/workflows/release-app.yml@";
const VERSION_INPUT: &str = "sabine_version:";
const INHERITED_SECRETS: &str = "secrets: inherit";
const SECRETS: [&str; 9] = [
    "SABINE_UPDATE_SIGNING_KEY",
    "MACOS_CERTIFICATE",
    "MACOS_CERTIFICATE_PASSWORD",
    "MACOS_SIGNING_IDENTITY",
    "APPLE_API_KEY",
    "APPLE_API_KEY_ID",
    "APPLE_API_ISSUER",
    "WINDOWS_CERTIFICATE",
    "WINDOWS_CERTIFICATE_PASSWORD",
];

pub(crate) enum WorkflowChange {
    Created,
    Updated,
    Current,
    /// The file exists but doesn't call Sabine's release workflow.
    Custom,
}

/// This CLI's Sabine release, and the commit its tag points to, which the
/// release workflow is pinned to.
pub(crate) struct Release {
    tag: String,
    commit: String,
}

impl Release {
    pub(crate) fn current() -> Result<Self, String> {
        let tag = format!("v{}", sabine_service::SABINE_VERSION);
        let reference = format!("refs/tags/{tag}");
        let output = Command::new("git")
            .args(["ls-remote", REPOSITORY, &reference])
            .arg(format!("{reference}^{{}}"))
            .output()
            .map_err(|error| format!("could not run git: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "could not look up Sabine {tag}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let refs = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(commit, name)| (commit.to_string(), name.ends_with("^{}")))
            .collect::<Vec<_>>();
        let commit = refs
            .iter()
            .find(|(_, peeled)| *peeled)
            .or_else(|| refs.first())
            .map(|(commit, _)| commit.clone())
            .ok_or_else(|| format!("Sabine {tag} isn't published on GitHub yet"))?;
        Ok(Self { tag, commit })
    }
}

/// Writes the release workflow into `project`, or moves an existing one onto
/// `release`.
pub(crate) fn write_release_workflow(
    project: &Path,
    release: &Release,
) -> io::Result<WorkflowChange> {
    let path = project.join(PATH);
    let existing = match fs::read_to_string(&path) {
        Ok(existing) => existing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path.parent().expect("the workflow path has a folder"))?;
            fs::write(&path, release_workflow(release))?;
            return Ok(WorkflowChange::Created);
        }
        Err(error) => return Err(error),
    };
    if !existing.contains(REUSABLE_WORKFLOW) {
        return Ok(WorkflowChange::Custom);
    }
    let updated = on_release(&existing, release);
    if updated == existing {
        return Ok(WorkflowChange::Current);
    }
    fs::write(&path, updated)?;
    Ok(WorkflowChange::Updated)
}

fn release_workflow(release: &Release) -> String {
    format!(
        r#"name: Release

on:
  push:
    tags:
      - "v*"

permissions:
  contents: write
  id-token: write
  attestations: write

jobs:
  release:
    uses: {REUSABLE_WORKFLOW}{commit} # {tag}
    with:
      sabine_version: {tag}
      # MSI packages need the WiX 7 license: review https://docs.firegiant.com/wix/osmf/ and set this to true
      accept_wix_eula: false
    secrets:
{secrets}"#,
        commit = release.commit,
        tag = release.tag,
        secrets = named_secrets("      "),
    )
}

fn named_secrets(indent: &str) -> String {
    SECRETS
        .iter()
        .map(|secret| format!("{indent}{secret}: ${{{{ secrets.{secret} }}}}\n"))
        .collect()
}

fn on_release(workflow: &str, release: &Release) -> String {
    let mut updated = String::new();
    for line in workflow.lines() {
        if let Some((before, _)) = line.split_once(REUSABLE_WORKFLOW) {
            updated.push_str(&format!(
                "{before}{REUSABLE_WORKFLOW}{} # {}\n",
                release.commit, release.tag
            ));
        } else if let Some((before, _)) = line.split_once(VERSION_INPUT) {
            updated.push_str(&format!("{before}{VERSION_INPUT} {}\n", release.tag));
        } else if let Some(indent) = line.strip_suffix(INHERITED_SECRETS)
            && indent.trim().is_empty()
        {
            updated.push_str(&format!("{indent}secrets:\n"));
            updated.push_str(&named_secrets(&format!("{indent}  ")));
        } else {
            updated.push_str(line);
            updated.push('\n');
        }
    }
    if !workflow.ends_with('\n') {
        updated.pop();
    }
    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_an_existing_workflow_onto_a_release() {
        let release = Release {
            tag: "v0.95".into(),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
        };
        let current = release_workflow(&release);
        let old = current
            .replace(&format!("{} # {}", release.commit, release.tag), "v0.1")
            .replace(&release.tag, "v0.1")
            .replace(
                &format!("    secrets:\n{}", named_secrets("      ")),
                "    secrets: inherit\n",
            );
        assert_eq!(on_release(&old, &release), current);
    }
}
