use std::path::PathBuf;

use rfd::{AsyncFileDialog, FileHandle};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::OsrHostEvent;
use crate::osr::protocol::{FileDialogMode, FileDialogRequest};

impl OsrNativeHost {
    /// Shows the page's file chooser as a desktop dialog attached to this
    /// window, and hands the chosen paths back to the page when it closes.
    pub(in crate::osr::host) fn show_file_dialog(&self, request: FileDialogRequest) {
        let mut dialog = AsyncFileDialog::new();
        if let Some(window) = &self.window {
            dialog = dialog.set_parent(window.as_ref());
        }
        if !request.title.is_empty() {
            dialog = dialog.set_title(request.title);
        }
        if let Some(default_path) = request.default_path {
            if default_path.is_dir() {
                dialog = dialog.set_directory(default_path);
            } else {
                if let Some(directory) = default_path.parent().filter(|path| path.is_dir()) {
                    dialog = dialog.set_directory(directory);
                }
                if let Some(name) = default_path.file_name() {
                    dialog = dialog.set_file_name(name.to_string_lossy());
                }
            }
        }
        for filter in request.filters {
            dialog = dialog.add_filter(filter.description, &filter.extensions);
        }
        let generation = self.connection_generation;
        let sender = self.sender.clone();
        let proxy = self.proxy.clone();
        let id = request.id;
        let mode = request.mode;
        std::thread::spawn(move || {
            let paths = pollster::block_on(choose(dialog, mode));
            if sender
                .send(OsrHostEvent::FileDialogClosed(generation, id, paths))
                .is_ok()
            {
                proxy.wake_up();
            }
        });
    }

    pub(in crate::osr::host) fn finish_file_dialog(&self, id: u32, paths: Option<Vec<PathBuf>>) {
        let paths = paths.map_or_else(String::new, |paths| {
            let paths = serde_json::json!({ "paths": paths });
            format!("\t{paths}")
        });
        self.send_control(format!("file_dialog\t{id}{paths}\n"));
    }
}

async fn choose(dialog: AsyncFileDialog, mode: FileDialogMode) -> Option<Vec<PathBuf>> {
    let single = |file: Option<FileHandle>| file.map(|file| vec![file.path().to_path_buf()]);
    match mode {
        FileDialogMode::Open => single(dialog.pick_file().await),
        FileDialogMode::OpenMultiple => dialog
            .pick_files()
            .await
            .map(|files| files.iter().map(|file| file.path().to_path_buf()).collect()),
        FileDialogMode::OpenFolder => single(dialog.pick_folder().await),
        FileDialogMode::Save => single(dialog.save_file().await),
    }
}
