#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::time::{SystemTime, UNIX_EPOCH};

use sabine::prelude::*;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Deserialize, Type)]
struct NewNote {}

#[derive(Serialize, Type)]
struct CreatedNote {
    id: String,
}

fn main() {
    SabineWindow::main(|window| {
        Ok(window
            .app()
            .frameless()
            .glass()
            .app_chrome(AppChrome::default())
            .content_suffix("?chrome=app")
            .bridge_typed("notes.create", |NewNote {}| Ok(create_note())))
    });
}

fn create_note() -> CreatedNote {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    CreatedNote {
        id: format!("note-{nanos}"),
    }
}
