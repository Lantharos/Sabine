use std::{fs, io, path::Path};

/// Copies the parts of a runtime that apps load, leaving out the build SDK,
/// leases, and hosts compiled against it.
pub fn copy_runtime_payload(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    let source = source.canonicalize()?;
    for name in [
        "Release",
        "Resources",
        "Chromium Embedded Framework.framework",
        ".sabine-version",
        "LICENSE.txt",
        "README.txt",
    ] {
        let path = source.join(name);
        if path.is_dir() {
            copy_runtime_recursive(&path, &destination.join(name), &source)?;
        } else if path.is_file() {
            fs::copy(&path, destination.join(name))?;
        }
    }
    Ok(())
}

fn copy_runtime_recursive(source: &Path, destination: &Path, root: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        if matches!(
            name_text.as_ref(),
            ".leases" | ".sabine-host-build" | ".sabine-hosts"
        ) || name_text.ends_with(".installing")
        {
            continue;
        }
        let source_path = entry.path();
        let destination_path = destination.join(name);
        if entry.file_type()?.is_symlink() {
            let target = fs::read_link(&source_path)?;
            if target.is_absolute() || !source_path.canonicalize()?.starts_with(root) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "runtime symlink escapes the bundle",
                ));
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, destination_path)?;
            #[cfg(windows)]
            if source_path.is_dir() {
                std::os::windows::fs::symlink_dir(target, destination_path)?;
            } else {
                std::os::windows::fs::symlink_file(target, destination_path)?;
            }
        } else if source_path.is_dir() {
            copy_runtime_recursive(&source_path, &destination_path, root)?;
        } else {
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}
