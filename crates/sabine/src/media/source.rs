use std::path::{Path, PathBuf};

use url::Url;

/// Which sources the page may open, mirroring what it may load itself.
pub(crate) struct SourcePolicy<'a> {
    pub(crate) web_root: Option<&'a Path>,
    pub(crate) local_files: bool,
}

/// Turns a page media URL into a URI GStreamer can open.
pub(super) fn resolve(source: &str, policy: &SourcePolicy) -> Result<String, String> {
    let url = Url::parse(source).map_err(|_| format!("{source} is not an absolute URL"))?;
    let path = match (url.scheme(), url.host_str()) {
        ("http" | "https", _) => return Ok(url.into()),
        ("sabine", Some("app")) => app_file(&url, policy.web_root)?,
        ("sabine", Some("file")) if policy.local_files => decoded_path(&url)?,
        ("file", _) if policy.local_files => url
            .to_file_path()
            .map_err(|_| format!("{source} is not a local file"))?,
        _ => return Err(format!("the page cannot play {source}")),
    };
    if !path.is_file() {
        return Err(format!("{} does not exist", path.display()));
    }
    Url::from_file_path(&path)
        .map(String::from)
        .map_err(|_| format!("{} is not an absolute path", path.display()))
}

fn app_file(url: &Url, web_root: Option<&Path>) -> Result<PathBuf, String> {
    let root = web_root
        .ok_or("this page has no app files")?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let relative = decoded_path(url)?;
    let path = root
        .join(relative.strip_prefix("/").unwrap_or(&relative))
        .canonicalize()
        .map_err(|_| format!("{url} does not exist"))?;
    path.starts_with(&root)
        .then_some(path)
        .ok_or_else(|| format!("{url} is outside the app"))
}

fn decoded_path(url: &Url) -> Result<PathBuf, String> {
    percent_decode(url.path())
        .map(PathBuf::from)
        .ok_or_else(|| format!("{url} has an invalid path"))
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = value.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_local_files_only_when_allowed() {
        let file = std::env::current_exe().unwrap();
        let source = format!("sabine://file{}", file.display()).replace(' ', "%20");
        let denied = SourcePolicy {
            web_root: None,
            local_files: false,
        };
        let allowed = SourcePolicy {
            web_root: None,
            local_files: true,
        };
        assert!(resolve(&source, &denied).is_err());
        assert_eq!(
            resolve(&source, &allowed).unwrap(),
            String::from(Url::from_file_path(&file).unwrap())
        );
    }

    #[test]
    fn keeps_app_files_inside_the_app() {
        let scratch =
            std::env::temp_dir().join(format!("sabine-media-source-{}", std::process::id()));
        let root = scratch.join("app");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("clip.mp4"), []).unwrap();
        std::fs::write(scratch.join("private.mp4"), []).unwrap();
        let policy = SourcePolicy {
            web_root: Some(&root),
            local_files: false,
        };
        assert!(resolve("sabine://app/clip.mp4", &policy).is_ok());
        assert!(resolve("sabine://app/%2E%2E/private.mp4", &policy).is_err());
        std::fs::remove_dir_all(scratch).unwrap();
    }
}
