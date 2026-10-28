use std::{path::Path, process::Command};

use sabine_bridge::{INSTALL_SCRIPT, clipboard, media};

pub(crate) const HOST_SOURCES: &[(&str, &str)] = &[
    ("CMakeLists.txt", include_str!("../shared/CMakeLists.txt")),
    ("main.cc", include_str!("../shared/main.cc")),
    ("entry.h", include_str!("../shared/entry.h")),
    (
        "runtime/probe.cc",
        include_str!("../shared/runtime/probe.cc"),
    ),
    ("runtime/probe.h", include_str!("../shared/runtime/probe.h")),
    (
        "runtime/identity.cc",
        include_str!("../shared/runtime/identity.cc"),
    ),
    (
        "runtime/identity.h",
        include_str!("../shared/runtime/identity.h"),
    ),
    (
        "runtime/renderers.cc",
        include_str!("../shared/runtime/renderers.cc"),
    ),
    (
        "runtime/renderers.h",
        include_str!("../shared/runtime/renderers.h"),
    ),
    (
        "runtime/server.cc",
        include_str!("../shared/runtime/server.cc"),
    ),
    (
        "runtime/server.h",
        include_str!("../shared/runtime/server.h"),
    ),
    (
        "display/offer.cc",
        include_str!("../shared/display/offer.cc"),
    ),
    ("display/offer.h", include_str!("../shared/display/offer.h")),
    (
        "display/private_display.cc",
        include_str!("../shared/display/private_display.cc"),
    ),
    (
        "display/private_display.h",
        include_str!("../shared/display/private_display.h"),
    ),
    (
        "display/protocols.cc",
        include_str!("../shared/display/protocols.cc"),
    ),
    (
        "display/protocols.h",
        include_str!("../shared/display/protocols.h"),
    ),
    (
        "display/wayland.cc",
        include_str!("../shared/display/wayland.cc"),
    ),
    (
        "display/wayland.h",
        include_str!("../shared/display/wayland.h"),
    ),
    ("main_mac.mm", include_str!("../shared/main_mac.mm")),
    (
        "mac/Info.plist.in",
        include_str!("../shared/mac/Info.plist.in"),
    ),
    ("app/app.cc", include_str!("../shared/app/app.cc")),
    ("app/app.h", include_str!("../shared/app/app.h")),
    ("app/profile.cc", include_str!("../shared/app/profile.cc")),
    ("app/profile.h", include_str!("../shared/app/profile.h")),
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
        "osr/browser/file_dialog.cc",
        include_str!("../shared/osr/browser/file_dialog.cc"),
    ),
    (
        "osr/browser/media_state.cc",
        include_str!("../shared/osr/browser/media_state.cc"),
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
        "osr/print/document.cc",
        include_str!("../shared/osr/print/document.cc"),
    ),
    (
        "osr/print/document.h",
        include_str!("../shared/osr/print/document.h"),
    ),
    (
        "osr/print/documents.cc",
        include_str!("../shared/osr/print/documents.cc"),
    ),
    (
        "osr/print/page.cc",
        include_str!("../shared/osr/print/page.cc"),
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
    (
        "osr/window_state.cc",
        include_str!("../shared/osr/window_state.cc"),
    ),
    (
        "osr/common/registry.cc",
        include_str!("../shared/osr/common/registry.cc"),
    ),
    (
        "osr/common/registry.h",
        include_str!("../shared/osr/common/registry.h"),
    ),
    (
        "osr/common/strings.cc",
        include_str!("../shared/osr/common/strings.cc"),
    ),
    (
        "osr/common/strings.h",
        include_str!("../shared/osr/common/strings.h"),
    ),
    (
        "osr/input/key_codes.cc",
        include_str!("../shared/osr/input/key_codes.cc"),
    ),
    (
        "osr/input/key_codes.h",
        include_str!("../shared/osr/input/key_codes.h"),
    ),
    (
        "osr/transport/control.cc",
        include_str!("../shared/osr/transport/control.cc"),
    ),
    (
        "osr/transport/message_kinds.h",
        include_str!("../shared/osr/transport/message_kinds.h"),
    ),
    (
        "osr/transport/paint_batch.cc",
        include_str!("../shared/osr/transport/paint_batch.cc"),
    ),
    (
        "osr/transport/socket.cc",
        include_str!("../shared/osr/transport/socket.cc"),
    ),
    (
        "osr/transport/tasks.cc",
        include_str!("../shared/osr/transport/tasks.cc"),
    ),
    (
        "osr/transport/tasks.h",
        include_str!("../shared/osr/transport/tasks.h"),
    ),
    (
        "osr/transport/wire.cc",
        include_str!("../shared/osr/transport/wire.cc"),
    ),
    (
        "osr/transport/wire.h",
        include_str!("../shared/osr/transport/wire.h"),
    ),
    (
        "osr/accelerated/damage.h",
        include_str!("../shared/osr/accelerated/damage.h"),
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
        "osr/accelerated/linux/device_loss.cc",
        include_str!("../shared/osr/accelerated/linux/device_loss.cc"),
    ),
    (
        "osr/accelerated/linux/dmabuf_copy.cc",
        include_str!("../shared/osr/accelerated/linux/dmabuf_copy.cc"),
    ),
    (
        "osr/accelerated/linux/dmabuf_copy.h",
        include_str!("../shared/osr/accelerated/linux/dmabuf_copy.h"),
    ),
    (
        "osr/accelerated/linux/dmabuf_image.cc",
        include_str!("../shared/osr/accelerated/linux/dmabuf_image.cc"),
    ),
    (
        "osr/accelerated/linux/dmabuf_image.h",
        include_str!("../shared/osr/accelerated/linux/dmabuf_image.h"),
    ),
    (
        "osr/accelerated/linux/settle.cc",
        include_str!("../shared/osr/accelerated/linux/settle.cc"),
    ),
    (
        "osr/accelerated/linux/vulkan_context.cc",
        include_str!("../shared/osr/accelerated/linux/vulkan_context.cc"),
    ),
    (
        "osr/accelerated/linux/vulkan_context.h",
        include_str!("../shared/osr/accelerated/linux/vulkan_context.h"),
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

const HOST_ASSETS: &[(&str, &[u8])] = &[(
    "mac/sabine.icns",
    include_bytes!("../shared/mac/sabine.icns"),
)];

fn host_files() -> impl Iterator<Item = (&'static str, &'static [u8])> {
    HOST_SOURCES
        .iter()
        .map(|(name, body)| (*name, body.as_bytes()))
        .chain(HOST_ASSETS.iter().copied())
}

pub(crate) fn write_host_source(source_dir: &Path) -> Result<(), String> {
    for (name, body) in host_files() {
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
        (
            "sabine_media_state_js.h",
            "media_state.js",
            "SABINE_MEDIA_STATE_JS_RAW",
            media::STATE_SCRIPT,
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
        "// AUTO-GENERATED by sabine-host from\n// crates/sabine-bridge/src/scripts/{source}. Do not edit by hand.\n#pragma once\nconstexpr const char* {constant} = R\"js({script})js\";\n"
    )
}

pub(crate) fn host_source_fingerprint() -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for (name, body) in host_files() {
        hash_bytes(&mut hash, name.as_bytes());
        hash_bytes(&mut hash, &[0xff]);
        hash_bytes(&mut hash, body);
    }
    hash_bytes(&mut hash, INSTALL_SCRIPT.as_bytes());
    hash_bytes(&mut hash, clipboard::SCRIPT.as_bytes());
    hash_bytes(&mut hash, media::STATE_SCRIPT.as_bytes());
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
