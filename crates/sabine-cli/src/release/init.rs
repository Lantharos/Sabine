use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use sabine_service::public_key_from_private;

use super::new_private_key;
use super::workflow::{Release, WorkflowChange, write_release_workflow};

const SECRET: &str = "SABINE_UPDATE_SIGNING_KEY";

/// Sets up signed GitHub releases for the app in the current folder and
/// returns its update public key.
pub(crate) fn initialize_github_release(
    repository: Option<&str>,
    rotate_key: bool,
) -> Result<String, String> {
    let project = std::env::current_dir().map_err(|error| error.to_string())?;
    let manifest = project.join("Sabine.toml");
    if !manifest.is_file() && !project.join("Cargo.toml").is_file() {
        return Err("run sabine release-init from the app's project folder".to_string());
    }
    let repository = resolve_repository(repository)?;
    let release = Release::current()?;
    let mut document = read_manifest(&manifest)?;
    if let Some(existing) = existing_key(&repository, &document)? {
        if !rotate_key {
            return Err(format!(
                "{existing} already holds this app's update signing key, and installed copies only accept releases signed with it. Pass --rotate-key to replace it anyway"
            ));
        }
        eprintln!(
            "Replacing the update signing key. Copies built with the old key reject every release from now on; people have to reinstall them."
        );
    }
    let private_key = new_private_key()?;
    let saved = save_private_key(&repository, &private_key)?;
    println!(
        "saved the update signing key to {}; keep it somewhere safe, as installed copies only accept releases signed with it",
        saved.display()
    );
    store_secret(&repository, &private_key)?;
    enable_immutable_releases(&repository)?;
    let public_key = public_key_from_private(&private_key).map_err(|error| error.to_string())?;
    configure_updates(&mut document, &repository, &public_key);
    std::fs::write(&manifest, document.to_string()).map_err(|error| error.to_string())?;
    report_workflow(&project, &release)?;
    Ok(public_key)
}

fn resolve_repository(repository: Option<&str>) -> Result<String, String> {
    repository
        .map(str::to_string)
        .or_else(|| std::env::var("GITHUB_REPOSITORY").ok())
        .or_else(|| {
            gh_output(&[
                "repo",
                "view",
                "--json",
                "nameWithOwner",
                "--jq",
                ".nameWithOwner",
            ])
        })
        .filter(|repository| repository.split_once('/').is_some())
        .ok_or_else(|| {
            "could not determine GitHub repository; pass --repository owner/name".to_string()
        })
}

fn read_manifest(path: &Path) -> Result<toml_edit::DocumentMut, String> {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    source
        .parse()
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

/// Where a signing key for this app already lives, if one does.
fn existing_key(
    repository: &str,
    document: &toml_edit::DocumentMut,
) -> Result<Option<String>, String> {
    let secrets = gh_output(&[
        "secret", "list", "--repo", repository, "--json", "name", "--jq", ".[].name",
    ])
    .ok_or_else(|| format!("could not list the Actions secrets of {repository}"))?;
    if secrets.lines().any(|name| name == SECRET) {
        return Ok(Some(format!("The {SECRET} secret of {repository}")));
    }
    let public_key = document
        .get("updates")
        .and_then(|updates| updates.get("public_key"))
        .and_then(toml_edit::Item::as_str)
        .is_some_and(|key| !key.trim().is_empty());
    Ok(public_key.then(|| "[updates] public_key in Sabine.toml".to_string()))
}

/// Keeps a copy of the private key that only this user can read, outside the
/// project so it never ends up in the repository.
fn save_private_key(repository: &str, private_key: &str) -> Result<PathBuf, String> {
    let directory = dirs::config_dir()
        .ok_or("this computer has no configuration folder for the signing key")?
        .join("sabine-keys");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let path = directory.join(format!("{}-{created}.key", repository.replace('/', "-")));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(&path)
        .and_then(|mut file| file.write_all(private_key.as_bytes()))
        .map_err(|error| {
            format!(
                "could not save the signing key to {}: {error}",
                path.display()
            )
        })?;
    Ok(path)
}

fn store_secret(repository: &str, private_key: &str) -> Result<(), String> {
    let mut child = Command::new("gh")
        .args(["secret", "set", SECRET, "--repo", repository])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start GitHub CLI: {error}"))?;
    child
        .stdin
        .take()
        .expect("the secret input is piped")
        .write_all(private_key.as_bytes())
        .map_err(|error| error.to_string())?;
    let status = child.wait().map_err(|error| error.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "could not configure the app update signing secret".to_string())
}

fn enable_immutable_releases(repository: &str) -> Result<(), String> {
    let status = Command::new("gh")
        .args([
            "api",
            "--method",
            "PUT",
            "-H",
            "X-GitHub-Api-Version: 2026-03-10",
            &format!("repos/{repository}/immutable-releases"),
        ])
        .status()
        .map_err(|error| format!("failed to enable immutable releases: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "could not enable immutable GitHub Releases".to_string())
}

fn configure_updates(document: &mut toml_edit::DocumentMut, repository: &str, public_key: &str) {
    let updates = document["updates"].or_insert(toml_edit::table());
    if let Some(previous) = updates["provider"]
        .as_str()
        .filter(|name| *name != "github")
    {
        println!("moved [updates] from provider = \"{previous}\" to GitHub releases");
    }
    if let Some(table) = updates.as_table_like_mut() {
        table.remove("url");
    }
    updates["provider"] = toml_edit::value("github");
    updates["repository"] = toml_edit::value(repository);
    updates["public_key"] = toml_edit::value(public_key);
}

fn report_workflow(project: &Path, release: &Release) -> Result<(), String> {
    let change = write_release_workflow(project, release).map_err(|error| error.to_string())?;
    match change {
        WorkflowChange::Created => println!("wrote .github/workflows/release.yml"),
        WorkflowChange::Updated => println!(
            "moved .github/workflows/release.yml onto Sabine {}",
            sabine_service::SABINE_VERSION
        ),
        WorkflowChange::Current => {}
        WorkflowChange::Custom => println!(
            "left .github/workflows/release.yml as it is; it doesn't call Sabine's release workflow"
        ),
    }
    Ok(())
}

fn gh_output(args: &[&str]) -> Option<String> {
    Command::new("gh")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_an_http_source_to_github_drops_its_url() {
        let mut document: toml_edit::DocumentMut =
            "[updates]\nprovider = \"http\"\nurl = \"https://example.com/tern\"\npolicy = \"notify\"\n"
                .parse()
                .unwrap();
        configure_updates(&mut document, "example/tern", "key");
        let updates = &document["updates"];
        assert!(updates.get("url").is_none());
        assert_eq!(updates["provider"].as_str(), Some("github"));
        assert_eq!(updates["policy"].as_str(), Some("notify"));
    }
}
