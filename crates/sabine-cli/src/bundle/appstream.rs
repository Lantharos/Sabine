use super::config::BundleApp;

/// AppStream metadata that lets software centers describe the app, written
/// for apps that appear in launchers and name a homepage.
pub(super) fn metainfo(app: &BundleApp, executable: &str) -> Option<String> {
    if !app.listing.listed {
        return None;
    }
    let homepage = xml(app.homepage.as_deref()?);
    let summary = xml(app.listing.generic_name.as_deref().unwrap_or(&app.name));
    let license = app
        .license
        .as_deref()
        .map(|license| format!("  <project_license>{}</project_license>\n", xml(license)))
        .unwrap_or_default();
    let media_types = app
        .associations
        .documents()
        .map(|document| format!("    <mediatype>{}</mediatype>\n", document.mime_type))
        .collect::<String>();
    let keywords = if app.listing.keywords.is_empty() {
        String::new()
    } else {
        format!(
            "  <keywords>\n{}  </keywords>\n",
            app.listing
                .keywords
                .iter()
                .map(|keyword| format!("    <keyword>{}</keyword>\n", xml(keyword)))
                .collect::<String>()
        )
    };
    Some(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>{id}</id>
  <metadata_license>CC0-1.0</metadata_license>
{license}  <name>{name}</name>
  <summary>{summary}</summary>
  <description>
    <p>{summary}</p>
  </description>
  <developer>
    <name>{publisher}</name>
  </developer>
  <url type="homepage">{homepage}</url>
  <launchable type="desktop-id">{id}.desktop</launchable>
  <provides>
    <binary>{executable}</binary>
{media_types}  </provides>
{keywords}  <content_rating type="oars-1.1"/>
  <releases>
    <release version="{version}" timestamp="{timestamp}"/>
  </releases>
</component>
"#,
        id = app.id,
        name = xml(&app.name),
        publisher = xml(&app.publisher),
        executable = xml(executable),
        version = xml(&app.version),
        timestamp = release_timestamp(),
    ))
}

fn release_timestamp() -> u64 {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after 1970")
                .as_secs()
        })
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
