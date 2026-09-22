use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Debug, Default)]
pub struct SourceMetadata {
    pub id: Option<String>,
    pub name: Option<String>,
    pub command: Option<String>,
    pub icon: Option<PathBuf>,
    pub mime_types: Vec<String>,
}
#[derive(Debug, Default)]
pub struct StagedAssets {
    pub icon: Option<PathBuf>,
}
pub fn stage(source: &Path, app_dir: &Path, icon: Option<&Path>) -> Result<StagedAssets, String> {
    Ok(StagedAssets {
        icon: stage_icon(source, app_dir, icon)?,
    })
}
pub fn metadata(source: &Path) -> SourceMetadata {
    let sabine = source.join("Sabine.toml");
    let mut metadata = SourceMetadata::default();
    if sabine.exists() {
        merge_sabine_metadata(source, &sabine, &mut metadata);
    }
    if metadata.icon.is_none() {
        metadata.icon = detect_icon(source);
    }
    metadata
}

fn stage_icon(
    source: &Path,
    app_dir: &Path,
    configured_icon: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let icon = configured_icon
        .map(Path::to_path_buf)
        .or_else(|| metadata(source).icon);
    let Some(icon) = icon.filter(|icon| icon.is_file()) else {
        return Ok(None);
    };
    let icons_dir = app_dir.join("icons");
    if icons_dir.exists() {
        fs::remove_dir_all(&icons_dir).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&icons_dir).map_err(|error| error.to_string())?;
    let destination = icons_dir.join(icon.file_name().unwrap_or_default());
    fs::copy(&icon, &destination).map_err(|error| error.to_string())?;
    Ok(Some(destination))
}

fn merge_sabine_metadata(source: &Path, path: &Path, metadata: &mut SourceMetadata) {
    let Ok(value) = read_toml(path) else {
        return;
    };
    if let Some(install) = value.get("install").and_then(toml::Value::as_table) {
        metadata.command = metadata
            .command
            .take()
            .or_else(|| string_value(install, "command"));
    }
    if let Some(app) = value.get("app").and_then(toml::Value::as_table) {
        metadata.id = metadata.id.take().or_else(|| string_value(app, "id"));
        metadata.name = metadata.name.take().or_else(|| string_value(app, "name"));
        metadata.command = metadata
            .command
            .take()
            .or_else(|| string_value(app, "command"));
        metadata.icon = metadata
            .icon
            .take()
            .or_else(|| string_value(app, "icon").map(|path| source.join(path)));
        if metadata.mime_types.is_empty() {
            metadata.mime_types = string_array(app, "mime_types");
        }
    }
    if let Some(desktop) = value.get("desktop").and_then(toml::Value::as_table)
        && metadata.mime_types.is_empty()
    {
        metadata.mime_types = string_array(desktop, "mime_types");
    }
}

fn read_toml(path: &Path) -> Result<toml::Table, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    text.parse::<toml::Table>()
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn string_value(table: &toml::Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_string)
}

fn string_array(table: &toml::Table, key: &str) -> Vec<String> {
    table
        .get(key)
        .and_then(toml::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(toml::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn detect_icon(source: &Path) -> Option<PathBuf> {
    [
        "static/icon.svg",
        "static/favicon.svg",
        "src/lib/assets/favicon.svg",
        "favicon.svg",
        "icon.svg",
        "icon.png",
        "icons/icon.svg",
        "icons/icon.png",
        "desktop/icons/icon.svg",
        "static/icon.png",
        "desktop/icons/icon.png",
    ]
    .iter()
    .map(|icon| source.join(icon))
    .find(|icon| icon.is_file())
}
