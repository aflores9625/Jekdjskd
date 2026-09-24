//! Ensures only one copy of the app runs at a time. A second launch just
//! activates the already-running window and exits immediately.

use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE};

/// The window class name this app registers its main window under
/// (set via `WindowBuilderExtWindows::with_window_classname`), used here to
/// find an already-running instance without depending on the (dynamic)
/// window title.
pub const WINDOW_CLASS_NAME: &str = "YoutubeGlassMainWindow";

const MUTEX_NAME: &str = "Local\\YoutubeGlass-SingleInstance-Mutex";

/// Returned handle must be kept alive for the process's lifetime (letting it
/// drop at the end of `main` when the event loop exits is fine - the OS
/// releases it on process exit either way).
pub struct InstanceLock(HANDLE);

impl Drop for InstanceLock {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// If another instance is already running, brings its window to the front
/// and returns `None` (caller should exit immediately without creating any
/// window). Otherwise returns `Some(lock)` and the caller should proceed
/// with normal startup.
///
/// `wait` is set for a self-restart: the previous instance still holds the
/// mutex while it shuts down, so the new one must wait for it instead of
/// treating it as a second launch (which made "Restart" simply quit).
pub fn acquire(wait: bool) -> Option<InstanceLock> {
    let deadline = std::time::Instant::now() + RESTART_WAIT;
    loop {
        match try_acquire() {
            Acquire::Locked(lock) => return Some(lock),
            Acquire::Busy if wait && std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Acquire::Busy => {
                activate_existing();
                return None;
            }
        }
    }
}

/// Command-line flag passed to the relaunched copy on restart.
pub const RESTART_ARG: &str = "--restarted";

const RESTART_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

enum Acquire {
    Locked(InstanceLock),
    Busy,
}

fn try_acquire() -> Acquire {
    let name = HSTRING::from(MUTEX_NAME);
    let handle = unsafe { CreateMutexW(None, false, &name) };

    let Ok(handle) = handle else {
        // Couldn't even create the mutex; fail open rather than block startup.
        return Acquire::Locked(InstanceLock(HANDLE::default()));
    };

    if unsafe { windows::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Acquire::Busy;
    }

    Acquire::Locked(InstanceLock(handle))
}

fn activate_existing() {
    let class_name = HSTRING::from(WINDOW_CLASS_NAME);
    if let Ok(hwnd) = unsafe { FindWindowW(&class_name, None) } {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}
