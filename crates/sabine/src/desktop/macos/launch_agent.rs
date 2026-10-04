use std::fs;

use sabine_platform::AutostartEntry;

pub(in crate::desktop) fn write_autostart_entry(entry: &AutostartEntry) -> Result<(), String> {
    let plist_path =
        sabine_service::app_autostart_path(&entry.id).map_err(|error| error.to_string())?;
    let label = sabine_service::app_autostart_label(&entry.id);
    if !entry.enabled {
        return match fs::remove_file(&plist_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        };
    }
    let program_args = shell_words::split(&entry.command)
        .map_err(|error| format!("Invalid autostart command: {error}"))?;
    if program_args.first().is_none_or(String::is_empty) {
        return Err("Autostart command must name an executable".into());
    }
    let args_xml = program_args
        .iter()
        .map(|arg| format!("    <string>{}</string>", xml_escape(arg)))
        .collect::<Vec<_>>()
        .join("\n");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
{args_xml}
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
    );
    if let Some(agents) = plist_path.parent() {
        fs::create_dir_all(agents).map_err(|error| error.to_string())?;
    }
    fs::write(plist_path, plist).map_err(|error| error.to_string())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
