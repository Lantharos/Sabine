// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Sabine's NSIS setup registers a quiet uninstaller under the app's ID, which
// copies itself to the temporary folder and returns at once, so the removal is
// only over once its uninstall entry is gone. Sabine's MSI packages share an
// upgrade code derived from the app's ID, which finds every installed product.

use std::time::{Duration, Instant};

use sabine_runtime::background_command;
use windows::{
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::ApplicationInstallationAndServicing::MsiEnumRelatedProductsW,
    },
    core::{HSTRING, PWSTR},
};

use crate::{ServiceError, ServiceResult, windows_registry};

const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL_WAIT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(250);
const MSI_UNKNOWN_PRODUCT: i32 = 1605;
const MSI_RESTART_REQUIRED: i32 = 3010;

/// Runs the uninstallers `id`'s setup registered, if it registered any.
pub(super) fn run_uninstaller(id: &str) -> ServiceResult<()> {
    run_nsis_uninstaller(id)?;
    for product in msi_products(id) {
        let status = background_command("msiexec")
            .args(["/x", &product, "/qn", "REBOOT=ReallySuppress"])
            .status()?;
        if !matches!(
            status.code(),
            Some(0 | MSI_UNKNOWN_PRODUCT | MSI_RESTART_REQUIRED)
        ) {
            return Err(ServiceError::Update(format!(
                "the uninstaller of {id} stopped with {status}"
            )));
        }
    }
    Ok(())
}

fn run_nsis_uninstaller(id: &str) -> ServiceResult<()> {
    let key = format!(r"{UNINSTALL_KEY}\{id}");
    let Some(command) = windows_registry::current_user_value(&key, "QuietUninstallString") else {
        return Ok(());
    };
    let (program, arguments) = split_command(&command);
    background_command(program).args(arguments).status()?;
    let started = Instant::now();
    while windows_registry::current_user_value(&key, "UninstallString").is_some() {
        if started.elapsed() > UNINSTALL_WAIT {
            return Err(ServiceError::Update(format!(
                "the uninstaller of {id} didn't finish"
            )));
        }
        std::thread::sleep(POLL);
    }
    Ok(())
}

fn split_command(command: &str) -> (&str, Vec<&str>) {
    let command = command.trim();
    let (program, rest) = match command.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"').unwrap_or((quoted, "")),
        None => command.split_once(' ').unwrap_or((command, "")),
    };
    (program, rest.split_whitespace().collect())
}

fn msi_products(id: &str) -> Vec<String> {
    let upgrade_code = HSTRING::from(format!("{{{}}}", super::installer_guid(id)));
    let mut products = Vec::new();
    for index in 0.. {
        let mut product = [0u16; 39];
        let status = unsafe {
            MsiEnumRelatedProductsW(&upgrade_code, None, index, PWSTR(product.as_mut_ptr()))
        };
        if status != ERROR_SUCCESS.0 {
            break;
        }
        let length = product
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(product.len());
        products.push(String::from_utf16_lossy(&product[..length]));
    }
    products
}
