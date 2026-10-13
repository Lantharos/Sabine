use std::path::{Component, Path, PathBuf};

/// The machine-wide Sabine directory. An administrator or OS image fills it
/// with `sabine runtime install --system-wide`; every user reads it and
/// nothing writes to it while apps run.
pub fn system_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
            .join("Sabine")
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Library/Application Support/Sabine")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/usr/lib/sabine")
    }
}

pub fn system_runtime_path() -> PathBuf {
    system_dir().join("runtimes").join("cef")
}

/// Where `path` lands when a system-wide installation is staged into `root`,
/// such as an OS image or a package build directory.
pub fn path_in_root(root: Option<&Path>, path: &Path) -> PathBuf {
    let Some(root) = root else {
        return path.to_path_buf();
    };
    root.join(
        path.components()
            .filter(|component| matches!(component, Component::Normal(_)))
            .collect::<PathBuf>(),
    )
}

/// The per-user directory that holds every piece of shared Sabine state:
/// the service binaries, runtimes, app registry, downloads, and logs.
pub fn sabine_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("AppData").join("Local"))
            .join("Sabine")
    }
    #[cfg(target_os = "macos")]
    {
        home_dir().join("Library/Application Support/Sabine")
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".local/share"))
            .join("sabine")
    }
}

fn home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(target_os = "windows"))]
    let home = std::env::var_os("HOME");
    home.map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

pub fn user_runtime_path() -> PathBuf {
    sabine_data_dir().join("runtimes").join("cef")
}

pub fn bundled_runtime_path(app_dir: &Path) -> PathBuf {
    app_dir.join("runtimes").join("cef")
}

pub fn runtime_version_path(version: &str) -> PathBuf {
    user_runtime_path().join(format!("{version}-minimal"))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn runtime_execution_path(runtime: &Path) -> std::io::Result<PathBuf> {
    let fingerprint = crate::Fingerprint::default()
        .path(&runtime.canonicalize()?)
        .finish();
    Ok(sabine_data_dir().join("executions").join(fingerprint))
}
