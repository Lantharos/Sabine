pub(crate) fn show(title: &str, message: &str) {
    let message = format!(
        "{message}\n\nLogs: {}",
        super::diagnostics::log_directory().display()
    );
    eprintln!("{title}: {message}");
    show_platform(title, &message);
}

#[cfg(target_os = "windows")]
fn show_platform(title: &str, message: &str) {
    use windows::{
        Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW},
        core::PCWSTR,
    };
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let title = wide(title);
    let message = wide(message);
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}

#[cfg(target_os = "macos")]
fn show_platform(title: &str, message: &str) {
    let _ = sabine_runtime::background_command("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv\ndisplay alert (item 1 of argv) message (item 2 of argv) as critical buttons {\"Close\"} default button \"Close\"\nend run",
            "--", title, message,
        ])
        .status();
}

#[cfg(target_os = "linux")]
fn show_platform(title: &str, message: &str) {
    let message = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let _ = sabine_runtime::background_command("notify-send")
        .args([
            "--app-name=Sabine",
            "--urgency=critical",
            "--",
            title,
            &message,
        ])
        .status();
}
