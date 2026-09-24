//! Remembers window position/size (and maximized state) across runs.
//!
//! Stored as a tiny hand-rolled `key=value` text file under `%APPDATA%` -
//! not worth pulling in serde for four numbers and a bool.

use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            x: 100,
            y: 100,
            width: 1280,
            height: 800,
            maximized: false,
        }
    }
}

fn state_file() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA").filter(|value| !value.is_empty())?;
    Some(
        PathBuf::from(appdata)
            .join("YoutubeGlass")
            .join("window.state"),
    )
}

pub fn load() -> WindowState {
    let mut state = WindowState::default();
    let Some(path) = state_file() else {
        return state;
    };
    load_from(&mut state, &path);
    state
}

fn load_from(state: &mut WindowState, path: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };

    for line in text.trim_start_matches('\u{feff}').lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "x" => state.x = value.parse().unwrap_or(state.x),
            "y" => state.y = value.parse().unwrap_or(state.y),
            "width" => state.width = parse_dimension(value).unwrap_or(state.width),
            "height" => state.height = parse_dimension(value).unwrap_or(state.height),
            "maximized" => state.maximized = parse_bool(value).unwrap_or(state.maximized),
            _ => {}
        }
    }
}

fn parse_dimension(value: &str) -> Option<u32> {
    value
        .parse()
        .ok()
        .filter(|&size| size > 0 && size <= i32::MAX as u32)
}

/// Pulls `state`'s position back onto a currently-connected monitor if it
/// isn't already on one (e.g. the second monitor it was last placed on got
/// unplugged) - otherwise the window would restore entirely off-screen with
/// no way to get it back short of editing the state file by hand.
pub fn clamp_to_monitors(mut state: WindowState, monitors: &[(i32, i32, u32, u32)]) -> WindowState {
    const VISIBLE_MARGIN: i32 = 80; // require at least this much of the titlebar on-screen

    let on_screen = monitors.iter().any(|&(mx, my, mw, mh)| {
        i64::from(state.x) + i64::from(VISIBLE_MARGIN) > i64::from(mx)
            && i64::from(state.x) < i64::from(mx) + i64::from(mw)
            && i64::from(state.y) + i64::from(VISIBLE_MARGIN) > i64::from(my)
            && i64::from(state.y) < i64::from(my) + i64::from(mh)
    });

    if !on_screen {
        if let Some(&(mx, my, _, _)) = monitors.first() {
            state.x = mx.saturating_add(100);
            state.y = my.saturating_add(100);
        } else {
            state.x = 100;
            state.y = 100;
        }
    }
    state
}

pub fn save(state: WindowState) {
    let Some(path) = state_file() else { return };
    let _ = save_to(state, &path);
}

fn save_to(state: WindowState, path: &std::path::Path) -> std::io::Result<()> {
    if state.width == 0
        || state.height == 0
        || state.width > i32::MAX as u32
        || state.height > i32::MAX as u32
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid window dimensions",
        ));
    }
    atomic_write(path, |file| {
        write!(
            file,
            "x={}\ny={}\nwidth={}\nheight={}\nmaximized={}\n",
            state.x,
            state.y,
            state.width,
            state.height,
            if state.maximized { 1 } else { 0 }
        )
    })
}

fn parse_bool(value: &str) -> Option<bool> {
    if value == "1" || value.eq_ignore_ascii_case("true") {
        Some(true)
    } else if value == "0" || value.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

fn atomic_write(
    path: &std::path::Path,
    write_contents: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    let parent = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(parent)?;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing file name")
    })?;
    for _ in 0..128 {
        let mut temp_name = name.to_os_string();
        temp_name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let temp_path = parent.join(temp_name);
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = write_contents(&mut file).and_then(|()| file.sync_all());
        drop(file);
        let result = result.and_then(|()| std::fs::rename(&temp_path, path));
        if result.is_err() {
            let _ = std::fs::remove_file(&temp_path);
        }
        return result;
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no unused temporary file name",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: (i32, i32, u32, u32) = (0, 0, 1920, 1080);

    fn state_at(x: i32, y: i32) -> WindowState {
        WindowState {
            x,
            y,
            ..WindowState::default()
        }
    }

    #[test]
    fn on_screen_position_is_kept() {
        let s = clamp_to_monitors(state_at(500, 300), &[MONITOR]);
        assert_eq!((s.x, s.y), (500, 300));
    }

    #[test]
    fn off_screen_position_is_pulled_back() {
        // Entirely to the right of the only monitor (unplugged secondary).
        let s = clamp_to_monitors(state_at(2500, 300), &[MONITOR]);
        assert_eq!((s.x, s.y), (100, 100));
        // Far above.
        let s = clamp_to_monitors(state_at(500, -2000), &[MONITOR]);
        assert_eq!((s.x, s.y), (100, 100));
    }

    #[test]
    fn second_monitor_position_is_kept_when_present() {
        let second = (1920, 0, 1920, 1080);
        let s = clamp_to_monitors(state_at(2500, 300), &[MONITOR, second]);
        assert_eq!((s.x, s.y), (2500, 300));
    }

    #[test]
    fn no_monitors_falls_back_to_default_offset() {
        let s = clamp_to_monitors(state_at(2500, 300), &[]);
        assert_eq!((s.x, s.y), (100, 100));
    }

    fn test_directory() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "yg-{}-{}-{}",
            module_path!().replace("::", "-"),
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn failed_write_preserves_old_file_and_cleans_temp() {
        let dir = test_directory();
        let path = dir.join("state");
        std::fs::write(&path, b"original").unwrap();
        let result = atomic_write(&path, |file| {
            file.write_all(b"partial")?;
            Err(std::io::Error::other("injected write failure"))
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn failed_rename_cleans_temp() {
        let dir = test_directory();
        let path = dir.join("state");
        std::fs::create_dir(&path).unwrap();
        assert!(atomic_write(&path, |file| file.write_all(b"new")).is_err());
        assert!(path.is_dir());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn locked_destination_is_not_truncated() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = test_directory();
        let path = dir.join("state");
        std::fs::write(&path, b"original").unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(atomic_write(&path, |file| file.write_all(b"new")).is_err());
        drop(lock);
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn replacement_and_concurrent_writes_are_complete() {
        let dir = test_directory();
        let path = dir.join("state");
        atomic_write(&path, |file| file.write_all(b"old")).unwrap();
        atomic_write(&path, |file| file.write_all(b"replacement")).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let path = path.clone();
                std::thread::spawn(move || {
                    atomic_write(&path, |file| file.write_all(&vec![b'a' + index; 8192]))
                })
            })
            .collect();
        let successes = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().is_ok())
            .filter(|success| *success)
            .count();
        assert!(successes > 0);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 8192);
        assert!(bytes.iter().all(|byte| *byte == bytes[0]));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn round_trip_and_invalid_save() {
        let dir = test_directory();
        let path = dir.join("window.state");
        let expected = WindowState {
            x: -1920,
            y: -100,
            width: 1920,
            height: 1080,
            maximized: true,
        };
        save_to(expected, &path).unwrap();
        let mut loaded = WindowState::default();
        load_from(&mut loaded, &path);
        assert_eq!(loaded, expected);
        let original = std::fs::read(&path).unwrap();
        for width in [0, u32::MAX] {
            assert!(save_to(WindowState { width, ..expected }, &path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn malformed_geometry_preserves_previous_values() {
        let dir = test_directory();
        let path = dir.join("window.state");
        let mut state = WindowState::default();
        load_from(&mut state, &path);
        assert_eq!(state, WindowState::default());
        std::fs::write(&path, "\u{feff} x = -1920\r\ny=-2147483648\nx=2147483648\nwidth=1600\nwidth=0\nwidth=4294967295\nheight=-1\nheight=99999999999999999999\nmaximized=TRUE\nmaximized=bad\nunknown=0\n").unwrap();
        load_from(&mut state, &path);
        assert_eq!(
            state,
            WindowState {
                x: -1920,
                y: i32::MIN,
                width: 1600,
                maximized: true,
                ..WindowState::default()
            }
        );
        std::fs::write(&path, "maximized = False\n").unwrap();
        load_from(&mut state, &path);
        assert!(!state.maximized);
        std::fs::write(&path, [0xff]).unwrap();
        let previous = state;
        load_from(&mut state, &path);
        assert_eq!(state, previous);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let state = state_at(i32::MAX, i32::MIN);
        let monitor = (i32::MIN, i32::MIN, u32::MAX, u32::MAX);
        let result = clamp_to_monitors(state, &[monitor]);
        assert_eq!(result, state_at(i32::MIN + 100, i32::MIN + 100));
        let result = clamp_to_monitors(state_at(0, 0), &[(i32::MAX, i32::MAX, 1, 1)]);
        assert_eq!((result.x, result.y), (i32::MAX, i32::MAX));
    }
}
