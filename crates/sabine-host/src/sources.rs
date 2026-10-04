use std::{path::Path, process::Command};

use sabine_bridge::{INSTALL_SCRIPT, clipboard};

pub(crate) const HOST_SOURCES: &[(&str, &str)] = &[
    ("CMakeLists.txt", include_str!("../shared/CMakeLists.txt")),
    ("main.cc", include_str!("../shared/main.cc")),
    ("entry.h", include_str!("../shared/entry.h")),
    (
        "runtime/probe.cc",
        include_str!("../shared/runtime/probe.cc"),
    ),
    ("runtime/probe.h", include_str!("../shared/runtime/probe.h")),
    ("main_mac.mm", include_str!("../shared/main_mac.mm")),
    (
        "mac/Info.plist.in",
        include_str!("../shared/mac/Info.plist.in"),
    ),
    ("app/app.cc", include_str!("../shared/app/app.cc")),
    ("app/app.h", include_str!("../shared/app/app.h")),
    ("app/bridge.cc", include_str!("../shared/app/bridge.cc")),
    ("app/bridge.h", include_str!("../shared/app/bridge.h")),
    (
        "app/clipboard.cc",
        include_str!("../shared/app/clipboard.cc"),
    ),
    ("app/clipboard.h", include_str!("../shared/app/clipboard.h")),
    ("app/scheme.cc", include_str!("../shared/app/scheme.cc")),
    ("app/scheme.h", include_str!("../shared/app/scheme.h")),
    (
        "common/bytes_message.cc",
        include_str!("../shared/common/bytes_message.cc"),
    ),
    (
        "common/bytes_message.h",
        include_str!("../shared/common/bytes_message.h"),
    ),
    ("common/json.cc", include_str!("../shared/common/json.cc")),
    ("common/json.h", include_str!("../shared/common/json.h")),
    (
        "common/bridge_policy.h",
        include_str!("../shared/common/bridge_policy.h"),
    ),
    ("guest/input.cc", include_str!("../shared/guest/input.cc")),
    ("guest/input.h", include_str!("../shared/guest/input.h")),
    (
        "guest/manager.cc",
        include_str!("../shared/guest/manager.cc"),
    ),
    ("guest/manager.h", include_str!("../shared/guest/manager.h")),
    ("osr/handler.cc", include_str!("../shared/osr/handler.cc")),
    ("osr/handler.h", include_str!("../shared/osr/handler.h")),
    ("osr/bridge.cc", include_str!("../shared/osr/bridge.cc")),
    (
        "osr/browser/callbacks.cc",
        include_str!("../shared/osr/browser/callbacks.cc"),
    ),
    (
        "osr/browser/context_menu.cc",
        include_str!("../shared/osr/browser/context_menu.cc"),
    ),
    (
        "osr/browser/clipboard.cc",
        include_str!("../shared/osr/browser/clipboard.cc"),
    ),
    (
        "osr/browser/recovery.cc",
        include_str!("../shared/osr/browser/recovery.cc"),
    ),
    (
        "osr/browser/downloads.cc",
        include_str!("../shared/osr/browser/downloads.cc"),
    ),
    (
        "osr/browser/permissions.cc",
        include_str!("../shared/osr/browser/permissions.cc"),
    ),
    (
        "osr/input/drag.cc",
        include_str!("../shared/osr/input/drag.cc"),
    ),
    (
        "osr/guest/commands.cc",
        include_str!("../shared/osr/guest/commands.cc"),
    ),
    (
        "osr/guest/lifecycle.cc",
        include_str!("../shared/osr/guest/lifecycle.cc"),
    ),
    (
        "osr/input/ime.cc",
        include_str!("../shared/osr/input/ime.cc"),
    ),
    ("osr/input/ime.h", include_str!("../shared/osr/input/ime.h")),
    (
        "osr/input/edit_commands_mac.cc",
        include_str!("../shared/osr/input/edit_commands_mac.cc"),
    ),
    (
        "osr/input/input.cc",
        include_str!("../shared/osr/input/input.cc"),
    ),
    (
        "osr/browser/screen.cc",
        include_str!("../shared/osr/browser/screen.cc"),
    ),
    (
        "osr/browser/screen.h",
        include_str!("../shared/osr/browser/screen.h"),
    ),
    ("osr/tasks.cc", include_str!("../shared/osr/tasks.cc")),
    ("osr/tasks.h", include_str!("../shared/osr/tasks.h")),
    (
        "osr/transport.cc",
        include_str!("../shared/osr/transport.cc"),
    ),
    (
        "osr/utilities.cc",
        include_str!("../shared/osr/utilities.cc"),
    ),
    ("osr/utilities.h", include_str!("../shared/osr/utilities.h")),
    (
        "osr/window_state.cc",
        include_str!("../shared/osr/window_state.cc"),
    ),
    (
        "osr/accelerated/paint.cc",
        include_str!("../shared/osr/accelerated/paint.cc"),
    ),
    (
        "osr/accelerated/paint.h",
        include_str!("../shared/osr/accelerated/paint.h"),
    ),
    (
        "osr/accelerated/protocol.cc",
        include_str!("../shared/osr/accelerated/protocol.cc"),
    ),
    (
        "osr/accelerated/protocol.h",
        include_str!("../shared/osr/accelerated/protocol.h"),
    ),
    (
        "osr/accelerated/windows/d3d11_copy.cc",
        include_str!("../shared/osr/accelerated/windows/d3d11_copy.cc"),
    ),
    (
        "osr/accelerated/windows/d3d11_copy.h",
        include_str!("../shared/osr/accelerated/windows/d3d11_copy.h"),
    ),
    (
        "osr/accelerated/macos/iosurface_copy.h",
        include_str!("../shared/osr/accelerated/macos/iosurface_copy.h"),
    ),
    (
        "osr/accelerated/macos/iosurface_copy.mm",
        include_str!("../shared/osr/accelerated/macos/iosurface_copy.mm"),
    ),
    (
        "osr/accelerated/macos/surface_broker.cc",
        include_str!("../shared/osr/accelerated/macos/surface_broker.cc"),
    ),
    (
        "osr/accelerated/macos/surface_broker.h",
        include_str!("../shared/osr/accelerated/macos/surface_broker.h"),
    ),
    (
        "osr/paint/shared_pool.cc",
        include_str!("../shared/osr/paint/shared_pool.cc"),
    ),
    (
        "osr/paint/shared_pool.h",
        include_str!("../shared/osr/paint/shared_pool.h"),
    ),
];

pub(crate) fn write_host_source(source_dir: &Path) -> Result<(), String> {
    for (name, body) in HOST_SOURCES {
        let path = source_dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, body).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        source_dir.join("sabine_host_protocol.h"),
        format!(
            "#pragma once\n#define SABINE_HOST_PROTOCOL_VERSION \"{}\"\n",
            crate::HOST_PROTOCOL_VERSION
        ),
    )
    .map_err(|error| error.to_string())?;
    for (header, source, constant, script) in [
        (
            "sabine_bridge_js.h",
            "web_bridge.js",
            "SABINE_BRIDGE_JS_RAW",
            INSTALL_SCRIPT,
        ),
        (
            "sabine_clipboard_js.h",
            "clipboard.js",
            "SABINE_CLIPBOARD_JS_RAW",
            clipboard::SCRIPT,
        ),
    ] {
        std::fs::write(
            source_dir.join(header),
            script_header(source, constant, script),
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn script_header(source: &str, constant: &str, script: &str) -> String {
    format!(
        "// AUTO-GENERATED by sabine-host from\n// crates/sabine-bridge/src/{source}. Do not edit by hand.\n#pragma once\nconstexpr const char* {constant} = R\"js({script})js\";\n"
    )
}

pub(crate) fn host_source_fingerprint() -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for (name, body) in HOST_SOURCES {
        hash_bytes(&mut hash, name.as_bytes());
        hash_bytes(&mut hash, &[0xff]);
        hash_bytes(&mut hash, body.as_bytes());
    }
    hash_bytes(&mut hash, INSTALL_SCRIPT.as_bytes());
    hash_bytes(&mut hash, clipboard::SCRIPT.as_bytes());
    hash_bytes(&mut hash, crate::HOST_PROTOCOL_VERSION.as_bytes());
    format!("{hash:016x}")
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = (*hash).wrapping_mul(0x100000001b3);
    }
}

pub(crate) fn command_available(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .is_ok_and(|status| status.success())
}
