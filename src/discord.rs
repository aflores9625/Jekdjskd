//! Discord Rich Presence: shows "Watching <video>" with the video thumbnail
//! in the user's Discord status.
//!
//! Runs on its own thread fed by an mpsc channel (updates originate on the
//! event-loop thread from page IPC). The Discord connection is lazy and
//! self-healing: if Discord isn't running (or restarts), updates are dropped
//! and reconnection is retried with a cooldown instead of spamming attempts.

use std::sync::mpsc::{channel, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, ActivityType, Assets, Button, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

#[derive(Debug)]
pub enum Update {
    Watching {
        video_id: String,
        title: String,
        channel: String,
        paused: bool,
        /// Playhead position in seconds (for the Discord progress bar).
        position_secs: u64,
        /// Total video length in seconds; 0 = unknown/live.
        duration_secs: u64,
    },
    Clear,
}

const RECONNECT_COOLDOWN: Duration = Duration::from_secs(30);

pub fn spawn(client_id: String) -> Sender<Update> {
    let (tx, rx) = channel::<Update>();

    std::thread::spawn(move || {
        let mut client: Option<DiscordIpcClient> = None;
        let mut last_attempt: Option<Instant> = None;
        let mut logged_unavailable = false;

        while let Ok(update) = rx.recv() {
            // Lazily (re)connect, but not more often than the cooldown - the
            // channel keeps delivering updates while Discord is closed.
            if client.is_none() {
                let due = last_attempt
                    .map(|t| t.elapsed() >= RECONNECT_COOLDOWN)
                    .unwrap_or(true);
                if !due {
                    continue;
                }
                last_attempt = Some(Instant::now());
                match DiscordIpcClient::new(&client_id) {
                    Ok(mut c) => match c.connect() {
                        Ok(()) => {
                            crate::logging::log("discord rpc: connected".to_string());
                            logged_unavailable = false;
                            client = Some(c);
                        }
                        Err(e) => {
                            if !logged_unavailable {
                                crate::logging::log(format!(
                                    "discord rpc: not available (is Discord running?): {e}"
                                ));
                                logged_unavailable = true;
                            }
                            continue;
                        }
                    },
                    Err(e) => {
                        crate::logging::log(format!("discord rpc: client init failed: {e}"));
                        return; // bad client id - no point retrying forever
                    }
                }
            }

            let c = client.as_mut().expect("connected above");
            let result = match &update {
                Update::Watching {
                    video_id,
                    title,
                    channel,
                    paused,
                    position_secs,
                    duration_secs,
                } => {
                    let url = format!("https://www.youtube.com/watch?v={video_id}");
                    let thumb = format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg");
                    // Discord rejects fields over 128 chars; titles can exceed it.
                    let details = truncate(title, 120);
                    let state = if *paused {
                        format!("⏸ На паузе · {}", truncate(channel, 90))
                    } else {
                        truncate(channel, 120)
                    };

                    // ActivityType::Watching renders as "Watching <app>"
                    // instead of "Playing <app>" - this is a video, not a game.
                    let mut activity = Activity::new()
                        .activity_type(ActivityType::Watching)
                        .details(&details)
                        .state(&state)
                        .assets(Assets::new().large_image(&thumb).large_text("YouTube"))
                        .buttons(vec![Button::new("Открыть видео", &url)]);

                    // start = wall clock minus playhead, end = start + length:
                    // Discord derives a real progress bar (elapsed / total)
                    // from the pair instead of a meaningless session timer.
                    // Paused or live (unknown duration) videos get no bar.
                    if !*paused {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        let start = now - *position_secs as i64;
                        let mut ts = Timestamps::new().start(start);
                        if *duration_secs > 0 {
                            ts = ts.end(start + *duration_secs as i64);
                        }
                        activity = activity.timestamps(ts);
                    }
                    c.set_activity(activity)
                }
                Update::Clear => c.clear_activity(),
            };

            if let Err(e) = result {
                // Pipe broke (Discord closed / restarted): drop the client and
                // let the next update trigger a reconnect after the cooldown.
                crate::logging::log(format!("discord rpc: update failed, will reconnect: {e}"));
                client = None;
            }
        }
    });

    tx
}

/// Truncates on a char boundary (Discord limits are in characters, and a
/// mid-UTF-8 byte slice would panic).
fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}
