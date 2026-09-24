//! System Media Transport Controls (SMTC): surfaces YouTube's now-playing
//! video in the Windows volume flyout / "Now Playing" widget, and lets
//! hardware & keyboard media keys (Play/Pause/Next/Previous) drive playback.

use windows::core::HSTRING;
use windows::Foundation::TypedEventHandler;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

/// The subset of hardware media-key presses the page-side JS knows how to act on.
#[derive(Clone, Copy, Debug)]
pub enum MediaButton {
    PlayPause,
    Next,
    Previous,
}

pub struct MediaControls {
    smtc: SystemMediaTransportControls,
}

impl MediaControls {
    /// Attaches SMTC to this window and wires button presses to `on_button`.
    /// `on_button` fires on whatever thread WinRT delivers the event on
    /// (not necessarily the UI thread) - callers must marshal back themselves
    /// (e.g. via an `EventLoopProxy`, which is `Send + Sync`).
    pub fn new(
        hwnd_isize: isize,
        on_button: impl Fn(MediaButton) + Send + 'static,
    ) -> windows::core::Result<Self> {
        let hwnd = HWND(hwnd_isize as *mut core::ffi::c_void);
        let interop: ISystemMediaTransportControlsInterop =
            windows::core::factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()?;
        let smtc: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd)? };

        smtc.SetIsEnabled(true)?;
        smtc.SetIsPlayEnabled(true)?;
        smtc.SetIsPauseEnabled(true)?;
        smtc.SetIsNextEnabled(true)?;
        smtc.SetIsPreviousEnabled(true)?;

        let updater = smtc.DisplayUpdater()?;
        updater.SetType(MediaPlaybackType::Video)?;
        updater.Update()?;

        smtc.ButtonPressed(&TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(move |_sender, args| {
            if let Some(args) = args {
                let button = match args.Button()? {
                    SystemMediaTransportControlsButton::Play
                    | SystemMediaTransportControlsButton::Pause => MediaButton::PlayPause,
                    SystemMediaTransportControlsButton::Next => MediaButton::Next,
                    SystemMediaTransportControlsButton::Previous => MediaButton::Previous,
                    _ => return Ok(()),
                };
                on_button(button);
            }
            Ok(())
        }))?;

        Ok(Self { smtc })
    }

    /// Updates the title shown in the Now Playing widget.
    pub fn set_title(&self, title: &str) {
        if let Ok(updater) = self.smtc.DisplayUpdater() {
            if let Ok(video) = updater.VideoProperties() {
                let _ = video.SetTitle(&HSTRING::from(title));
            }
            let _ = updater.Update();
        }
    }

    /// Updates the Playing/Paused state shown alongside the transport controls.
    pub fn set_playing(&self, playing: bool) {
        let status = if playing {
            MediaPlaybackStatus::Playing
        } else {
            MediaPlaybackStatus::Paused
        };
        let _ = self.smtc.SetPlaybackStatus(status);
    }
}
