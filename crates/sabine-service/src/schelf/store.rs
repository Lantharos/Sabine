use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::app::desktop::{
    caches::{refresh_desktop_database, refresh_icon_cache, refresh_mime_database},
    data_home, desktop_exec,
    entries::forget_desktop_entries,
};
use crate::app::forget::remove_path;
use crate::manifest::SabineFile;
use crate::{
    AppInstallMode, AppManifest, AppUpdateConfig, AppUpdateSource, RegisteredApp, SabineService,
    SabineVersion, ServiceError, ServiceResult, UpdatePolicy, write_install_inventory,
};

use super::notices::installed_by_store;
use super::releases::{launch_release, release_folder, remove_unused_releases};

impl SabineService {
    /// Installs `id` from `archive`, the app's Linux archive (its `portable`
    /// bundle) that a store has downloaded and checked against its own signed
    /// index. Sabine installs it for the current user, adds its launcher entry,
    /// icons and file types, and leaves its updates to the store: installing a
    /// newer archive the same way updates it, beside the version a running
    /// copy of the app may still use, and moves an app running in the
    /// background to it. Refuses a version Schelf has blocked.
    pub fn install_store_app(&self, archive: &Path, id: &str) -> ServiceResult<RegisteredApp> {
        if !crate::valid_app_id(id) {
            return Err(invalid("invalid app identifier"));
        }
        let staging = self.root.join("downloads").join(id).join("store");
        sabine_runtime::remove_directory_if_present(&staging)?;
        let installed = (|| {
            crate::archive::extract(archive, &staging)?;
            let bundle = StoreBundle::read(&staging.join(id).join("root"), id)?;
            self.refuse_blocked_release(id, &bundle.version)?;
            write_install_inventory(&bundle.payload, id, &[])?;
            let install = self.store_install_dir(id);
            let release = release_folder(&install, &bundle.version)?;
            self.install_app_payload(&bundle.payload, &release, bundle.manifest(id), || false)?;
            launch_release(&install, &release)?;
            install_desktop_files(id, &bundle.share, &install.join(&bundle.executable))?;
            crate::app::succession::hand_over(id, &release.join(&bundle.executable));
            remove_unused_releases(&release, &bundle.executable)?;
            self.app(id)
        })();
        sabine_runtime::remove_directory_if_present(&staging)?;
        installed
    }

    /// Removes an app a store installed, with its launcher entry, icons and
    /// file types. Its data stays.
    pub fn remove_store_app(&self, id: &str) -> ServiceResult<()> {
        let app = self.app(id)?;
        if !installed_by_store(&app) {
            return Err(invalid("the app wasn't installed by a store"));
        }
        let install = self.store_install_dir(id);
        forget_desktop_files(id, &install)?;
        remove_path(&install)?;
        self.forget_app(id, &install)
    }

    fn store_install_dir(&self, id: &str) -> PathBuf {
        self.root.join("apps").join(id).join("install")
    }
}

/// The files of a Linux bundle, as `sabine bundle` lays them out under `root`.
struct StoreBundle {
    payload: PathBuf,
    share: PathBuf,
    executable: PathBuf,
    name: String,
    version: String,
    sabine: SabineVersion,
}

impl StoreBundle {
    fn read(root: &Path, id: &str) -> ServiceResult<Self> {
        let payload = root.join("usr/lib/sabine").join(id);
        let manifest = fs::read_to_string(payload.join("resources/Sabine.toml"))
            .map_err(|error| invalid(&format!("the archive has no app manifest: {error}")))?;
        let (manifest, _) = SabineFile::parse(&manifest)
            .map_err(|error| invalid(&format!("the archive's manifest is invalid: {error}")))?;
        let app = manifest.app;
        if app.id.as_deref() != Some(id) {
            return Err(invalid(&format!("the archive isn't {id}")));
        }
        let missing = |key: &str| invalid(&format!("the archive's manifest has no app {key}"));
        Ok(Self {
            executable: bundle_executable(root)?,
            share: root.join("usr/share"),
            name: app.name.ok_or_else(|| missing("name"))?,
            version: app.version.ok_or_else(|| missing("version"))?,
            sabine: app
                .sabine
                .as_deref()
                .and_then(SabineVersion::parse)
                .ok_or_else(|| missing("Sabine build"))?,
            payload,
        })
    }

    fn manifest(&self, id: &str) -> AppManifest {
        AppManifest {
            id: id.to_string(),
            name: self.name.clone(),
            version: self.version.clone(),
            executable: self.payload.join(&self.executable),
            args: Vec::new(),
            update: Some(AppUpdateConfig {
                source: AppUpdateSource::Schelf,
                channel: "stable".to_string(),
                policy: UpdatePolicy::Automatic,
                install_mode: AppInstallMode::Store,
                public_key: String::new(),
                package_kind: None,
            }),
            sabine: self.sabine,
        }
    }
}

/// The program `usr/bin` links to, relative to the app's own folder.
fn bundle_executable(root: &Path) -> ServiceResult<PathBuf> {
    let mut links = fs::read_dir(root.join("usr/bin"))
        .map_err(|error| invalid(&format!("the archive has no programs: {error}")))?;
    let link = links
        .next()
        .ok_or_else(|| invalid("the archive has no programs"))??;
    if links.next().is_some() {
        return Err(invalid("the archive has more than one program"));
    }
    let name = fs::read_link(link.path())?
        .file_name()
        .map(PathBuf::from)
        .ok_or_else(|| invalid("the archive's program link is invalid"))?;
    Ok(name)
}

fn install_desktop_files(id: &str, share: &Path, executable: &Path) -> ServiceResult<()> {
    let data = data_home()?;
    let entry = fs::read_to_string(share.join("applications").join(format!("{id}.desktop")))?;
    let applications = data.join("applications");
    fs::create_dir_all(&applications)?;
    fs::write(
        applications.join(format!("{id}.desktop")),
        launching(&entry, executable),
    )?;
    let icons = data.join("icons/hicolor");
    copy_tree(&share.join("icons/hicolor"), &icons)?;
    refresh_icon_cache(&icons);
    let mime_package = share.join("mime/packages").join(format!("{id}.xml"));
    if mime_package.is_file() {
        copy_tree(&share.join("mime/packages"), &data.join("mime/packages"))?;
        refresh_mime_database(&data.join("mime"));
    }
    refresh_desktop_database(&applications);
    Ok(())
}

fn forget_desktop_files(id: &str, install: &Path) -> ServiceResult<()> {
    let data = data_home()?;
    forget_desktop_entries(id, install)?;
    let icons = data.join("icons/hicolor");
    if let Ok(sizes) = fs::read_dir(&icons) {
        for size in sizes.flatten().map(|size| size.path()) {
            if !size.is_dir() {
                continue;
            }
            for extension in ["png", "svg"] {
                remove_path(&size.join("apps").join(format!("{id}.{extension}")))?;
            }
        }
    }
    refresh_icon_cache(&icons);
    let mime_package = data.join("mime/packages").join(format!("{id}.xml"));
    if mime_package.is_file() {
        fs::remove_file(mime_package)?;
        refresh_mime_database(&data.join("mime"));
    }
    refresh_desktop_database(&data.join("applications"));
    Ok(())
}

/// The bundle's launcher entry, starting the installed program.
fn launching(entry: &str, executable: &Path) -> String {
    entry
        .lines()
        .map(|line| match line.strip_prefix("Exec=") {
            Some(_) => format!("Exec={} %U\n", desktop_exec(&executable.to_string_lossy())),
            None => format!("{line}\n"),
        })
        .collect()
}

fn copy_tree(source: &Path, destination: &Path) -> ServiceResult<()> {
    let Ok(entries) = fs::read_dir(source) else {
        return Ok(());
    };
    fs::create_dir_all(destination)?;
    for entry in entries {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn invalid(message: &str) -> ServiceError {
    ServiceError::InvalidManifest(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launcher_starts_the_installed_program() {
        let entry =
            "[Desktop Entry]\nType=Application\nName=Tern\nExec=tern %U\nIcon=com.example.tern\n";
        assert_eq!(
            launching(
                entry,
                Path::new("/home/a/.local/share/sabine/apps/com.example.tern/install/tern")
            ),
            "[Desktop Entry]\nType=Application\nName=Tern\nExec=\"/home/a/.local/share/sabine/apps/com.example.tern/install/tern\" %U\nIcon=com.example.tern\n"
        );
    }
}
