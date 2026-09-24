//! Minimal Win32 FFI, with inert stand-ins elsewhere so tests run on any OS.

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    type Handle = *mut c_void;

    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        dow: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        ms: u16,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n: u32) -> Handle;
        fn GetConsoleMode(h: Handle, m: *mut u32) -> i32;
        fn SetConsoleMode(h: Handle, m: u32) -> i32;
        fn SetConsoleTitleW(t: *const u16) -> i32;
        fn GetLocalTime(st: *mut SystemTime);
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: Handle, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    #[link(name = "shell32")]
    extern "system" {
        fn IsUserAnAdmin() -> i32;
        fn ShellExecuteW(hwnd: Handle, op: *const u16, file: *const u16, params: *const u16, dir: *const u16, show: i32) -> Handle;
        fn SHGetFolderPathW(hwnd: Handle, csidl: i32, token: Handle, flags: u32, path: *mut u16) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn is_admin() -> bool {
        unsafe { IsUserAnAdmin() != 0 }
    }

    pub fn error_box(caption: &str, text: &str) {
        const MB_ICONERROR: u32 = 0x10;
        let (t, c) = (wide(text), wide(caption));
        unsafe { MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), MB_ICONERROR) };
    }

    pub fn enable_vt() -> bool {
        const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
        const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
        unsafe {
            let h = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode = 0u32;
            if h.is_null() || GetConsoleMode(h, &mut mode) == 0 {
                return false;
            }
            SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        }
    }

    pub fn set_title(t: &str) {
        let w = wide(t);
        unsafe { SetConsoleTitleW(w.as_ptr()) };
    }

    /// (year, month, day, hour, minute) in local time.
    pub fn local_time() -> (i32, u32, u32, u32, u32) {
        let mut st = SystemTime::default();
        unsafe { GetLocalTime(&mut st) };
        (st.year as i32, st.month as u32, st.day as u32, st.hour as u32, st.minute as u32)
    }

    /// True when this process is alone in its console, i.e. started by double-click.
    pub fn owns_console() -> bool {
        let mut buf = [0u32; 4];
        unsafe { GetConsoleProcessList(buf.as_mut_ptr(), 4) <= 1 }
    }

    pub fn desktop_dir() -> Option<String> {
        const CSIDL_DESKTOPDIRECTORY: i32 = 0x10;
        let mut buf = [0u16; 260];
        let hr = unsafe { SHGetFolderPathW(std::ptr::null_mut(), CSIDL_DESKTOPDIRECTORY, std::ptr::null_mut(), 0, buf.as_mut_ptr()) };
        if hr != 0 {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
        Some(String::from_utf16_lossy(&buf[..len])).filter(|s| !s.is_empty())
    }

    /// Relaunches through UAC; false if the user declined or it failed.
    pub fn relaunch_elevated(args: &[String]) -> bool {
        let Ok(exe) = std::env::current_exe() else { return false };
        let params = args.iter().map(|a| format!("\"{}\"", a.replace('"', ""))).collect::<Vec<_>>().join(" ");
        let (op, file, params) = (wide("runas"), wide(&exe.to_string_lossy()), wide(&params));
        let r = unsafe { ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), file.as_ptr(), params.as_ptr(), std::ptr::null(), 1) };
        r as isize > 32
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn is_admin() -> bool {
        false
    }
    pub fn enable_vt() -> bool {
        true
    }
    pub fn set_title(_: &str) {}
    pub fn local_time() -> (i32, u32, u32, u32, u32) {
        let now = crate::text::unix_now();
        let d = crate::text::civil_from_days(now.div_euclid(86400));
        let s = now.rem_euclid(86400);
        (d.y, d.m, d.d, (s / 3600) as u32, (s % 3600 / 60) as u32)
    }
    pub fn owns_console() -> bool {
        false
    }
    pub fn desktop_dir() -> Option<String> {
        None
    }
    pub fn relaunch_elevated(_: &[String]) -> bool {
        false
    }
    pub fn error_box(caption: &str, text: &str) {
        eprintln!("{caption}: {text}");
    }
}

pub use imp::*;
