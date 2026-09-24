//! Keeps navigation inside the YouTube/Google domain family in-app; anything
//! else (a link in a video description, a sponsor URL, etc.) opens in the
//! user's default browser instead of hijacking this window.

use windows::core::HSTRING;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const ALLOWED_DOMAINS: &[&str] = &[
    "youtube.com",
    "youtube-nocookie.com",
    "youtu.be",
    "ytimg.com",
    "ggpht.com",
    "googlevideo.com",
    "google.com",
    "gstatic.com",
    "googleapis.com",
    "googleusercontent.com",
    // The in-app settings page is served from the youtube-glass custom
    // protocol as http://youtube-glass.settings/... on Windows. It's our own
    // origin, navigated only from the tray menu, so it must clear the
    // in-app allowlist (otherwise it'd be bounced to the system browser).
    crate::settings::SETTINGS_HOST,
];

/// Google sign-in bounces through country domains (google.ru, google.co.uk,
/// google.com.br, ...) which can't all be listed. Recognize any host whose
/// registrable part is `google` followed by 1-2 short TLD labels.
fn is_google_country_domain(host: &str) -> bool {
    let parts: Vec<&str> = host.split('.').collect();
    for (i, p) in parts.iter().enumerate() {
        if *p == "google" {
            let tld_labels = &parts[i + 1..];
            return matches!(tld_labels.len(), 1 | 2)
                && tld_labels.iter().all(|l| !l.is_empty() && l.len() <= 3);
        }
    }
    false
}

/// True if `url`'s host is YouTube/Google itself (including auth/consent
/// subdomains needed for sign-in), so it's safe to navigate to in-app.
///
/// Non-http(s) URLs (`about:blank`, `chrome-extension://...`, devtools pages,
/// etc.) are allowed through unconditionally - those are internal WebView2/
/// extension pages, not something an attacker-controlled page can point at
/// to hijack the window the way an external `https://` link could.
pub fn is_allowed(url: &str) -> bool {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return true;
    }
    match host_of(url) {
        Some(host) => {
            ALLOWED_DOMAINS
                .iter()
                .any(|d| host == *d || host.ends_with(&format!(".{d}")))
                || is_google_country_domain(&host)
        }
        None => false,
    }
}

/// Opens `url` in the system's default browser via `ShellExecuteW` (the
/// "open" verb launches the registered URL handler directly - no shell/cmd
/// parsing involved, so this is safe even for untrusted, attacker-controlled
/// URLs from page content).
pub fn open_external(url: &str) {
    let operation = HSTRING::from("open");
    let file = HSTRING::from(url);
    unsafe {
        let _ = ShellExecuteW(None, &operation, &file, None, None, SW_SHOWNORMAL);
    }
}

/// Minimal host extraction (scheme + optional userinfo stripped, path/query
/// cut off) - enough for an allowlist check without pulling in a URL crate.
fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
    let end = rest
        .find(['/', '?', '#', ':'])
        .unwrap_or(rest.len());
    let host = rest[..end].to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_extraction() {
        assert_eq!(host_of("https://www.youtube.com/watch?v=x"), Some("youtube.com".into()));
        assert_eq!(host_of("http://accounts.google.com/signin"), Some("accounts.google.com".into()));
        assert_eq!(host_of("https://evil.com:8080/path"), Some("evil.com".into()));
        assert_eq!(host_of("https://user@evil.com/"), Some("evil.com".into()));
        assert_eq!(host_of("not-a-url"), None);
        assert_eq!(host_of("https://"), None);
    }

    #[test]
    fn google_country_domains() {
        assert!(is_google_country_domain("google.ru"));
        assert!(is_google_country_domain("accounts.google.co.uk"));
        assert!(is_google_country_domain("google.com.br"));
        assert!(!is_google_country_domain("google.evil-domain.com"));
        assert!(!is_google_country_domain("notgoogle.ru"));
        assert!(!is_google_country_domain("google.somethinglong"));
    }

    #[test]
    fn allowlist() {
        assert!(is_allowed("https://www.youtube.com/watch?v=x"));
        assert!(is_allowed("https://accounts.google.com/o/oauth2"));
        assert!(is_allowed("https://accounts.google.ru/signin"));
        assert!(is_allowed("https://i.ytimg.com/vi/x/hq.jpg"));
        assert!(is_allowed("chrome-extension://abc/page.html")); // internal
        assert!(is_allowed("about:blank"));
        assert!(!is_allowed("https://example.com/"));
        // Suffix spoofing must not pass.
        assert!(!is_allowed("https://fakeyoutube.com/"));
        assert!(!is_allowed("https://youtube.com.evil.net/"));
    }
}
