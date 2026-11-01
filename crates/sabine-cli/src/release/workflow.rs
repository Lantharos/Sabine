use std::{fs, io, path::Path};

const PATH: &str = ".github/workflows/release.yml";
const REUSABLE_WORKFLOW: &str = "Lantharos/Sabine/.github/workflows/release-app.yml@";
const VERSION_INPUT: &str = "sabine_version:";

pub(crate) enum WorkflowChange {
    Created,
    Updated,
    Current,
    /// The file exists but doesn't call Sabine's release workflow.
    Custom,
}

/// Writes the release workflow into `project`, or moves an existing one onto
/// this Sabine release.
pub(crate) fn write_release_workflow(project: &Path) -> io::Result<WorkflowChange> {
    let path = project.join(PATH);
    let existing = match fs::read_to_string(&path) {
        Ok(existing) => existing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path.parent().expect("the workflow path has a folder"))?;
            fs::write(&path, release_workflow())?;
            return Ok(WorkflowChange::Created);
        }
        Err(error) => return Err(error),
    };
    if !existing.contains(REUSABLE_WORKFLOW) {
        return Ok(WorkflowChange::Custom);
    }
    let updated = on_this_release(&existing);
    if updated == existing {
        return Ok(WorkflowChange::Current);
    }
    fs::write(&path, updated)?;
    Ok(WorkflowChange::Updated)
}

fn release_workflow() -> String {
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
    uses: {REUSABLE_WORKFLOW}{tag}
    with:
      sabine_version: {tag}
      # Building the MSI package accepts the WiX 7 license: https://docs.firegiant.com/wix/osmf/
      accept_wix_eula: true
    secrets: inherit
"#,
        tag = sabine_tag()
    )
}

fn on_this_release(workflow: &str) -> String {
    let tag = sabine_tag();
    let mut updated = workflow
        .lines()
        .map(|line| {
            if let Some((before, _)) = line.split_once(REUSABLE_WORKFLOW) {
                format!("{before}{REUSABLE_WORKFLOW}{tag}")
            } else if let Some((before, _)) = line.split_once(VERSION_INPUT) {
                format!("{before}{VERSION_INPUT} {tag}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if workflow.ends_with('\n') {
        updated.push('\n');
    }
    updated
}

fn sabine_tag() -> String {
    format!("v{}", sabine_service::SABINE_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_an_existing_workflow_onto_this_release() {
        let old = release_workflow().replace(&sabine_tag(), "v0.1");
        assert_eq!(on_this_release(&old), release_workflow());
    }
}
