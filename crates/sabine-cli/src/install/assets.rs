use std::{
    fs,
    path::{Path, PathBuf},
};

/// Copies the app's icon into its install folder, where launchers find it
/// even while the checkout moves.
pub fn stage_icon(app_dir: &Path, icon: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let Some(icon) = icon.filter(|icon| icon.is_file()) else {
        return Ok(None);
    };
    let icons_dir = app_dir.join("icons");
    sabine_runtime::remove_directory_if_present(&icons_dir).map_err(|error| error.to_string())?;
    fs::create_dir_all(&icons_dir).map_err(|error| error.to_string())?;
    let destination = icons_dir.join(icon.file_name().unwrap_or_default());
    fs::copy(icon, &destination).map_err(|error| error.to_string())?;
    Ok(Some(destination))
}
