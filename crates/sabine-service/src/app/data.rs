use std::path::PathBuf;

pub fn browser_profile_path(profile_key: &str) -> PathBuf {
    browser_profiles_root()
        .join(format!("{:016x}", stable_hash(&[profile_key])))
        .join("profile")
}

fn user_cache_home() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(path) = std::env::var_os("LOCALAPPDATA").filter(|path| !path.is_empty()) {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME").filter(|path| !path.is_empty()) {
        return PathBuf::from(home).join("Library/Caches");
    }
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
}

fn stable_hash(parts: &[&str]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub(crate) fn browser_profiles_root() -> PathBuf {
    user_cache_home().join("sabine/profiles")
}
