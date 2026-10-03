use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use crate::launch::executable::running_appimage;

pub(crate) fn native_host_program(id: &str, executable: PathBuf) -> io::Result<PathBuf> {
    let Some(image) = running_appimage()? else {
        return Ok(executable);
    };
    let Ok(program) = executable
        .canonicalize()?
        .strip_prefix(&image.mount)
        .map(PathBuf::from)
    else {
        return Ok(executable);
    };
    let directory = sabine_service::service_data_dir().join("native-messaging");
    fs::create_dir_all(&directory)?;
    let launcher = directory.join(id);
    fs::write(&launcher, launcher_script(&image.file, &program)?)?;
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))?;
    Ok(launcher)
}

fn launcher_script(image: &Path, program: &Path) -> io::Result<String> {
    Ok(format!(
        "#!/bin/sh\nexport {}={}\nexec {} \"$@\"\n",
        sabine_service::APPIMAGE_PROGRAM_ENV,
        shell_quoted(program)?,
        shell_quoted(image)?
    ))
}

fn shell_quoted(path: &Path) -> io::Result<String> {
    let path = path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "native messaging executable path must be valid Unicode",
        )
    })?;
    Ok(format!("'{}'", path.replace('\'', r"'\''")))
}
