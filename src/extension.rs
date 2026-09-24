//! Loads unpacked browser extensions (SponsorBlock, Return YouTube Dislike,
//! uBlock Origin Lite) into the WebView2 profile via the native COM API,
//! which wry does not wrap directly. (Classic uBlock Origin MV2 was bundled
//! once and removed: it blanks the whole page under current WebView2
//! runtimes. The MV3 uBO Lite build does not have that problem.)

use std::path::{Path, PathBuf};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2BrowserExtension, ICoreWebView2Profile7, ICoreWebView2_13,
};
use webview2_com::{
    BrowserExtensionRemoveCompletedHandler, CoTaskMemPWSTR,
    ProfileAddBrowserExtensionCompletedHandler, ProfileGetBrowserExtensionsCompletedHandler,
};
use windows::core::{Interface, PCWSTR, PWSTR};
use wry::{WebView, WebViewExtWindows};

/// Folder names under `extensions/` that ship with this app.
// uBlock Origin Lite disabled - causes black screen (blocks YouTube itself)
const BUNDLED: &[&str] = &["sponsorblock", "return-youtube-dislike"];

/// Extension IDs that break YouTube playback in WebView2 (blank player / no stream).
const HARMFUL_EXTENSION_IDS: &[&str] = &[
    "cjpalhdnlbbapaambmecoklfbocmfokc", // uBlock Origin
    "iphlfnjapbjhaklgklodocojofhibfel", // uBlock filters (seen in DevTools stacks)
];

const HARMFUL_NAME_MARKERS: &[&str] = &["ublock", "ubo lite", "adblock", "ad guard", "adguard"];

fn is_harmful(id: &str, name: &str) -> bool {
    let id = id.to_lowercase();
    let name = name.to_lowercase();
    HARMFUL_EXTENSION_IDS.iter().any(|bad| id == *bad)
        || HARMFUL_NAME_MARKERS.iter().any(|m| name.contains(m))
}

pub fn profile_extensions_dir() -> Option<PathBuf> {
    Some(
        crate::bundle::webview_data_dir()
            .join("EBWebView")
            .join("Default")
            .join("Extensions"),
    )
}

fn manifest_looks_harmful(ext_dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(ext_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let manifest = if path.file_name().is_some_and(|n| n == "manifest.json") {
            path
        } else if path.is_dir() {
            path.join("manifest.json")
        } else {
            continue;
        };
        if !manifest.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let lower = text.to_lowercase();
        if HARMFUL_NAME_MARKERS.iter().any(|m| lower.contains(m)) {
            return true;
        }
    }
    false
}

/// Remove uBlock / other aggressive blockers left in the WebView2 profile from older builds.
pub fn purge_harmful_extensions_from_profile() {
    let Some(root) = profile_extensions_dir() else {
        return;
    };
    if !root.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().to_lowercase();
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let harmful_id = HARMFUL_EXTENSION_IDS.iter().any(|bad| id == *bad);
        if harmful_id || manifest_looks_harmful(&path) {
            if std::fs::remove_dir_all(&path).is_ok() {
                removed.push(id);
            } else {
                crate::logging::log(format!(
                    "extension purge: failed to remove {}",
                    path.display()
                ));
            }
        }
    }
    if !removed.is_empty() {
        crate::logging::log(format!(
            "extension purge: removed harmful WebView2 extensions: {}",
            removed.join(", ")
        ));
    }
}

/// Uninstall ad blockers that an older build registered in the WebView2
/// profile. Unpacked extensions added through `AddBrowserExtension` stay
/// installed (and are loaded from their original folder) across launches, so
/// dropping one from `BUNDLED` or deleting `Default/Extensions` does not
/// unload it. A leftover uBlock Origin Lite blanks youtube.com and makes the
/// player report "Video unavailable".
pub fn remove_harmful_installed(webview: &WebView) -> windows::core::Result<()> {
    let controller = webview.controller();
    let core = unsafe { controller.CoreWebView2()? };
    let core13: ICoreWebView2_13 = core.cast()?;
    let profile = unsafe { core13.Profile()? };
    let profile7: ICoreWebView2Profile7 = profile.cast()?;

    let page = core.clone();
    let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
        move |result: windows::core::Result<()>, list| -> windows::core::Result<()> {
            if let Err(e) = result {
                crate::logging::log(format!("extension list unavailable: {e}"));
                return Ok(());
            }
            let Some(list) = list else { return Ok(()) };
            let mut count = 0u32;
            unsafe { list.Count(&mut count)? };
            for i in 0..count {
                let ext = unsafe { list.GetValueAtIndex(i)? };
                let (id, name) = unsafe { (read_string(|p| ext.Id(p)), read_string(|p| ext.Name(p))) };
                if !is_harmful(&id, &name) {
                    continue;
                }
                crate::logging::log(format!("removing harmful extension {name} ({id})"));
                let label = name.clone();
                let page = page.clone();
                let done = BrowserExtensionRemoveCompletedHandler::create(Box::new(
                    move |result: windows::core::Result<()>| -> windows::core::Result<()> {
                        match result {
                            // The first page load already ran with the blocker active.
                            Ok(()) => {
                                let _ = unsafe { page.Reload() };
                            }
                            Err(e) => crate::logging::log(format!("failed to remove {label}: {e}")),
                        }
                        Ok(())
                    },
                ));
                if let Err(e) = unsafe { ext.Remove(&done) } {
                    crate::logging::log(format!("failed to remove {name}: {e}"));
                }
            }
            Ok(())
        },
    ));
    unsafe { profile7.GetBrowserExtensions(&handler) }
}

unsafe fn read_string(get: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>) -> String {
    let mut raw = PWSTR::null();
    if get(&mut raw).is_err() {
        return String::new();
    }
    CoTaskMemPWSTR::from(raw).to_string()
}

/// Resolve every bundled extension folder that's actually present: prefer the
/// copy next to the executable, fall back to the path baked in at compile
/// time (the project tree, for `cargo run` during development).
pub fn bundled_paths() -> Vec<PathBuf> {
    debug_assert_eq!(BUNDLED, crate::bundle::EXTENSION_NAMES);
    crate::bundle::extension_dirs()
}

/// Install (and load) an unpacked extension from `folder` into the webview's
/// profile. Idempotent across runs: once added to the persistent user-data
/// folder it stays installed, so a re-add returning an error is harmless.
pub fn install(webview: &WebView, folder: &std::path::Path) -> windows::core::Result<()> {
    let controller = webview.controller();
    let core = unsafe { controller.CoreWebView2()? };
    let core13: ICoreWebView2_13 = core.cast()?;
    let profile = unsafe { core13.Profile()? };
    let profile7: ICoreWebView2Profile7 = profile.cast()?;

    let wide: Vec<u16> = folder
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let label = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let handler = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(
        move |result: windows::core::Result<()>,
              ext: Option<ICoreWebView2BrowserExtension>|
              -> windows::core::Result<()> {
            match &result {
                Ok(()) => crate::logging::log(format!(
                    "{label}: extension loaded (got handle: {})",
                    ext.is_some()
                )),
                Err(e) => crate::logging::log(format!("{label}: load callback error: {e}")),
            }
            Ok(())
        },
    ));

    crate::logging::log(format!("Loading extension from {}", folder.display()));
    unsafe { profile7.AddBrowserExtension(PCWSTR(wide.as_ptr()), &handler) }
}

// `encode_wide` lives on the OsStrExt trait.
use std::os::windows::ffi::OsStrExt;
