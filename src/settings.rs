//! User-facing preferences, persisted across runs.
//!
//! Sibling to `window.state` in the same `%APPDATA%\YoutubeGlass\` folder.
//! Same hand-rolled `key=value` format and "no serde for a few booleans"
//! philosophy: window geometry and user prefs are kept in separate files
//! because they change on different cadences (geometry on every resize/move,
//! prefs only on an explicit toggle), but share the storage idiom.

use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Inject the Liquid Glass theme CSS (theme.rs).
    pub theme: bool,
    /// In-page ad hiding + auto-skip (features.rs).
    pub block_ads: bool,
    /// "Аналитика видео" stats panel on watch pages (analytics.rs).
    pub analytics: bool,
    /// Close button hides to tray instead of quitting. Only meaningful when a
    /// tray is actually available; ignored otherwise.
    pub close_to_tray: bool,
    /// Window stays above other windows (set_always_on_top).
    pub always_on_top: bool,
    /// Cinema mode default: when true, dim-around-the-player is on at launch;
    /// the player-bar toggle is always available regardless.
    pub cinema: bool,
    /// Prefer 1080p HD via YouTube's local quality preference (features.rs).
    /// Best-effort: YouTube may still downshift on slow links or small player.
    pub prefer_hd: bool,
    /// Discord Rich Presence ("Watching: <video>"). Inert until
    /// `discord_client_id` is set - the presence needs a Discord application
    /// id to speak through.
    pub discord_rpc: bool,
    /// Discord application id for Rich Presence. Empty = feature dormant.
    /// (Create one free at discord.com/developers/applications, name it
    /// "YouTube", paste the Application ID here.)
    pub discord_client_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: true,
            block_ads: true,
            analytics: true,
            close_to_tray: true,
            always_on_top: false,
            cinema: false,
            prefer_hd: false,
            discord_rpc: true,
            discord_client_id: String::new(),
        }
    }
}

fn settings_file() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA").filter(|value| !value.is_empty())?;
    Some(PathBuf::from(appdata).join("YoutubeGlass").join("settings"))
}

/// Host the in-app settings page is served at. On Windows, wry's custom
/// protocols are exposed as `http://<scheme>.<path>`, so registering scheme
/// `youtube-glass` yields `http://youtube-glass.settings/...` (the second
/// label is the path's first segment; ".settings" is just the page name we
/// navigate to). `links::is_allowed` whitelists this host so navigation
/// through it isn't bounced to the system browser.
pub const SETTINGS_HOST: &str = "youtube-glass.settings";

/// Minimal JSON serializer for the settings page. Five booleans only - hand
/// rolling this avoids pulling in serde just for the page's `fetch('/state')`
/// response, consistent with the rest of the file's "no serde" stance.
impl Settings {
    pub fn to_json(&self) -> String {
        let b = |v: bool| if v { "true" } else { "false" };
        format!(
            "{{\"theme\":{},\"block_ads\":{},\"analytics\":{},\"close_to_tray\":{},\"always_on_top\":{},\"cinema\":{},\"prefer_hd\":{},\"discord_rpc\":{},\"discord_configured\":{}}}",
            b(self.theme),
            b(self.block_ads),
            b(self.analytics),
            b(self.close_to_tray),
            b(self.always_on_top),
            b(self.cinema),
            b(self.prefer_hd),
            b(self.discord_rpc),
            b(!self.discord_client_id.is_empty()),
        )
    }
}

pub fn load() -> Settings {
    let mut s = Settings::default();
    let Some(path) = settings_file() else {
        return s;
    };
    load_from(&mut s, &path);
    s
}

/// Parses `path` into `s` in place; unrecognized lines fall back to the
/// existing field value (defaults if nothing was set). Split out so tests can
/// point at a temp file without mutating the process-wide APPDATA env var
/// (which would race across parallel test threads).
fn load_from(s: &mut Settings, path: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };

    for line in text.trim_start_matches('\u{feff}').lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key == "discord_client_id" {
            if valid_client_id(value) {
                s.discord_client_id = value.to_string();
            }
            continue;
        }
        let Some(on) = parse_bool(value) else {
            continue;
        };
        match key {
            "theme" => s.theme = on,
            "block_ads" => s.block_ads = on,
            "analytics" => s.analytics = on,
            "close_to_tray" => s.close_to_tray = on,
            "always_on_top" => s.always_on_top = on,
            "cinema" => s.cinema = on,
            "prefer_hd" => s.prefer_hd = on,
            "discord_rpc" => s.discord_rpc = on,
            _ => {}
        }
    }
}

/// Saves through a synced temporary file; failures leave the previous settings intact.
pub fn save(s: &Settings) {
    let Some(path) = settings_file() else {
        return;
    };
    let _ = save_to(s, &path);
}

fn valid_client_id(value: &str) -> bool {
    value.is_empty()
        || (value.bytes().all(|byte| byte.is_ascii_digit())
            && value.parse::<u64>().is_ok_and(|id| id != 0))
}

fn save_to(s: &Settings, path: &std::path::Path) -> std::io::Result<()> {
    if !valid_client_id(&s.discord_client_id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid Discord application id",
        ));
    }
    let one = |b: bool| if b { 1 } else { 0 };
    atomic_write(path, |file| {
        write!(
        file,
        "theme={}\nblock_ads={}\nanalytics={}\nclose_to_tray={}\nalways_on_top={}\ncinema={}\nprefer_hd={}\ndiscord_rpc={}\ndiscord_client_id={}\n",
        one(s.theme),
        one(s.block_ads),
        one(s.analytics),
        one(s.close_to_tray),
        one(s.always_on_top),
        one(s.cinema),
        one(s.prefer_hd),
        one(s.discord_rpc),
        s.discord_client_id,
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
    use std::path::PathBuf;

    /// A throwaway settings file in the OS temp dir. Each call gets a unique
    /// name (thread id + nanos), so parallel test threads don't collide.
    /// We test load_from/save_to directly (instead of mutating the process's
    /// APPDATA env var, which would race across threads).
    fn tmp_settings_file() -> PathBuf {
        let tid = format!("{:?}", std::thread::current().id());
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("yg-settings-test-{tid}-{nanos}"))
    }

    /// load_from on a nonexistent path leaves the incoming defaults untouched.
    #[test]
    fn load_from_missing_path_keeps_defaults() {
        let mut s = Settings::default();
        load_from(&mut s, &tmp_settings_file());
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = tmp_settings_file();
        let s = Settings {
            theme: false,
            block_ads: false,
            analytics: true,
            close_to_tray: false,
            always_on_top: true,
            cinema: false,
            prefer_hd: false,
            discord_rpc: false,
            discord_client_id: "123456789".to_string(),
        };
        save_to(&s, &path).unwrap();
        let mut loaded = Settings::default();
        load_from(&mut loaded, &path);
        assert_eq!(loaded, s);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_then_load_all_off() {
        let path = tmp_settings_file();
        let s = Settings {
            theme: false,
            block_ads: false,
            analytics: false,
            close_to_tray: false,
            always_on_top: false,
            cinema: false,
            prefer_hd: false,
            discord_rpc: false,
            discord_client_id: String::new(),
        };
        save_to(&s, &path).unwrap();
        let mut loaded = Settings::default();
        load_from(&mut loaded, &path);
        assert_eq!(loaded, s);
        let _ = std::fs::remove_file(&path);
    }

    /// Unrecognized/garbled lines are skipped; recognized-but-invalid values
    /// preserve the existing values; untouched keys keep their defaults.
    #[test]
    fn garbled_lines_fall_back_to_defaults() {
        let path = tmp_settings_file();
        std::fs::write(
            &path,
            "this is not a config line\ntheme=maybe\n=garbage\nblock_ads=0\n",
        )
        .unwrap();
        let mut s = Settings::default();
        load_from(&mut s, &path);
        assert!(s.theme);
        // block_ads=0 parsed correctly
        assert!(!s.block_ads);
        // everything else stays default
        assert!(s.analytics);
        assert!(s.close_to_tray);
        assert!(!s.always_on_top);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn accepts_true_spelling() {
        let path = tmp_settings_file();
        std::fs::write(&path, "theme=true\nanalytics=TRUE\nalways_on_top=True\n").unwrap();
        let mut s = Settings::default();
        load_from(&mut s, &path);
        assert!(s.theme);
        assert!(s.analytics);
        assert!(s.always_on_top);
        let _ = std::fs::remove_file(&path);
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
    fn whitespace_duplicates_and_invalid_values_are_safe() {
        let path = tmp_settings_file();
        std::fs::write(&path, "\u{feff} theme = FALSE\r\ntheme=typo\nanalytics=0\nanalytics=\ncinema=TrUe\ndiscord_client_id=12345\ndiscord_client_id=invalid\n").unwrap();
        let mut settings = Settings::default();
        load_from(&mut settings, &path);
        assert!(!settings.theme);
        assert!(!settings.analytics);
        assert!(settings.cinema);
        assert_eq!(settings.discord_client_id, "12345");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_client_id_cannot_inject_settings_or_replace_file() {
        let path = tmp_settings_file();
        let mut settings = Settings::default();
        save_to(&settings, &path).unwrap();
        let original = std::fs::read(&path).unwrap();
        for invalid in [
            "123\ntheme=0",
            "123\rtheme=0",
            "0",
            "18446744073709551616",
            "+123",
        ] {
            settings.discord_client_id = invalid.to_string();
            assert!(save_to(&settings, &path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
        std::fs::write(&path, [0xff]).unwrap();
        let mut loaded = Settings::default();
        load_from(&mut loaded, &path);
        assert_eq!(loaded, Settings::default());
        std::fs::remove_file(path).unwrap();
    }
}
