#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::time::{SystemTime, UNIX_EPOCH};

use sabine::prelude::*;
use serde::{Deserialize, Serialize};
use specta::Type;

const MAX_TITLE: usize = 200;

#[derive(Deserialize, Type)]
struct NewNote {
    title: String,
}

#[derive(Serialize, Type)]
struct CreatedNote {
    id: String,
    title: String,
}

fn main() {
    SabineWindow::main(|window| {
        Ok(window
            .app()
            .glass()
            .app_chrome(AppChrome::default())
            .min_size(720, 480)
            .bridge_typed("notes.create", create_note))
    });
}

fn create_note(note: NewNote) -> Result<CreatedNote, BridgeError> {
    let title = match note.title.trim() {
        "" => "Untitled",
        title => title,
    };
    if title.chars().count() > MAX_TITLE {
        return Err(BridgeError::new(format!(
            "Titles can be up to {MAX_TITLE} characters"
        )));
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(CreatedNote {
        id: format!("note-{nanos}"),
        title: title.to_string(),
    })
}
