//! `recently-used.xbel`, as the freedesktop desktop bookmark spec describes
//! it. A bookmark keeps what GLib's bookmark file keeps, so writing the list
//! back loses nothing GTK apps would keep either.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

const BOOKMARK_NS: &str = "http://www.freedesktop.org/standards/desktop-bookmarks";
const MIME_NS: &str = "http://www.freedesktop.org/standards/shared-mime-info";
const FREEDESKTOP_OWNER: &str = "http://freedesktop.org";

/// How the list names the app that opened a file.
pub(super) struct Application {
    name: String,
    exec: String,
}

impl Application {
    pub(super) fn new(name: &str, program: &Path) -> Self {
        let program = program.to_string_lossy().replace('\'', r"'\''");
        Self {
            name: name.to_string(),
            exec: format!("'{program} %u'"),
        }
    }
}

#[derive(Default)]
struct Bookmarks {
    title: Option<String>,
    description: Option<String>,
    bookmarks: Vec<Bookmark>,
}

struct Bookmark {
    href: String,
    added: String,
    modified: String,
    visited: String,
    title: Option<String>,
    description: Option<String>,
    mime_type: Option<String>,
    groups: Vec<String>,
    applications: Vec<Opener>,
    private: bool,
    icon: Option<(String, Option<String>)>,
}

struct Opener {
    name: String,
    exec: String,
    modified: String,
    count: u64,
}

pub(super) fn add(file: &Path, application: &Application) -> io::Result<()> {
    let href = file_uri(file);
    let now = timestamp(SystemTime::now());
    update(|list| {
        let index = match list.bookmarks.iter().position(|item| item.href == href) {
            Some(index) => index,
            None => {
                list.bookmarks
                    .push(Bookmark::new(href.clone(), now.clone()));
                list.bookmarks.len() - 1
            }
        };
        let bookmark = &mut list.bookmarks[index];
        bookmark.modified.clone_from(&now);
        bookmark.visited.clone_from(&now);
        if bookmark.mime_type.is_none() {
            bookmark.mime_type = Some(mime_type(file));
        }
        match bookmark
            .applications
            .iter_mut()
            .find(|opener| opener.name == application.name)
        {
            Some(opener) => {
                opener.exec.clone_from(&application.exec);
                opener.modified.clone_from(&now);
                opener.count += 1;
            }
            None => bookmark.applications.push(Opener {
                name: application.name.clone(),
                exec: application.exec.clone(),
                modified: now.clone(),
                count: 1,
            }),
        }
    })
}

pub(super) fn clear(application: &Application) -> io::Result<()> {
    update(|list| list.forget(&application.name))
}

/// Reads the list, changes it and puts the result in its place at once, so
/// apps reading it meanwhile see either list whole.
fn update(change: impl FnOnce(&mut Bookmarks)) -> io::Result<()> {
    let path = list_path()?;
    let text = match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let mut list = match &text {
        Some(text) => Bookmarks::parse(text).map_err(io::Error::other)?,
        None => Bookmarks::default(),
    };
    change(&mut list);
    let updated = list.render();
    if text.as_deref() == Some(updated.as_str()) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("xbel.{}.tmp", std::process::id()));
    fs::write(&temporary, updated)?;
    fs::rename(&temporary, &path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

fn list_path() -> io::Result<PathBuf> {
    let data = env::var_os("XDG_DATA_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(data.join("recently-used.xbel"))
}

impl Bookmark {
    fn new(href: String, now: String) -> Self {
        Self {
            href,
            added: now.clone(),
            modified: now.clone(),
            visited: now,
            title: None,
            description: None,
            mime_type: None,
            groups: Vec::new(),
            applications: Vec::new(),
            private: false,
            icon: None,
        }
    }
}

type Node<'a, 'input> = roxmltree::Node<'a, 'input>;

impl Bookmarks {
    fn parse(text: &str) -> Result<Self, String> {
        let document = roxmltree::Document::parse(text)
            .map_err(|error| format!("recently-used.xbel is not valid XML: {error}"))?;
        let root = document.root_element();
        if root.tag_name().name() != "xbel" {
            return Err("recently-used.xbel is not an XBEL file".to_string());
        }
        let mut list = Self::default();
        for node in elements(root) {
            match node.tag_name().name() {
                "title" => list.title = node.text().map(str::to_string),
                "desc" => list.description = node.text().map(str::to_string),
                "bookmark" => list.bookmarks.extend(parse_bookmark(node)),
                _ => {}
            }
        }
        Ok(list)
    }

    /// Takes the app off every file it opened, and the files only it opened
    /// off the list.
    fn forget(&mut self, name: &str) {
        self.bookmarks.retain_mut(|bookmark| {
            let opened = bookmark.applications.len();
            bookmark.applications.retain(|opener| opener.name != name);
            bookmark.applications.len() == opened || !bookmark.applications.is_empty()
        });
    }

    fn render(&self) -> String {
        let mut output = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xbel version=\"1.0\"\n      xmlns:bookmark=\"{BOOKMARK_NS}\"\n      xmlns:mime=\"{MIME_NS}\"\n>\n"
        );
        if let Some(title) = &self.title {
            output.push_str(&format!("  <title>{}</title>\n", escape(title)));
        }
        if let Some(description) = &self.description {
            output.push_str(&format!("  <desc>{}</desc>\n", escape(description)));
        }
        for bookmark in &self.bookmarks {
            bookmark.render(&mut output);
        }
        output.push_str("</xbel>");
        output
    }
}

fn parse_bookmark(node: Node) -> Option<Bookmark> {
    let mut bookmark = Bookmark::new(node.attribute("href")?.to_string(), String::new());
    let time = |name: &str| node.attribute(name).unwrap_or_default().to_string();
    bookmark.added = time("added");
    bookmark.modified = time("modified");
    bookmark.visited = time("visited");
    for child in elements(node) {
        match child.tag_name().name() {
            "title" => bookmark.title = child.text().map(str::to_string),
            "desc" => bookmark.description = child.text().map(str::to_string),
            "info" => elements(child)
                .filter(|metadata| {
                    metadata.tag_name().name() == "metadata"
                        && metadata.attribute("owner") == Some(FREEDESKTOP_OWNER)
                })
                .for_each(|metadata| parse_metadata(metadata, &mut bookmark)),
            _ => {}
        }
    }
    Some(bookmark)
}

fn parse_metadata(metadata: Node, bookmark: &mut Bookmark) {
    for node in elements(metadata) {
        let namespace = node.tag_name().namespace();
        match (namespace, node.tag_name().name()) {
            (Some(MIME_NS), "mime-type") => {
                bookmark.mime_type = node.attribute("type").map(str::to_string);
            }
            (Some(BOOKMARK_NS), "groups") => bookmark
                .groups
                .extend(elements(node).filter_map(|group| group.text().map(str::to_string))),
            (Some(BOOKMARK_NS), "applications") => {
                bookmark
                    .applications
                    .extend(elements(node).filter_map(parse_opener));
            }
            (Some(BOOKMARK_NS), "private") => bookmark.private = true,
            (Some(BOOKMARK_NS), "icon") => {
                bookmark.icon = node
                    .attribute("href")
                    .map(|href| (href.to_string(), node.attribute("type").map(str::to_string)));
            }
            _ => {}
        }
    }
}

fn parse_opener(node: Node) -> Option<Opener> {
    let modified = match (node.attribute("modified"), node.attribute("timestamp")) {
        (Some(modified), _) => modified.to_string(),
        (None, Some(seconds)) => {
            timestamp(UNIX_EPOCH + std::time::Duration::from_secs(seconds.parse().ok()?))
        }
        (None, None) => String::new(),
    };
    Some(Opener {
        name: node.attribute("name")?.to_string(),
        exec: node.attribute("exec")?.to_string(),
        modified,
        count: node
            .attribute("count")
            .and_then(|count| count.parse().ok())
            .unwrap_or(1),
    })
}

fn elements<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

impl Bookmark {
    fn render(&self, output: &mut String) {
        output.push_str(&format!(
            "  <bookmark href=\"{}\" added=\"{}\" modified=\"{}\" visited=\"{}\">\n",
            escape(&self.href),
            escape(&self.added),
            escape(&self.modified),
            escape(&self.visited)
        ));
        if let Some(title) = &self.title {
            output.push_str(&format!("    <title>{}</title>\n", escape(title)));
        }
        if let Some(description) = &self.description {
            output.push_str(&format!("    <desc>{}</desc>\n", escape(description)));
        }
        output.push_str(&format!(
            "    <info>\n      <metadata owner=\"{FREEDESKTOP_OWNER}\">\n"
        ));
        if let Some(mime_type) = &self.mime_type {
            output.push_str(&format!(
                "        <mime:mime-type type=\"{}\"/>\n",
                escape(mime_type)
            ));
        }
        if !self.groups.is_empty() {
            output.push_str("        <bookmark:groups>\n");
            for group in &self.groups {
                output.push_str(&format!(
                    "          <bookmark:group>{}</bookmark:group>\n",
                    escape(group)
                ));
            }
            output.push_str("        </bookmark:groups>\n");
        }
        if !self.applications.is_empty() {
            output.push_str("        <bookmark:applications>\n");
            for opener in &self.applications {
                output.push_str(&format!(
                    "          <bookmark:application name=\"{}\" exec=\"{}\" modified=\"{}\" count=\"{}\"/>\n",
                    escape(&opener.name),
                    escape(&opener.exec),
                    escape(&opener.modified),
                    opener.count
                ));
            }
            output.push_str("        </bookmark:applications>\n");
        }
        if self.private {
            output.push_str("        <bookmark:private/>\n");
        }
        if let Some((href, mime_type)) = &self.icon {
            output.push_str(&format!("        <bookmark:icon href=\"{}\"", escape(href)));
            if let Some(mime_type) = mime_type {
                output.push_str(&format!(" type=\"{}\"", escape(mime_type)));
            }
            output.push_str("/>\n");
        }
        output.push_str("      </metadata>\n    </info>\n  </bookmark>\n");
    }
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// The file's URI as GLib writes it, so a file GTK apps list already is
/// found again instead of listed twice.
fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;

    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()/:@&=+$,".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

fn mime_type(path: &Path) -> String {
    static DATABASE: OnceLock<xdg_mime::SharedMimeInfo> = OnceLock::new();
    DATABASE
        .get_or_init(xdg_mime::SharedMimeInfo::new)
        .guess_mime_type()
        .path(path)
        .guess()
        .mime_type()
        .essence_str()
        .to_string()
}

/// `time` in UTC as ISO 8601 with microseconds, as GLib writes it.
fn timestamp(time: SystemTime) -> String {
    let since_epoch = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since_epoch.as_secs();
    let days = (seconds / 86_400) as i64;
    let (year, month, day) = civil_date(days);
    let of_day = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:06}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60,
        since_epoch.subsec_micros()
    )
}

/// The proleptic Gregorian date `days` after 1970-01-01.
fn civil_date(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GTK_LIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0"
      xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks"
      xmlns:mime="http://www.freedesktop.org/standards/shared-mime-info"
>
  <bookmark href="file:///home/a/notes%20one.txt" added="2026-01-02T03:04:05.000006Z" modified="2026-01-02T03:04:05.000006Z" visited="2026-01-02T03:04:05.000006Z">
    <info>
      <metadata owner="http://freedesktop.org">
        <mime:mime-type type="text/plain"/>
        <bookmark:groups>
          <bookmark:group>Editors</bookmark:group>
        </bookmark:groups>
        <bookmark:applications>
          <bookmark:application name="Text Editor" exec="&apos;gnome-text-editor %u&apos;" timestamp="0" count="2"/>
        </bookmark:applications>
      </metadata>
    </info>
  </bookmark>
</xbel>"#;

    #[test]
    fn keeps_what_gtk_apps_wrote() {
        let list = Bookmarks::parse(GTK_LIST).unwrap();
        let rendered = list.render();
        let bookmark = &list.bookmarks[0];
        assert_eq!(bookmark.groups, ["Editors"]);
        assert_eq!(bookmark.applications[0].count, 2);
        assert_eq!(
            bookmark.applications[0].modified,
            "1970-01-01T00:00:00.000000Z"
        );
        assert!(rendered.contains("exec=\"&apos;gnome-text-editor %u&apos;\""));
        assert!(rendered.contains("<mime:mime-type type=\"text/plain\"/>"));
        let reparsed = Bookmarks::parse(&rendered).unwrap();
        assert_eq!(reparsed.render(), rendered);
    }

    #[test]
    fn clearing_leaves_files_other_apps_opened() {
        let mut list = Bookmarks::parse(GTK_LIST).unwrap();
        let mine = Bookmark {
            applications: vec![Opener {
                name: "Notes".into(),
                exec: "'notes %u'".into(),
                modified: String::new(),
                count: 1,
            }],
            ..Bookmark::new("file:///home/a/mine.txt".into(), String::new())
        };
        list.bookmarks.push(mine);
        list.forget("Notes");
        assert_eq!(list.bookmarks.len(), 1);
        assert_eq!(list.bookmarks[0].href, "file:///home/a/notes%20one.txt");
    }

    #[test]
    fn writes_uris_and_times_as_glib_does() {
        assert_eq!(
            file_uri(Path::new("/home/a/b c/ü.txt")),
            "file:///home/a/b%20c/%C3%BC.txt"
        );
        assert_eq!(
            timestamp(UNIX_EPOCH + std::time::Duration::from_micros(1_767_323_045_000_006)),
            "2026-01-02T03:04:05.000006Z"
        );
    }
}
