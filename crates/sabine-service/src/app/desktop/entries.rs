use std::{fs, io, path::Path, time::Duration};

use super::{config_home, data_home};

/// Marks the hidden entry Sabine writes so the GlobalShortcuts portal can
/// name an app that has no installed entry yet.
pub const SHORTCUT_HOST_KEY: &str = "X-Sabine-Shortcut-Host=true";

/// Removes the desktop entries Sabine wrote for an app whose programs live in
/// `program_root`: its launcher, its URL handler and its shortcut stand-in,
/// along with the default-application choices that name them.
pub fn forget_desktop_entries(id: &str, program_root: &Path) -> io::Result<()> {
    let applications = data_home()?.join("applications");
    let mut removed = Vec::new();
    for desktop_id in [format!("{id}.desktop"), format!("{id}.sabine-url.desktop")] {
        let path = applications.join(&desktop_id);
        let Ok(entry) = fs::read_to_string(&path) else {
            continue;
        };
        if entry.contains(SHORTCUT_HOST_KEY) || launches_from(&entry, program_root) {
            fs::remove_file(&path)?;
            removed.push(desktop_id);
        }
    }
    forget_associations(&removed)
}

fn launches_from(entry: &str, program_root: &Path) -> bool {
    let Some(program) = exec_program(entry) else {
        return false;
    };
    let program = Path::new(&program);
    program.starts_with(program_root)
        || program_root
            .canonicalize()
            .is_ok_and(|root| program.starts_with(root))
}

fn forget_associations(desktop_ids: &[String]) -> io::Result<()> {
    if desktop_ids.is_empty() {
        return Ok(());
    }
    let config = config_home()?;
    let _lock = sabine_runtime::FileLock::acquire(
        &config.join("sabine/mimeapps.lock"),
        Duration::from_secs(5),
        |_| {},
    )?;
    let path = config.join("mimeapps.list");
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let updated = without_desktop_ids(&content, desktop_ids);
    if updated == content {
        return Ok(());
    }
    let temporary = path.with_extension("list.sabine-tmp");
    fs::write(&temporary, updated)?;
    fs::rename(temporary, path)
}

fn exec_program(entry: &str) -> Option<String> {
    let value = entry.lines().find_map(|line| line.strip_prefix("Exec="))?;
    let mut characters = value.replace("\\\\", "\\").into_bytes().into_iter();
    if characters.next()? != b'"' {
        return None;
    }
    let mut program = Vec::new();
    while let Some(byte) = characters.next() {
        match byte {
            b'\\' => program.push(characters.next()?),
            b'"' => {
                return String::from_utf8(program)
                    .ok()
                    .map(|program| program.replace("%%", "%"));
            }
            _ => program.push(byte),
        }
    }
    None
}

fn without_desktop_ids(content: &str, desktop_ids: &[String]) -> String {
    let mut output = String::with_capacity(content.len());
    for line in content.lines() {
        let owned = |entry: &&str| desktop_ids.iter().any(|id| id == entry);
        match line.split_once('=') {
            Some((key, value)) if value.split(';').any(|entry| owned(&entry)) => {
                let remaining = value
                    .split(';')
                    .filter(|entry| !entry.is_empty() && !owned(entry))
                    .collect::<Vec<_>>();
                if !remaining.is_empty() {
                    output.push_str(&format!("{key}={};\n", remaining.join(";")));
                }
            }
            _ => {
                output.push_str(line);
                output.push('\n');
            }
        }
    }
    if !content.ends_with('\n') {
        output.pop();
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_program_the_runtime_writes() {
        let entry = "[Desktop Entry]\nType=Application\nExec=\"/home/a b/sabine/apps/com.example.signin/install/sign\\\\\"in\" %U\nNoDisplay=true\n";
        assert_eq!(
            exec_program(entry).as_deref(),
            Some("/home/a b/sabine/apps/com.example.signin/install/sign\"in")
        );
    }

    #[test]
    fn forgets_only_the_apps_associations() {
        let content = "[Default Applications]\nx-scheme-handler/example=com.example.signin.sabine-url.desktop\ntext/plain=org.gnome.TextEditor.desktop\n\n[Added Associations]\ntext/markdown=com.example.signin.desktop;org.gnome.TextEditor.desktop;\nx-scheme-handler/other=com.example.signin.desktop.extra.desktop;\n";
        let ids = [
            "com.example.signin.desktop".to_string(),
            "com.example.signin.sabine-url.desktop".to_string(),
        ];
        assert_eq!(
            without_desktop_ids(content, &ids),
            "[Default Applications]\ntext/plain=org.gnome.TextEditor.desktop\n\n[Added Associations]\ntext/markdown=org.gnome.TextEditor.desktop;\nx-scheme-handler/other=com.example.signin.desktop.extra.desktop;\n"
        );
    }
}
