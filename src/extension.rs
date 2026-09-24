//! Loads unpacked browser extensions (SponsorBlock, Return YouTube Dislike,
//! uBlock Origin Lite) into the WebView2 profile via the native COM API,
//! which wry does not wrap directly. (Classic uBlock Origin MV2 was bundled
//! once and removed: it blanks the whole page under current WebView2
//! runtimes. The MV3 uBO Lite build does not have that problem.)

use std::path::{Path, PathBuf};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2BrowserExtension, ICoreWebView2Profile7, ICoreWebView2_13,
};
use webview2_com::ProfileAddBrowserExtensionCompletedHandler;
use windows::core::{Interface, PCWSTR};
use wry::{WebView, WebViewExtWindows};

/// Folder names under `extensions/` that ship with this app.
// uBlock Origin Lite disabled - causes black screen (blocks YouTube itself)
const BUNDLED: &[&str] = &["sponsorblock", "return-youtube-dislike"];

/// Extension IDs that break YouTube playback in WebView2 (blank player / no stream).
const HARMFUL_EXTENSION_IDS: &[&str] = &[
    "cjpalhdnlbbapaambmecoklfbocmfokc", // uBlock Origin
    "iphlfnjapbjhaklgklodocojofhibfel", // uBlock filters (seen in DevTools stacks)
];

const HARMFUL_NAME_MARKERS: &[&str] = &["ublock", "adblock", "ad guard", "adguard"];

pub fn profile_extensions_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let folder = format!("{}.WebView2", exe.file_name()?.to_string_lossy());
    Some(
        dir.join(folder)
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

/// Resolve every bundled extension folder that's actually present: prefer the
/// copy next to the executable, fall back to the path baked in at compile
/// time (the project tree, for `cargo run` during development).
pub fn bundled_paths() -> Vec<PathBuf> {
    BUNDLED
        .iter()
        .filter_map(|name| resolve(name))
        .collect()
}

fn resolve(name: &str) -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("extensions").join(name);
            if p.join("manifest.json").exists() {
                return Some(p);
            }
        }
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("extensions")
        .join(name);
    if p.join("manifest.json").exists() {
        Some(p)
    } else {
        None
    }
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
