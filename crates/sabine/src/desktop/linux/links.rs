use std::{collections::BTreeSet, fs, io, path::PathBuf};

use sabine_platform::{AutostartEntry, DeepLinkRegistration};
use sabine_service::SHORTCUT_HOST_KEY;

use super::util::*;
use crate::desktop::sanitize_id;

/// Rewrites the login entry only when its contents changed, and leaves one
/// the person turned off in their desktop's settings off.
pub(super) fn write_autostart_entry(entry: &AutostartEntry) -> io::Result<()> {
    let path = sabine_service::app_autostart_path(&entry.id)?;
    if !entry.enabled {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
    }
    let current = fs::read_to_string(&path).ok();
    if current.as_deref().is_some_and(turned_off) {
        return Ok(());
    }
    let contents = desktop_entry(&entry.id, &entry.name, &entry.command);
    if current.as_deref() == Some(contents.as_str()) {
        return Ok(());
    }
    write_file(path, &contents)
}

/// Desktops turn a login entry off by marking it instead of removing it.
fn turned_off(entry: &str) -> bool {
    entry.lines().any(|line| {
        matches!(
            line.trim(),
            "Hidden=true" | "X-GNOME-Autostart-enabled=false"
        )
    })
}

/// The portal finds an app through the desktop entry named after its id. An
/// installed entry is never touched; without one, a hidden entry starting
/// this executable stands in until the app is installed.
pub(super) fn ensure_shortcut_host_entry(app_id: &str, name: &str) -> io::Result<()> {
    let file_name = format!("{}.desktop", sanitize_id(app_id));
    let own = data_home()?.join("applications").join(&file_name);
    let installed = application_dirs()?
        .into_iter()
        .map(|dir| dir.join(&file_name))
        .any(|path| path.is_file() && !is_shortcut_host_entry(&path));
    if !installed {
        let command = desktop_exec(&utf8_executable()?);
        return write_file(
            own,
            &format!(
                "{}{SHORTCUT_HOST_KEY}\n",
                desktop_entry(app_id, name, &command)
            ),
        );
    }
    if is_shortcut_host_entry(&own) {
        fs::remove_file(own)?;
    }
    Ok(())
}

fn is_shortcut_host_entry(path: &std::path::Path) -> bool {
    fs::read_to_string(path).is_ok_and(|entry| entry.lines().any(|line| line == SHORTCUT_HOST_KEY))
}

pub(super) fn register_deep_links(registration: &DeepLinkRegistration) -> io::Result<()> {
    registration.validate().map_err(io::Error::other)?;
    if registration.schemes.is_empty() {
        return Ok(());
    }
    let schemes = registration
        .schemes
        .iter()
        .map(|scheme| scheme.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let installed_id = format!("{}.desktop", registration.id);
    let stand_in_id = format!("{}.sabine-url.desktop", registration.id);
    let stand_in = data_home()?.join("applications").join(&stand_in_id);
    let handler = if handles_schemes(&installed_id, &schemes)? {
        if let Err(error) = fs::remove_file(&stand_in)
            && error.kind() != io::ErrorKind::NotFound
        {
            return Err(error);
        }
        installed_id.clone()
    } else {
        write_stand_in(&stand_in, registration, &schemes)?;
        stand_in_id.clone()
    };
    let config = config_home()?;
    let _lock = sabine_runtime::FileLock::acquire(
        &config.join("sabine/mimeapps.lock"),
        std::time::Duration::from_secs(5),
        |_| {},
    )?;
    let path = config.join("mimeapps.list");
    let mut content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let own = [installed_id.as_str(), stand_in_id.as_str()];
    for scheme in schemes {
        let replaceable = match mime_default(&content, &scheme) {
            Some(chosen) => own.contains(&chosen) || find_entry(chosen)?.is_none(),
            None => true,
        };
        if replaceable {
            content = set_mime_default(&content, &scheme, &handler);
        }
    }
    write_file(path.with_extension("list.sabine-tmp"), &content)?;
    fs::rename(path.with_extension("list.sabine-tmp"), path)
}

fn write_stand_in(
    path: &std::path::Path,
    registration: &DeepLinkRegistration,
    schemes: &BTreeSet<String>,
) -> io::Result<()> {
    let executable = utf8_executable()?;
    let mime_types = schemes
        .iter()
        .map(|scheme| format!("x-scheme-handler/{scheme};"))
        .collect::<String>();
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName={}\nIcon={}\nTryExec={}\nExec={} %U\nTerminal=false\nNoDisplay=true\nMimeType={mime_types}\n",
        registration.id,
        registration.id,
        desktop_value(&executable),
        desktop_exec(&executable)
    );
    write_file(path.with_extension("desktop.tmp"), &desktop)?;
    fs::rename(path.with_extension("desktop.tmp"), path)
}

fn utf8_executable() -> io::Result<String> {
    crate::launch::executable::launch_executable()?
        .into_os_string()
        .into_string()
        .map_err(|_| io::Error::other("the app's executable must have a UTF-8 path"))
}

fn find_entry(desktop_id: &str) -> io::Result<Option<PathBuf>> {
    Ok(application_dirs()?
        .into_iter()
        .map(|dir| dir.join(desktop_id))
        .find(|path| path.is_file()))
}

fn handles_schemes(desktop_id: &str, schemes: &BTreeSet<String>) -> io::Result<bool> {
    let Some(path) = find_entry(desktop_id)? else {
        return Ok(false);
    };
    let entry = fs::read_to_string(path)?;
    let declared = entry
        .lines()
        .find_map(|line| line.strip_prefix("MimeType="))
        .unwrap_or_default()
        .split(';')
        .collect::<BTreeSet<_>>();
    Ok(schemes
        .iter()
        .all(|scheme| declared.contains(format!("x-scheme-handler/{scheme}").as_str())))
}

fn mime_default<'a>(content: &'a str, scheme: &str) -> Option<&'a str> {
    let key = format!("x-scheme-handler/{scheme}");
    content
        .lines()
        .skip_while(|line| line.trim() != "[Default Applications]")
        .skip(1)
        .take_while(|line| !line.trim().starts_with('['))
        .find_map(|line| {
            line.split_once('=')
                .filter(|(line_key, _)| *line_key == key)
        })
        .and_then(|(_, value)| value.split(';').next())
        .filter(|value| !value.is_empty())
}

fn desktop_exec(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    format!("\"{}\"", escaped.replace('\\', "\\\\"))
}

fn write_file(path: PathBuf, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

fn desktop_entry(id: &str, name: &str, command: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nGenericName={}\nComment={}\nExec={}\nIcon={}\nTerminal=false\nNoDisplay=true\nStartupNotify=false\nCategories=Utility;\n",
        desktop_value(name),
        desktop_value(name),
        desktop_value(name),
        desktop_value(command),
        desktop_value(id)
    )
}

fn set_mime_default(content: &str, scheme: &str, desktop_id: &str) -> String {
    let key = format!("x-scheme-handler/{scheme}");
    let value = format!("{key}={desktop_id}");
    let mut lines = content.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
    let Some(section_start) = lines
        .iter()
        .position(|line| line.trim() == "[Default Applications]")
    else {
        if !lines.is_empty() && lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
        lines.push("[Default Applications]".to_string());
        lines.push(value);
        return finish_lines(lines);
    };
    let section_end = lines
        .iter()
        .enumerate()
        .skip(section_start + 1)
        .find_map(|(index, line)| line.trim().starts_with('[').then_some(index))
        .unwrap_or(lines.len());
    if let Some(index) = lines[section_start + 1..section_end]
        .iter()
        .position(|line| {
            line.split_once('=')
                .is_some_and(|(line_key, _)| line_key == key)
        })
    {
        lines[section_start + 1 + index] = value;
    } else {
        lines.insert(section_end, value);
    }
    finish_lines(lines)
}

fn finish_lines(lines: Vec<String>) -> String {
    let mut output = lines.join("\n");
    output.push('\n');
    output
}
