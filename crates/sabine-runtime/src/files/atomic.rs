use std::{
    fs,
    io::{self, Write},
    path::Path,
};

/// Replaces `path` with `contents` so readers see either the old file or the
/// complete new one, even across a crash.
fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("new");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    replace_file(&temporary, path)
}

pub fn write_json_atomic(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}

/// Moves a completed `temporary` file over `destination` and makes the rename durable.
pub fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)?;
    match destination.parent() {
        Some(parent) => sync_directory(parent),
        None => Ok(()),
    }
}

/// Flushes a directory's entries, so renames inside it survive a crash.
pub fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(windows)]
    let _ = path;
    Ok(())
}
