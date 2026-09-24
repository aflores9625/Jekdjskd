//! File logging so failures are visible in release builds, where the console
//! is hidden (`main.rs` sets `windows_subsystem = "windows"`). Errors used to
//! just vanish into `eprintln!`, which nobody could ever see outside a debug
//! build launched from a terminal.

use std::io::Write;
use std::path::PathBuf;

fn log_file() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(appdata).join("YoutubeGlass").join("app.log"))
}

pub fn log(msg: impl AsRef<str>) {
    write_line(msg.as_ref(), false);
}

fn write_line(msg: &str, sync: bool) {
    #[cfg(debug_assertions)]
    eprintln!("{msg}");

    let Some(path) = log_file() else { return };
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    // The log was append-only forever; keep one previous generation instead.
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "[{}] {msg}", timestamp());
        if sync {
            // With panic = "abort" the process dies right after the hook
            // returns; without an fsync the line may never reach the disk.
            let _ = file.sync_all();
        }
    }
}

const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// Rough `YYYY-MM-DD HH:MM:SS` timestamp without pulling in a datetime crate.
fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Not calendar-accurate leap-second handling, just human-readable enough for a log.
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    format!(
        "day{}+{:02}:{:02}:{:02} UTC",
        days,
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60
    )
}

/// Installs a panic hook that writes the panic message/location to the log
/// file before the process exits (still fires under `panic = "abort"" - the
/// hook runs first, then the runtime aborts).
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        write_line(&format!("PANIC: {info}"), true);
    }));
}
