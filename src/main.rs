// Hide the console window in release builds; keep it in debug for panic output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ambient;
mod analytics;
mod discord;
mod extension;
mod features;
mod icon;
mod links;
mod logging;
mod media;
mod media_keys;
mod settings;
mod settings_page;
mod single_instance;
mod theme;
mod titlebar;
mod tray;
mod window_state;

use std::rc::Rc;

use tao::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::{Theme, WindowBuilder},
};
use tray_icon::{menu::MenuEvent as TrayMenuEvent, TrayIconEvent};
use wry::{WebViewBuilder, WebViewBuilderExtWindows};

enum UserEvent {
    Tray(TrayIconEvent),
    Menu(TrayMenuEvent),
    MediaButton(media::MediaButton),
    /// Navigate the main webview (popup requests to first-party domains,
    /// e.g. the Google sign-in flow, get folded back into the single window).
    OpenInApp(String),
    /// A command from the in-app settings page: `set:<key>=<0|1>` to change a
    /// pref, `restart` to relaunch, `back` to return to YouTube.
    SettingsCommand(String),
    /// Player entered/left fullscreen; mirror it onto the OS window so the
    /// native title bar disappears too.
    Fullscreen(bool),
}

impl From<TrayIconEvent> for UserEvent {
    fn from(e: TrayIconEvent) -> Self {
        UserEvent::Tray(e)
    }
}
impl From<TrayMenuEvent> for UserEvent {
    fn from(e: TrayMenuEvent) -> Self {
        UserEvent::Menu(e)
    }
}

fn main() -> wry::Result<()> {
    logging::install_panic_hook();
    logging::log(format!("YouTube Glass v{} starting", env!("CARGO_PKG_VERSION")));

    // Second launch just activates the first instance's window and exits; a
    // self-restart waits for the previous instance to release the lock.
    let restarted = std::env::args().any(|a| a == single_instance::RESTART_ARG);
    let Some(_instance_lock) = single_instance::acquire(restarted) else {
        return Ok(());
    };

    // Only after the lock: a second launch must not delete files out of the
    // profile the running instance is using.
    extension::purge_harmful_extensions_from_profile();

    // User prefs (theme/ads/analytics/always-on-top/close-to-tray). Loaded once
    // here; the tray menu seeds its check items from it, and live toggles write
    // back into the same Settings instance over the run.
    let mut settings = settings::load();

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let monitors: Vec<(i32, i32, u32, u32)> = event_loop
        .available_monitors()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            (p.x, p.y, s.width, s.height)
        })
        .collect();
    let saved = window_state::clamp_to_monitors(window_state::load(), &monitors);

    let window = Rc::new(
        WindowBuilder::new()
            .with_title("YouTube")
            // Dark window chrome is part of the Liquid Glass theme; with the
            // theme off, follow the system setting.
            .with_theme(settings.theme.then_some(Theme::Dark))
            .with_window_icon(Some(icon::youtube_icon(128)))
            .with_window_classname(single_instance::WINDOW_CLASS_NAME)
            .with_inner_size(PhysicalSize::new(saved.width, saved.height))
            .with_position(PhysicalPosition::new(saved.x, saved.y))
            .with_min_inner_size(PhysicalSize::new(640u32, 480u32))
            .build(&event_loop)
            .expect("failed to create window"),
    );
    if saved.maximized {
        window.set_maximized(true);
    }

    if settings.always_on_top {
        window.set_always_on_top(true);
    }

    // Tint the native caption to match the theme instead of replacing it with
    // custom-drawn chrome (see titlebar.rs for why).
    if settings.theme {
        titlebar::apply(window.hwnd());
    }

    // `None` if the notification area is unavailable - closing the window
    // then quits normally instead of hiding it with no way back.
    let tray = tray::init::<UserEvent>(proxy.clone(), &settings);

    // System Media Transport Controls: Now Playing widget + hardware media keys.
    // Button presses arrive on a WinRT callback thread, so they're relayed
    // through the event loop proxy rather than touching the webview directly.
    let media_proxy = proxy.clone();
    let media_controls: Option<Rc<media::MediaControls>> = if std::env::var_os("YTG_NO_SMTC")
        .is_some()
    {
        logging::log("SMTC disabled via YTG_NO_SMTC".to_string());
        None
    } else {
        match media::MediaControls::new(window.hwnd(), move |button| {
            let _ = media_proxy.send_event(UserEvent::MediaButton(button));
        }) {
            Ok(m) => Some(Rc::new(m)),
            Err(e) => {
                logging::log(format!(
                    "SMTC unavailable, falling back to global media-key hotkeys: {e}"
                ));
                // Rude global-hotkey fallback (steals the keys system-wide),
                // so it's only wired up when the cooperative SMTC path failed.
                let fallback_proxy = proxy.clone();
                media_keys::spawn_fallback(move |button| {
                    let _ = fallback_proxy.send_event(UserEvent::MediaButton(button));
                });
                None
            }
        }
    };

    // Init scripts inject at WebView creation time and can't be removed in
    // place, so each one is gated by its setting: toggling it off takes effect
    // on the next launch (the "Restart" tray item). features.rs's media-key
    // reporting and hotkeys always run - they're inert without ads/tray, and
    // keeping them unconditional avoids a separate "media only" code path.
    let mut parts: Vec<String> = Vec::new();
    // Version banner in the page's DevTools console - the first thing to
    // check when debugging "which build am I even running".
    parts.push(format!(
        "console.log('%cYouTube Glass v{v}%c  theme={t} ads={a} analytics={an} cinema={c} rpc={r}', \
         'background:#f1f1f1;color:#0d0d0d;padding:2px 8px;border-radius:6px;font-weight:600', '');",
        v = env!("CARGO_PKG_VERSION"),
        t = settings.theme,
        a = settings.block_ads,
        an = settings.analytics,
        c = settings.cinema,
        r = settings.discord_rpc,
    ));
    // WebView2 runs init scripts in every document and frame: Google sign-in,
    // consent.youtube.com, our settings page, live-chat/ad iframes. Scope
    // them (see `youtube_only`) so the dark theme can't paint foreign pages
    // dark-on-dark and page logic can't run twice from iframes.
    if settings.theme {
        // Frames too: the live chat iframe should match the theme.
        parts.push(youtube_only(&theme::injection_script(), true));
        // Ambilight backlight: derives its colors from the live video frames
        // (ambient.rs), so it rides along with the visual theme package.
        parts.push(youtube_only(ambient::injection_script(), false));
    }
    parts.push(youtube_only(
        &features::script(settings.block_ads, settings.cinema, settings.prefer_hd),
        false,
    ));
    parts.push(youtube_only(include_str!("player_extras.js"), false));
    if settings.analytics {
        parts.push(youtube_only(analytics::script(), false));
    }
    let init_script = parts.join("\n");
    logging::log(format!(
        "init scripts registered separately: theme={} analytics={} ({} bytes combined)",
        settings.theme,
        settings.analytics,
        init_script.len()
    ));

    // Discord Rich Presence worker. Dormant without a client id (see the
    // settings file / settings page hint for how to provide one).
    let discord_tx = if settings.discord_rpc && !settings.discord_client_id.is_empty() {
        Some(discord::spawn(settings.discord_client_id.clone()))
    } else {
        if settings.discord_rpc {
            logging::log(
                "discord rpc: enabled but discord_client_id is empty; feature dormant".to_string(),
            );
        }
        None
    };

    let title_window = window.clone();
    let media_for_title = media_controls.clone();
    let media_for_ipc = media_controls.clone();

    let mut webview_builder = WebViewBuilder::new(&window).with_url("https://www.youtube.com");
    for script in &parts {
        webview_builder = webview_builder.with_initialization_script(script);
    }

    let webview = webview_builder
        .with_background_color((13, 13, 13, 255))
        .with_browser_extensions_enabled(true) // required before loading extensions
        // Full GPU acceleration. A black-window episode earlier was traced to
        // the (since removed) uBlock MV2 extension + stale profile state, not
        // the GPU pipeline itself - retested clean and hardware rendering is
        // fine. If a black window ever comes back, set
        //   YTG_BROWSER_ARGS="<flags below> --disable-gpu-compositing"
        // as an env override before blaming anything else. The first flags
        // replicate wry's own defaults, which passing custom args overrides.
        .with_additional_browser_args(&std::env::var("YTG_BROWSER_ARGS").unwrap_or_else(|_| {
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
             --autoplay-policy=no-user-gesture-required"
                .to_string()
        }))
        // In-app settings page served from youtube-glass:// (rendered on
        // Windows as http://youtube-glass.settings/...). Two routes only:
        // the page itself and /state (JSON). Settings are re-read from disk
        // on each /state request rather than shared with the event loop's
        // mutable copy - cheap (tiny file, fetched only while the page is
        // open) and avoids a RefCell race between two threads.
        .with_custom_protocol("youtube-glass".into(), |request| {
            use std::borrow::Cow;
            use wry::http::{Response, StatusCode};

            // Strip leading '/'; the page lives at "/" or "/settings".
            let path = request.uri().path().trim_start_matches('/');
            let json = settings::load().to_json();
            match settings_page::handle(path, &json) {
                Some((status, mime, body)) => {
                    let mut resp = Response::new(Cow::Owned(body));
                    *resp.status_mut() = StatusCode::from_u16(status).unwrap();
                    resp.headers_mut().insert(
                        "content-type",
                        mime.parse().unwrap(),
                    );
                    resp
                }
                None => {
                    let mut resp = Response::new(Cow::Borrowed(&b"not found"[..]));
                    *resp.status_mut() = StatusCode::NOT_FOUND;
                    resp
                }
            }
        })
        .with_navigation_handler(|url| {
            if links::is_allowed(&url) {
                true
            } else {
                links::open_external(&url);
                false
            }
        })
        .with_new_window_req_handler({
            let popup_proxy = proxy.clone();
            move |url| {
                // No in-app tab/window management. First-party popups (the
                // Google sign-in flow opens one) navigate the main window
                // instead; anything else goes to the system browser.
                // Extension-internal pages (chrome-extension://) are dropped.
                if url.starts_with("http://") || url.starts_with("https://") {
                    if links::is_allowed(&url) {
                        let _ = popup_proxy.send_event(UserEvent::OpenInApp(url));
                    } else {
                        links::open_external(&url);
                    }
                }
                false
            }
        })
        .with_ipc_handler({
            let settings_proxy = proxy.clone();
            move |req| {
                // wry stamps the sender page's URL into the request URI. The
                // settings commands below mutate prefs / relaunch the app, so
                // they're only accepted from our own settings page - youtube.com
                // scripts also have window.ipc and must not be able to fake them.
                let from_settings_page = req
                    .uri()
                    .host()
                    .is_some_and(|h| h == settings::SETTINGS_HOST);
                let body = req.into_body();
                // The settings page speaks `set:key=value` / `restart` / `back`;
                // everything else is media state from the player. Forward page
                // commands to the event loop (they need window/settings access
                // the IPC thread can't safely touch).
                if body.starts_with("set:")
                    || body == "restart"
                    || body == "back"
                {
                    if from_settings_page {
                        let _ = settings_proxy.send_event(UserEvent::SettingsCommand(body));
                    } else {
                        logging::log(format!(
                            "ipc: settings command from non-settings origin ignored: {body}"
                        ));
                    }
                    return;
                }
                if let Some(state) = body.strip_prefix("fs:") {
                    let _ = settings_proxy.send_event(UserEvent::Fullscreen(state == "1"));
                    return;
                }
                if let Some(rest) = body.strip_prefix("rpc:") {
                    // "state \x1f id \x1f title \x1f channel \x1f pos \x1f dur"
                    if let Some(tx) = &discord_tx {
                        let mut it = rest.splitn(6, '\u{1f}');
                        let state = it.next().unwrap_or("none");
                        let video_id = it.next().unwrap_or("").to_string();
                        let title = it.next().unwrap_or("").to_string();
                        let channel = it.next().unwrap_or("").to_string();
                        let position_secs = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                        let duration_secs = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                        let update = if state == "none" || video_id.is_empty() {
                            discord::Update::Clear
                        } else {
                            discord::Update::Watching {
                                video_id,
                                title,
                                channel,
                                paused: state == "pause",
                                position_secs,
                                duration_secs,
                            }
                        };
                        let _ = tx.send(update);
                    }
                    return;
                }
                match body.as_str() {
                    "media:play" => {
                        if let Some(media) = &media_for_ipc {
                            media.set_playing(true);
                        }
                    }
                    "media:pause" => {
                        if let Some(media) = &media_for_ipc {
                            media.set_playing(false);
                        }
                    }
                    other => logging::log(format!("unknown ipc message: {other}")),
                }
            }
        })
        .with_document_title_changed_handler({
            // YouTube re-fires title changes constantly (buffering,
            // "(1) ..." notification counters); each SMTC update is a COM
            // round-trip, so skip when the effective title hasn't changed.
            let last_now_playing = std::cell::RefCell::new(String::new());
            move |title| {
                // Mirrors real-browser behavior: YouTube's <title> is already
                // "Video Name - YouTube" on watch pages and "YouTube" elsewhere.
                title_window.set_title(&title);
                if let Some(media) = &media_for_title {
                    // Strip only YouTube's "(1) " unread-notification prefix:
                    // a parenthesized run of digits followed by whitespace.
                    // Titles that merely start with digits ("2024: ...") must
                    // survive intact.
                    let base = title.strip_suffix(" - YouTube").unwrap_or(&title);
                    let now_playing = match base.strip_prefix('(') {
                        Some(rest) => match rest.find(')') {
                            Some(end) if !rest[..end].is_empty()
                                && rest[..end].chars().all(|c| c.is_ascii_digit()) =>
                            {
                                rest[end + 1..].trim_start()
                            }
                            _ => base,
                        },
                        None => base,
                    };
                    let mut last = last_now_playing.borrow_mut();
                    if now_playing != *last {
                        *last = now_playing.to_string();
                        media.set_title(now_playing);
                    }
                }
            }
        })
        .build()?;

    if let Err(e) = extension::remove_harmful_installed(&webview) {
        logging::log(format!("extension cleanup failed: {e}"));
    }

    // Load the bundled browser extensions (SponsorBlock, Return YouTube
    // Dislike) into the WebView2 profile. YTG_ONLY_EXT can restrict to one
    // folder name for debugging.
    if std::env::var_os("YTG_NO_EXT").is_none() {
        let only = std::env::var("YTG_ONLY_EXT").ok();
        for path in extension::bundled_paths() {
            if let Some(only) = &only {
                if path.file_name().map(|n| n.to_string_lossy() != only.as_str()).unwrap_or(true) {
                    continue;
                }
            }
            if let Err(e) = extension::install(&webview, &path) {
                logging::log(format!("Failed to load extension at {}: {e}", path.display()));
            }
        }
    } else {
        logging::log("extensions disabled via YTG_NO_EXT".to_string());
    }

    let mut geometry = saved;
    let event_window = window.clone();
    let has_tray = tray.is_some();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // Hide to tray only when both a tray exists AND the user opted
                // into "close minimizes". Otherwise the X quits for real -
                // otherwise a tray-capable machine could never be exited by
                // the close button, which is worse UX than not having a tray.
                if has_tray && settings.close_to_tray {
                    event_window.set_visible(false);
                } else {
                    window_state::save(geometry);
                    settings::save(&settings);
                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::WindowEvent {
                event: WindowEvent::Resized(_) | WindowEvent::Moved(_),
                ..
            } => {
                // Minimized windows report a (-32000, -32000) position and a
                // zero size; keep the last real geometry instead.
                if event_window.is_minimized() {
                    return;
                }
                geometry.maximized = event_window.is_maximized();
                if !geometry.maximized {
                    if let Ok(pos) = event_window.outer_position() {
                        geometry.x = pos.x;
                        geometry.y = pos.y;
                    }
                    let size = event_window.inner_size();
                    geometry.width = size.width;
                    geometry.height = size.height;
                }
            }

            Event::UserEvent(UserEvent::Tray(TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            })) => {
                event_window.set_visible(true);
                event_window.set_focus();
            }

            Event::UserEvent(UserEvent::Menu(menu_event)) => {
                if let Some(t) = &tray {
                    if menu_event.id == t.show_item_id {
                        event_window.set_visible(true);
                        event_window.set_focus();
                    } else if menu_event.id == t.settings_item_id {
                        event_window.set_visible(true);
                        event_window.set_focus();
                        // Leaving the page never fires fullscreenchange.
                        event_window.set_fullscreen(None);
                        let url = format!("http://{}/settings", settings::SETTINGS_HOST);
                        if let Err(e) = webview.load_url(&url) {
                            logging::log(format!("settings page load failed: {e}"));
                        }
                    } else if menu_event.id == *t.always_on_top_item.id() {
                        // Live toggle: check item already flipped its state on
                        // click; mirror onto the window and persist.
                        let on = t.always_on_top_item.is_checked();
                        event_window.set_always_on_top(on);
                        settings.always_on_top = on;
                        settings::save(&settings);
                    } else if menu_event.id == *t.close_to_tray_item.id() {
                        settings.close_to_tray = t.close_to_tray_item.is_checked();
                        settings::save(&settings);
                    } else if menu_event.id == *t.theme_item.id() {
                        settings.theme = t.theme_item.is_checked();
                        settings::save(&settings);
                    } else if menu_event.id == *t.block_ads_item.id() {
                        settings.block_ads = t.block_ads_item.is_checked();
                        settings::save(&settings);
                    } else if menu_event.id == *t.analytics_item.id() {
                        settings.analytics = t.analytics_item.is_checked();
                        settings::save(&settings);
                    } else if menu_event.id == *t.cinema_item.id() {
                        settings.cinema = t.cinema_item.is_checked();
                        settings::save(&settings);
                    } else if menu_event.id == t.restart_item_id {
                        // Init-script toggles only apply at WebView creation,
                        // so relaunch a fresh copy of ourselves (it waits for
                        // our instance lock to be released - see relaunch()).
                        window_state::save(geometry);
                        settings::save(&settings);
                        relaunch();
                        *control_flow = ControlFlow::Exit;
                    } else if menu_event.id == t.quit_item_id {
                        window_state::save(geometry);
                        settings::save(&settings);
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }

            Event::UserEvent(UserEvent::Fullscreen(on)) => {
                event_window.set_fullscreen(if on {
                    Some(tao::window::Fullscreen::Borderless(None))
                } else {
                    None
                });
            }

            Event::UserEvent(UserEvent::OpenInApp(url)) => {
                if let Err(e) = webview.load_url(&url) {
                    logging::log(format!("load_url failed for {url}: {e}"));
                }
            }

            Event::UserEvent(UserEvent::MediaButton(button)) => {
                let action = match button {
                    media::MediaButton::PlayPause => "play_pause",
                    media::MediaButton::Next => "next",
                    media::MediaButton::Previous => "previous",
                };
                let _ = webview.evaluate_script(&features::media_command_script(action));
            }

            Event::UserEvent(UserEvent::SettingsCommand(cmd)) => {
                if cmd == "back" {
                    // Return to YouTube from the settings page.
                    let _ = webview.load_url("https://www.youtube.com");
                } else if cmd == "restart" {
                    // Relaunch a fresh copy so init-script toggles take effect.
                    window_state::save(geometry);
                    settings::save(&settings);
                    relaunch();
                    *control_flow = ControlFlow::Exit;
                } else if let Some(rest) = cmd.strip_prefix("set:") {
                    // `set:key=value` from the settings page. Live keys
                    // (always_on_top, close_to_tray) take effect immediately
                    // and sync their tray checkbox; the rest are persisted
                    // for the next launch.
                    if let Some((key, val)) = rest.split_once('=') {
                        let on = val == "1" || val.eq_ignore_ascii_case("true");
                        match key {
                            // Keep the tray check items in sync; a stale
                            // checkmark flipped the pref back on next click.
                            "theme" => {
                                settings.theme = on;
                                if let Some(t) = &tray {
                                    t.theme_item.set_checked(on);
                                }
                            }
                            "block_ads" => {
                                settings.block_ads = on;
                                if let Some(t) = &tray {
                                    t.block_ads_item.set_checked(on);
                                }
                            }
                            "analytics" => {
                                settings.analytics = on;
                                if let Some(t) = &tray {
                                    t.analytics_item.set_checked(on);
                                }
                            }
                            "cinema" => {
                                settings.cinema = on;
                                if let Some(t) = &tray {
                                    t.cinema_item.set_checked(on);
                                }
                            }
                            "prefer_hd" => settings.prefer_hd = on,
                            "discord_rpc" => settings.discord_rpc = on,
                            "always_on_top" => {
                                settings.always_on_top = on;
                                event_window.set_always_on_top(on);
                                if let Some(t) = &tray {
                                    t.always_on_top_item.set_checked(on);
                                }
                            }
                            "close_to_tray" => {
                                settings.close_to_tray = on;
                                if let Some(t) = &tray {
                                    t.close_to_tray_item.set_checked(on);
                                }
                            }
                            _ => logging::log(format!("settings: unknown key {key}")),
                        }
                        settings::save(&settings);
                    }
                }
            }

            Event::LoopDestroyed => {
                window_state::save(geometry);
                settings::save(&settings);
            }

            _ => {}
        }
    });
}

/// Spawns a fresh copy of the app for "Restart". The flag makes the child wait
/// for this instance's lock instead of exiting as a duplicate launch.
fn relaunch() {
    match std::env::current_exe() {
        Ok(exe) => {
            if let Err(e) = std::process::Command::new(exe)
                .arg(single_instance::RESTART_ARG)
                .spawn()
            {
                logging::log(format!("restart: failed to spawn new instance: {e}"));
            }
        }
        Err(e) => logging::log(format!("restart: current_exe unavailable: {e}")),
    }
}

/// Wraps a page script so it only runs on the YouTube web app itself
/// (`www.`/`m.`/bare youtube.com) and, unless `frames` is set, only in the
/// top-level document.
fn youtube_only(script: &str, frames: bool) -> String {
    let frame_check = if frames { "" } else { " && window === window.top" };
    format!(
        "if (/^(?:www\\.|m\\.)?youtube\\.com$/.test(location.hostname){frame_check}) {{\n{script}\n}}\n"
    )
}
