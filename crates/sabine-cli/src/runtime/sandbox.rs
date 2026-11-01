/// An AppArmor profile that lets the hosts of this user's runtimes, and the
/// one in use when it lives elsewhere, create Chromium's sandbox.
pub(super) fn profile() -> Result<String, String> {
    let host = super::ensure_runtime_ready()?;
    let data_root = sabine_runtime::sabine_data_dir()
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let name = sabine_runtime::Fingerprint::default()
        .bytes(data_root.as_os_str().as_encoded_bytes())
        .finish();
    let managed = format!("{}/**/sabine-host", escape(&data_root)?);
    let host = host.canonicalize().map_err(|error| error.to_string())?;
    let mut output = String::from("abi <abi/4.0>,\ninclude <tunables/global>\n\n");
    output.push_str(&attachment(&name, &managed));
    if !host.starts_with(&data_root) {
        output.push('\n');
        output.push_str(&attachment(&format!("{name}-external"), &escape(&host)?));
    }
    Ok(output)
}

fn attachment(name: &str, path: &str) -> String {
    format!("profile sabine-{name} \"{path}\" flags=(unconfined) {{\n  userns,\n}}\n")
}

fn escape(path: &std::path::Path) -> Result<String, String> {
    let path = path.to_str().ok_or("AppArmor paths must be valid UTF-8")?;
    if path.chars().any(char::is_control) {
        return Err("AppArmor paths cannot contain control characters".into());
    }
    let mut result = String::new();
    for character in path.chars() {
        if "\\\"*?[]{}^,".contains(character) {
            result.push('\\');
        }
        result.push(character);
    }
    Ok(result)
}
