use std::{fs, path::Path};

use sabine_bridge::BridgeBindings;

const TARGET: &str = "SABINE_BINDINGS";
const CHECK: &str = "SABINE_BINDINGS_CHECK";

/// When `sabine bindings` runs the app, writes or checks its TypeScript
/// bindings and exits instead of opening the window.
pub(crate) fn export_requested_bindings(bindings: &BridgeBindings) {
    let Some(target) = std::env::var_os(TARGET) else {
        return;
    };
    let checking = std::env::var_os(CHECK).is_some();
    match export(bindings, Path::new(&target), checking) {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn export(bindings: &BridgeBindings, target: &Path, checking: bool) -> Result<(), String> {
    let rendered = bindings
        .render()
        .map_err(|error| format!("could not render the bindings: {error}"))?;
    if fs::read_to_string(target).is_ok_and(|current| current == rendered) {
        return Ok(());
    }
    if checking {
        return Err(format!(
            "{} is out of date; run `sabine bindings` to update it",
            target.display()
        ));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    fs::write(target, rendered)
        .map_err(|error| format!("could not write {}: {error}", target.display()))
}
