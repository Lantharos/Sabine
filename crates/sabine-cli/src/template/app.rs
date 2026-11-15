//! Vite app templates.

use std::{fs, path::Path};

use super::{cargo_toml, frontends::Frontend};

pub(super) fn write_app_template(
    root: &Path,
    name: &str,
    id: &str,
    frontend: &Frontend,
) -> std::io::Result<()> {
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("ui/src"))?;
    fs::write(root.join("Cargo.toml"), cargo_toml(name))?;
    fs::write(root.join("Sabine.toml"), app_sabine_toml(name, id))?;
    fs::write(root.join("package.json"), package_json(name, frontend))?;
    fs::write(root.join("vite.config.ts"), vite_config(frontend))?;
    fs::write(root.join("tsconfig.json"), tsconfig(frontend))?;
    fs::write(root.join("src/main.rs"), MAIN_RS)?;
    fs::write(root.join("ui/index.html"), index_html(name, frontend))?;
    fs::write(
        root.join("ui/src/style.css"),
        include_str!("frontends/style.css"),
    )?;
    for (path, contents) in frontend.files {
        let file = Path::new(path)
            .file_name()
            .expect("template files have names");
        fs::write(root.join("ui/src").join(file), contents)?;
    }
    Ok(())
}

fn app_sabine_toml(name: &str, id: &str) -> String {
    format!(
        r#"[app]
id = "{id}"
name = "{name}"
version = "0.1.0"
icon = "assets/icon.svg"

[web]
root = "."
dist = "ui/dist"
entry = "ui/dist/index.html"
dev_port = 5173
build = "bun run build"

[bindings]
path = "ui/src/bindings.ts"
"#
    )
}

fn package_json(name: &str, frontend: &Frontend) -> String {
    let sabine = (
        "@lantharos/sabine",
        format!(
            "github:Lantharos/Sabine#v{}",
            sabine_service::SABINE_VERSION
        ),
    );
    let mut dependencies = frontend
        .dependencies
        .iter()
        .map(|(package, version)| (*package, version.to_string()))
        .chain([(sabine.0, sabine.1)])
        .collect::<Vec<_>>();
    dependencies.sort();
    let dev_dependencies = frontend
        .dev_dependencies
        .iter()
        .map(|(package, version)| (*package, version.to_string()))
        .collect::<Vec<_>>();
    format!(
        r#"{{
  "name": "{name}",
  "private": true,
  "type": "module",
  "scripts": {{
    "dev": "vite",
    "build": "{check} && vite build",
    "check": "{check}",
    "preview": "vite preview"
  }},
  "dependencies": {{
{}
  }},
  "devDependencies": {{
{}
  }}
}}
"#,
        json_entries(&dependencies),
        json_entries(&dev_dependencies),
        check = frontend.check,
    )
}

fn json_entries(entries: &[(&str, String)]) -> String {
    entries
        .iter()
        .map(|(key, value)| format!("    \"{key}\": \"{value}\""))
        .collect::<Vec<_>>()
        .join(",\n")
}

fn vite_config(frontend: &Frontend) -> String {
    let (import, plugins) = frontend
        .plugin
        .map(|(import, call)| (format!("{import}\n"), format!("  plugins: [{call}],\n")))
        .unwrap_or_default();
    format!(
        r#"import {{ defineConfig }} from "vite";
{import}
export default defineConfig({{
{plugins}  base: "./",
  root: "ui",
  server: {{
    port: 5173,
    strictPort: true,
  }},
  build: {{
    outDir: "dist",
    emptyOutDir: true,
  }},
}});
"#
    )
}

fn tsconfig(frontend: &Frontend) -> String {
    let options = frontend
        .compiler_options
        .iter()
        .map(|(option, value)| format!("    \"{option}\": \"{value}\",\n"))
        .collect::<String>();
    format!(
        r#"{{
  "compilerOptions": {{
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true,
    "skipLibCheck": true,
    "noEmit": true,
{options}    "types": ["vite/client"]
  }},
  "include": ["ui/src"]
}}
"#
    )
}

fn index_html(name: &str, frontend: &Frontend) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{name}</title>
  </head>
  <body>
    {body}
    <script type="module" src="/src/{entry}"></script>
  </body>
</html>
"#,
        body = frontend.body.replace("{name}", name),
        entry = frontend.entry,
    )
}

const MAIN_RS: &str = r#"#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use sabine::prelude::*;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Deserialize, Type)]
struct VersionRequest {}

#[derive(Serialize, Type)]
struct VersionResponse {
    version: &'static str,
}

fn main() {
    SabineWindow::main(|window| {
        Ok(window
            .app()
            .size(960, 640)
            .bridge_typed("app.version", |_request: VersionRequest| {
                Ok(VersionResponse {
                    version: env!("CARGO_PKG_VERSION"),
                })
            }))
    });
}
"#;
