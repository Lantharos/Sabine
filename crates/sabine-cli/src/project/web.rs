use std::path::{Path, PathBuf};

use super::{ConfigOverrides, file::WebSection};

#[derive(Debug)]
pub(crate) struct WebBundle {
    pub root: PathBuf,
    pub dist: PathBuf,
    pub entry: PathBuf,
    pub build_command: Option<String>,
    pub has_local_assets: bool,
    pub url: Option<String>,
    pub allowed_origins: Vec<String>,
    pub local_files: bool,
}

impl WebBundle {
    pub fn assets(&self) -> Result<Option<(&Path, PathBuf)>, String> {
        if !self.has_local_assets {
            return Ok(None);
        }
        let source = self.dist.as_path();
        let entry = self
            .entry
            .strip_prefix(source)
            .ok()
            .filter(|path| !path.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .or_else(|| self.entry.file_name().map(PathBuf::from))
            .ok_or_else(|| "web entry has no filename".to_string())?;
        if !source.join(&entry).is_file() {
            return Err(format!(
                "web entry was not found at {}; build the web assets before bundling",
                source.join(&entry).display()
            ));
        }
        Ok(Some((source, entry)))
    }
}

pub(super) fn resolve_web(
    source_dir: &Path,
    config: &WebSection,
    overrides: &ConfigOverrides,
) -> Result<Option<WebBundle>, String> {
    let url = config.url.clone();
    let dev_url = config.dev_url.clone();
    let allowed_origins = config.allowed_origins.clone();
    let configured_root = overrides
        .web_root
        .clone()
        .or_else(|| config.root.as_ref().map(PathBuf::from));
    let configured_entry = config.entry.as_ref().map(PathBuf::from);
    let configured_dist = overrides
        .web_dist
        .clone()
        .or_else(|| config.dist.as_ref().map(PathBuf::from));
    let has_explicit_local_assets =
        configured_root.is_some() || configured_entry.is_some() || configured_dist.is_some();
    let has_remote_url = url.is_some() || dev_url.is_some() || !allowed_origins.is_empty();
    let package_root = configured_root
        .as_deref()
        .map(|root| source_dir.join(root))
        .or_else(|| {
            (!has_remote_url)
                .then(|| detect_package_root(source_dir))
                .flatten()
        });
    let entry = config.entry.as_ref().map(PathBuf::from).or_else(|| {
        (!has_remote_url)
            .then(|| default_web_entry(source_dir))
            .flatten()
    });
    let dist = configured_dist.map(|path| source_dir.join(path));

    if package_root.is_none() && entry.is_none() && dist.is_none() && !has_remote_url {
        return Ok(None);
    }

    let root = package_root
        .or_else(|| {
            entry
                .as_ref()
                .and_then(|entry| source_dir.join(entry).parent().map(Path::to_path_buf))
        })
        .unwrap_or_else(|| source_dir.join("ui"));
    let entry = entry
        .map(|entry| source_dir.join(entry))
        .unwrap_or_else(|| root.join("index.html"));
    let dist = dist.unwrap_or_else(|| {
        if root.join("dist").exists() || root.join("package.json").exists() {
            root.join("dist")
        } else {
            entry.parent().unwrap_or(&root).to_path_buf()
        }
    });
    let build_command = overrides
        .web_build
        .clone()
        .or_else(|| config.build.clone())
        .or_else(|| {
            if has_explicit_local_assets || !has_remote_url {
                detect_web_build_command(&root)
            } else {
                None
            }
        });
    let has_local_assets = has_explicit_local_assets
        || entry.is_file()
        || dist.exists()
        || root.join("package.json").is_file() && !has_remote_url;

    Ok(Some(WebBundle {
        root,
        dist,
        entry,
        build_command,
        has_local_assets,
        url,
        allowed_origins,
        local_files: config.local_files,
    }))
}

fn detect_package_root(source_dir: &Path) -> Option<PathBuf> {
    crate::dev::web_detect::detect_package_root(source_dir)
}

fn default_web_entry(source_dir: &Path) -> Option<PathBuf> {
    [
        "ui/index.html",
        "web/index.html",
        "frontend/index.html",
        "index.html",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|entry| source_dir.join(entry).is_file())
}

fn detect_web_build_command(root: &Path) -> Option<String> {
    crate::dev::web_detect::detect_web_build_command(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_only_web_config_does_not_require_local_assets() {
        let source = PathBuf::from("/tmp/sabine-remote-only-config-test");
        let web = resolve_web(
            &source,
            &WebSection {
                url: Some("https://raday.lantharos.com".to_string()),
                allowed_origins: vec!["https://api.lantharos.com".to_string()],
                ..WebSection::default()
            },
            &ConfigOverrides::default(),
        )
        .unwrap()
        .unwrap();

        assert!(!web.has_local_assets);
        assert_eq!(web.url.as_deref(), Some("https://raday.lantharos.com"));
        assert_eq!(
            web.allowed_origins,
            vec!["https://api.lantharos.com".to_string()]
        );
    }

    #[test]
    fn explicit_local_assets_are_kept_for_site_backed_apps() {
        let source = PathBuf::from("/tmp/sabine-site-backed-config-test");
        let web = resolve_web(
            &source,
            &WebSection {
                root: Some("ui".to_string()),
                dist: Some("ui/dist".to_string()),
                entry: Some("ui/dist/index.html".to_string()),
                url: Some("https://raday.lantharos.com".to_string()),
                ..WebSection::default()
            },
            &ConfigOverrides::default(),
        )
        .unwrap()
        .unwrap();

        assert!(web.has_local_assets);
        assert_eq!(web.root, source.join("ui"));
        assert_eq!(web.dist, source.join("ui/dist"));
        assert_eq!(web.entry, source.join("ui/dist/index.html"));
    }
}
