//! `sabine new` project templates.

mod app;
mod frontends;
mod notes;

use std::{
    fs,
    io::{BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    release::workflow::{Release, write_release_workflow},
    util::sanitize_id,
};
use app::write_app_template;
use frontends::FRONTENDS;
use notes::write_notes_template;

const GITIGNORE: &str = "/target\n/node_modules\n/dist\n/ui/dist\n";

pub fn new_app(name: &str, template: &str, id: Option<String>) -> Result<ExitCode, String> {
    let frontend = FRONTENDS
        .iter()
        .find(|frontend| frontend.template == template);
    if frontend.is_none() && template != "notes" {
        let templates = FRONTENDS
            .iter()
            .map(|frontend| frontend.template)
            .chain(["notes"])
            .collect::<Vec<_>>();
        return Err(format!(
            "unknown template `{template}`; available templates: {}",
            templates.join(", ")
        ));
    }
    if !is_valid_name(name) {
        return Err("app name must contain only letters, numbers, hyphens, and underscores".into());
    }
    let root = PathBuf::from(name);
    if root.exists() {
        return Err(format!("{} already exists", root.display()));
    }
    let id = match id {
        Some(id) => id,
        None => ask_id(name)?,
    };
    if !sabine_service::valid_app_id(&id) {
        return Err(format!(
            "app id {id:?} may contain only letters, digits, dots, hyphens and underscores"
        ));
    }
    match frontend {
        Some(frontend) => write_app_template(&root, name, &id, frontend),
        None => write_notes_template(&root, name, &id),
    }
    .and_then(|()| fs::write(root.join(".gitignore"), GITIGNORE))
    .and_then(|()| write_default_icon(&root))
    .map_err(|error| format!("failed to create app: {error}"))?;
    match Release::current() {
        Ok(release) => {
            write_release_workflow(&root, &release).map_err(|error| error.to_string())?;
        }
        Err(error) => eprintln!(
            "left out the release workflow, which `sabine release-init` writes later: {error}"
        ),
    }
    println!("Created Sabine app {id} at {}", root.display());
    println!("Run it with: cd {name} && sabine dev");
    Ok(ExitCode::SUCCESS)
}

/// The reverse-DNS identifier installed copies of the app are known by.
fn ask_id(name: &str) -> Result<String, String> {
    let suggestion = format!("com.example.{}", sanitize_id(name));
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return Ok(suggestion);
    }
    print!("App identifier ({suggestion}): ");
    std::io::stdout()
        .flush()
        .map_err(|error| error.to_string())?;
    let mut answer = String::new();
    stdin
        .lock()
        .read_line(&mut answer)
        .map_err(|error| error.to_string())?;
    let answer = answer.trim();
    Ok(if answer.is_empty() {
        suggestion
    } else {
        answer.to_string()
    })
}

fn write_default_icon(root: &Path) -> std::io::Result<()> {
    let assets = root.join("assets");
    fs::create_dir_all(&assets)?;
    fs::write(
        assets.join("icon.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="112" fill="#17191f"/>
  <path d="M354 153c-25-24-59-37-99-37-65 0-111 34-111 84 0 54 47 70 104 82 47 10 69 18 69 42 0 25-24 39-61 39-43 0-80-16-108-47l-35 38c35 39 84 59 140 59 69 0 116-35 116-88 0-55-43-73-106-87-43-9-67-17-67-39 0-22 22-35 58-35 34 0 64 11 88 33z" fill="#f4f5f7"/>
</svg>
"##,
    )
}

fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(crate) fn cargo_toml(name: &str) -> String {
    let sabine_dep = format!(
        "{{ git = \"https://github.com/Lantharos/Sabine\", tag = \"v{}\", package = \"sabine\" }}",
        sabine_service::SABINE_VERSION
    );
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[dependencies]
sabine = {sabine_dep}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
specta = {{ version = "=2.0.0-rc.27", features = ["derive"] }}

[profile.release]
lto = "fat"
codegen-units = 1
strip = true

[workspace]
"#
    )
}
