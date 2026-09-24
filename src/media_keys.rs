//! Global media-key fallback for machines where SMTC isn't available (the
//! primary path in `media.rs`). Registers the hardware media keys as global
//! hotkeys on a dedicated thread and relays presses through the event loop.
//!
//! Only started when SMTC init failed: RegisterHotKey *steals* the key from
//! every other app system-wide, whereas SMTC cooperates with them, so the
//! rude variant must stay a fallback.

use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_NOREPEAT};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

use crate::media::MediaButton;

const VK_MEDIA_NEXT_TRACK: u32 = 0xB0;
const VK_MEDIA_PREV_TRACK: u32 = 0xB1;
const VK_MEDIA_PLAY_PAUSE: u32 = 0xB3;

/// Spawns the listener thread. Registration failures are logged and skipped
/// (another app may already own a key); the thread runs for the process
/// lifetime, which is fine for hotkeys registered against a thread queue.
pub fn spawn_fallback(on_button: impl Fn(MediaButton) + Send + 'static) {
    std::thread::spawn(move || {
        // (id, vk, button) - hotkeys with a null hwnd are delivered to this
        // thread's message queue, so registration must happen on this thread.
        let keys = [
            (1, VK_MEDIA_PLAY_PAUSE, MediaButton::PlayPause),
            (2, VK_MEDIA_NEXT_TRACK, MediaButton::Next),
            (3, VK_MEDIA_PREV_TRACK, MediaButton::Previous),
        ];

        let mut registered = 0;
        for (id, vk, _) in keys {
            match unsafe { RegisterHotKey(None, id, MOD_NOREPEAT, vk) } {
                Ok(()) => registered += 1,
                Err(e) => crate::logging::log(format!(
                    "media-key fallback: RegisterHotKey vk=0x{vk:X} failed: {e}"
                )),
            }
        }
        if registered == 0 {
            crate::logging::log("media-key fallback: no keys registered, thread exiting".to_string());
            return;
        }
        crate::logging::log(format!("media-key fallback active ({registered} keys)"));

        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            if msg.message == WM_HOTKEY {
                let button = keys
                    .iter()
                    .find(|(id, _, _)| *id as usize == msg.wParam.0)
                    .map(|(_, _, b)| *b);
                if let Some(b) = button {
                    on_button(b);
                }
            }
        }
    });
}
