use std::{fs, path::Path};

use sabine_service::manifest::SabineFile;

/// `Sabine.toml` in `source_dir`, or the defaults when there is none.
pub(crate) fn read_sabine_file(source_dir: &Path) -> Result<SabineFile, String> {
    let path = source_dir.join("Sabine.toml");
    if !path.exists() {
        return Ok(SabineFile::default());
    }
    let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let (file, unknown) = SabineFile::parse(&text)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    if !unknown.is_empty() {
        return Err(format!(
            "{} has keys Sabine doesn't know: {}. Check their spelling",
            path.display(),
            unknown.join(", ")
        ));
    }
    Ok(file)
}
