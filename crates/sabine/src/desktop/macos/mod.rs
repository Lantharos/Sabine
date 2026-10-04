mod app_delegate;
mod launch_agent;

use std::{collections::BTreeSet, env, path::PathBuf};

use dispatch2::DispatchQueue;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSBundle, NSDictionary, NSString, ns_string};
use sabine_platform::{DeepLinkRegistration, NativeMessagingHost};

pub(super) use super::hotkeys::GlobalShortcuts;
pub(super) use super::unix_instance::SingleInstanceGuard;
pub(super) use app_delegate::AppEvents;
pub(super) use launch_agent::write_autostart_entry;

/// AppKit's main thread. The tray icon, hotkeys and app delegate live there
/// and start once the main thread runs its event loop.
pub(super) struct UiThread;

#[derive(Clone)]
pub(super) struct UiQueue;

impl UiThread {
    pub(super) fn start() -> Result<Self, String> {
        Ok(Self)
    }

    pub(super) fn queue(&self) -> UiQueue {
        UiQueue
    }
}

impl UiQueue {
    pub(super) fn run(&self, task: impl FnOnce() + Send + 'static) {
        DispatchQueue::main().exec_async(task);
    }
}

pub(super) fn register_native_messaging_host(host: &NativeMessagingHost) -> Result<(), String> {
    super::native_messaging::write_manifests(host)
        .map(drop)
        .map_err(|error| error.to_string())
}

/// macOS routes URL schemes through the bundle's Info.plist, so this checks
/// that the bundle declares every scheme instead of registering anything.
pub(super) fn register_deep_links(registration: &DeepLinkRegistration) -> Result<(), String> {
    registration.validate()?;
    if registration.schemes.is_empty() {
        return Ok(());
    }
    let declared = declared_url_schemes();
    match registration
        .schemes
        .iter()
        .find(|scheme| !declared.contains(&scheme.to_ascii_lowercase()))
    {
        Some(scheme) => Err(format!(
            "URL scheme {scheme} is missing from the application bundle; add x-scheme-handler/{scheme} to app.mime_types in Sabine.toml and rebuild the macOS bundle"
        )),
        None => Ok(()),
    }
}

fn declared_url_schemes() -> BTreeSet<String> {
    let Some(types) = NSBundle::mainBundle()
        .objectForInfoDictionaryKey(ns_string!("CFBundleURLTypes"))
        .and_then(|types| types.downcast::<NSArray<AnyObject>>().ok())
    else {
        return BTreeSet::new();
    };
    types
        .iter()
        .filter_map(|entry| entry.downcast::<NSDictionary>().ok())
        .filter_map(|entry| {
            entry
                .objectForKey(ns_string!("CFBundleURLSchemes"))
                .and_then(|schemes| schemes.downcast::<NSArray<AnyObject>>().ok())
        })
        .flat_map(|schemes| schemes.iter().collect::<Vec<_>>())
        .filter_map(|scheme| scheme.downcast::<NSString>().ok())
        .map(|scheme| scheme.to_string().to_ascii_lowercase())
        .collect()
}

pub(super) fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is required for macOS desktop integration".to_string())
}
