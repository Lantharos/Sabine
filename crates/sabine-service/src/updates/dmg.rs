use std::{path::Path, process::Command};

use crate::types::{PendingAppUpdate, ServiceError, ServiceResult};

/// Copies the disk image into a new folder only the installing user can
/// write, checks it there and installs the app it holds from that copy, so
/// the app an administrator installs is the one that matched its release,
/// whatever happens to the download meanwhile.
const DMG_INSTALL: &str = r#"artifact=$1 sha256=$2 app=$3
directory=$(/usr/bin/mktemp -d /private/tmp/sabine-update.XXXXXX) || exit 1
mount="$directory/mount"
trap '/usr/bin/hdiutil detach -quiet "$mount" 2>/dev/null; /bin/rm -rf "$directory"' EXIT
image="$directory/update.dmg"
/bin/cp -- "$artifact" "$image" || exit 1
if [ "$(/usr/bin/shasum -a 256 "$image" | /usr/bin/cut -d ' ' -f 1)" != "$sha256" ]; then
  echo "the update does not match its release" >&2
  exit 1
fi
/bin/mkdir "$mount" || exit 1
/usr/bin/hdiutil attach -quiet -nobrowse -readonly -noautoopen -mountpoint "$mount" "$image" || exit 1
set -- "$mount"/*.app
if [ ! -d "$1" ]; then
  echo "the update holds no application" >&2
  exit 1
fi
/bin/rm -rf "$app.sabine-new" "$app.sabine-old" || exit 1
/usr/bin/ditto "$1" "$app.sabine-new" || exit 1
/bin/mv "$app" "$app.sabine-old" || exit 1
if ! /bin/mv "$app.sabine-new" "$app"; then
  /bin/mv "$app.sabine-old" "$app"
  exit 1
fi
/bin/rm -rf "$app.sabine-old""#;

pub(super) fn install_dmg(update: &PendingAppUpdate, target: Option<&Path>) -> ServiceResult<()> {
    let executable = target.ok_or_else(|| {
        ServiceError::Update("DMG update is missing its application path".to_string())
    })?;
    let app = executable
        .ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .ok_or_else(|| ServiceError::Update("could not locate installed macOS app".to_string()))?;
    let arguments = [
        update.artifact.display().to_string(),
        update.sha256.to_ascii_lowercase(),
        app.display().to_string(),
    ];
    let status = if replaceable_by_user(app) {
        Command::new("/bin/sh")
            .args(["-c", DMG_INSTALL, "sh"])
            .args(&arguments)
            .status()?
    } else {
        let arguments = arguments
            .iter()
            .map(|argument| shell_single_quote(argument))
            .collect::<Vec<_>>()
            .join(" ");
        let command = format!(
            "/bin/sh -c {} sh {arguments}",
            shell_single_quote(DMG_INSTALL)
        );
        Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "do shell script {} with administrator privileges",
                apple_script_string(&command)
            ))
            .status()?
    };
    status
        .success()
        .then_some(())
        .ok_or_else(|| ServiceError::Update("macOS app installation was cancelled".to_string()))
}

/// Whether the current user can swap the bundle in place: they own it and may
/// write to the folder holding it, as with apps dragged into `/Applications`.
fn replaceable_by_user(app: &Path) -> bool {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    let user = unsafe { libc::getuid() };
    let owned = std::fs::symlink_metadata(app).is_ok_and(|metadata| metadata.uid() == user);
    let writable_parent = app
        .parent()
        .and_then(|parent| std::ffi::CString::new(parent.as_os_str().as_bytes()).ok())
        .is_some_and(|parent| unsafe { libc::access(parent.as_ptr(), libc::W_OK) } == 0);
    owned && writable_parent
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn apple_script_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}
