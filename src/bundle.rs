//! Single-file distribution: WebView2Loader.dll and the bundled extensions
//! are compiled into the exe (see build.rs) and unpacked on demand into
//! `%LOCALAPPDATA%\YoutubeGlass`.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use windows::core::{HRESULT, HSTRING, PCSTR};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH,
};

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

const LOADER_DLL: &[u8] = include_bytes!("../vendor/webview2/WebView2Loader.dll");

fn app_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("YoutubeGlass")
}

/// WebView2 user-data folder. Installs that already have a profile next to
/// the exe (the old default) keep it, so sign-in survives the update.
pub fn webview_data_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let (Some(dir), Some(name)) = (exe.parent(), exe.file_name()) {
            let legacy = dir.join(format!("{}.WebView2", name.to_string_lossy()));
            if legacy.is_dir() {
                return legacy;
            }
        }
    }
    app_dir().join("WebView2")
}

/// Writes `data` to `path` unless an identical file is already there.
fn write_if_changed(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if std::fs::read(path).map(|old| old == data).unwrap_or(false) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp-ytg");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

/// Unpacks the embedded extensions and returns their folders. The location
/// is stable across versions: WebView2 keys unpacked extensions by path.
pub fn extension_dirs() -> Vec<PathBuf> {
    let root = app_dir().join("extensions");
    let marker = root.join(".bundle");
    let stamp = BUNDLE_HASH.to_string();
    let fresh = std::fs::read_to_string(&marker).map(|s| s == stamp).unwrap_or(false)
        && EXTENSION_NAMES.iter().all(|n| root.join(n).join("manifest.json").exists());
    if !fresh {
        let mut ok = true;
        for name in EXTENSION_NAMES {
            let _ = std::fs::remove_dir_all(root.join(name));
        }
        for (rel, data) in EXTENSION_FILES {
            if let Err(e) = write_if_changed(&root.join(rel), data) {
                crate::logging::log(format!("unpack {rel} failed: {e}"));
                ok = false;
            }
        }
        if ok {
            let _ = std::fs::write(&marker, &stamp);
        }
    }
    EXTENSION_NAMES
        .iter()
        .map(|name| root.join(name))
        .filter(|p| p.join("manifest.json").exists())
        .collect()
}

type CreateEnvFn =
    unsafe extern "system" fn(*const u16, *const u16, *mut c_void, *mut c_void) -> HRESULT;

fn loader() -> Option<CreateEnvFn> {
    static LOADER: OnceLock<Option<usize>> = OnceLock::new();
    let addr = *LOADER.get_or_init(|| {
        let path = app_dir().join("bin").join("WebView2Loader.dll");
        if let Err(e) = write_if_changed(&path, LOADER_DLL) {
            crate::logging::log(format!("unpack WebView2Loader.dll failed: {e}"));
        }
        unsafe {
            let module =
                LoadLibraryExW(&HSTRING::from(path.as_os_str()), None, LOAD_WITH_ALTERED_SEARCH_PATH)
                    .map_err(|e| crate::logging::log(format!("load WebView2Loader.dll: {e}")))
                    .ok()?;
            GetProcAddress(module, PCSTR(b"CreateCoreWebView2EnvironmentWithOptions\0".as_ptr()))
                .map(|f| f as usize)
        }
    });
    addr.map(|a| unsafe { std::mem::transmute::<usize, CreateEnvFn>(a) })
}

/// Defines the only WebView2Loader export wry uses, so the linker resolves
/// webview2-com's import here instead of against WebView2Loader.dll and the
/// exe carries no load-time dependency on it. If a dependency update starts
/// importing another loader export, `objdump -p` on the exe will list
/// WebView2Loader.dll again.
#[no_mangle]
pub unsafe extern "system" fn CreateCoreWebView2EnvironmentWithOptions(
    browser_executable_folder: *const u16,
    user_data_folder: *const u16,
    environment_options: *mut c_void,
    environment_created_handler: *mut c_void,
) -> HRESULT {
    match loader() {
        Some(create) => create(
            browser_executable_folder,
            user_data_folder,
            environment_options,
            environment_created_handler,
        ),
        None => E_FAIL,
    }
}
