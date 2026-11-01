//! Notes app template, made from the `sabine-notes` example.

use std::{fs, path::Path};

use serde_json::{Value, json};

use super::{cargo_toml, sanitize_id};

macro_rules! example {
    ($path:literal) => {
        concat!("../../../../examples/sabine-notes/", $path)
    };
}

const FILES: [(&str, &str); 6] = [
    ("src/main.rs", include_str!(example!("src/main.rs"))),
    ("ui/index.html", include_str!(example!("ui/index.html"))),
    ("ui/styles.css", include_str!(example!("ui/styles.css"))),
    ("ui/app.ts", include_str!(example!("ui/app.ts"))),
    ("ui/bindings.ts", include_str!(example!("ui/bindings.ts"))),
    ("ui/fonts/OFL.txt", include_str!(example!("ui/fonts/OFL.txt"))),
];

const FONTS: [(&str, &[u8]); 2] = [
    (
        "ui/fonts/instrument-sans-latin.woff2",
        include_bytes!(example!("ui/fonts/instrument-sans-latin.woff2")),
    ),
    (
        "ui/fonts/instrument-sans-latin-ext.woff2",
        include_bytes!(example!("ui/fonts/instrument-sans-latin-ext.woff2")),
    ),
];

pub(super) fn write_notes_template(root: &Path, name: &str) -> std::io::Result<()> {
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("ui/fonts"))?;
    fs::write(root.join("Cargo.toml"), cargo_toml(name))?;
    fs::write(root.join("Sabine.toml"), notes_sabine_toml(name))?;
    fs::write(root.join("package.json"), notes_package_json(name))?;
    fs::write(root.join("tsconfig.json"), notes_tsconfig())?;
    for (path, contents) in FILES {
        fs::write(root.join(path), contents)?;
    }
    for (path, contents) in FONTS {
        fs::write(root.join(path), contents)?;
    }
    Ok(())
}

/// The example's manifest with the new app's identity.
fn notes_sabine_toml(name: &str) -> String {
    let (_, web) = include_str!(example!("Sabine.toml"))
        .split_once("[web]")
        .expect("the example's manifest has a [web] section");
    format!(
        "[app]\nid = \"dev.sabine.{}\"\nname = \"{name}\"\nversion = \"0.1.0\"\nicon = \"assets/icon.svg\"\n\n[web]{web}",
        sanitize_id(name)
    )
}

/// The example's scripts, with the Sabine package that the example finds in
/// this repository instead.
fn notes_package_json(name: &str) -> String {
    let mut package = example_json(include_str!(example!("package.json")));
    package["name"] = name.into();
    package["dependencies"] = json!({
        "@lantharos/sabine": format!("github:Lantharos/Sabine#v{}", sabine_service::SABINE_VERSION),
    });
    pretty(&package)
}

fn notes_tsconfig() -> String {
    let mut tsconfig = example_json(include_str!(example!("tsconfig.json")));
    tsconfig["compilerOptions"]
        .as_object_mut()
        .expect("the example's tsconfig has compilerOptions")
        .remove("paths");
    pretty(&tsconfig)
}

fn example_json(text: &str) -> Value {
    serde_json::from_str(text).expect("the example's JSON files parse")
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("JSON values serialize") + "\n"
}
