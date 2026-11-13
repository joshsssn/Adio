//! Intégration Windows : touches multimédia + overlay média (SMTC) via souvlaki.

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

pub enum MediaCmd {
    Play,
    Pause,
    Toggle,
    Next,
    Prev,
    Stop,
    Seek(Duration),
}

pub struct MediaKeys {
    controls: MediaControls,
    pub rx: Receiver<MediaCmd>,
}

impl MediaKeys {
    pub fn new(hwnd: Option<*mut std::ffi::c_void>, ctx: eframe::egui::Context) -> Option<Self> {
        let config = PlatformConfig { dbus_name: "adio", display_name: "Adio", hwnd };
        let mut controls = MediaControls::new(config).ok()?;
        let (tx, rx) = channel();
        controls
            .attach(move |e| {
                let cmd = match e {
                    MediaControlEvent::Play => MediaCmd::Play,
                    MediaControlEvent::Pause => MediaCmd::Pause,
                    MediaControlEvent::Toggle => MediaCmd::Toggle,
                    MediaControlEvent::Next => MediaCmd::Next,
                    MediaControlEvent::Previous => MediaCmd::Prev,
                    MediaControlEvent::Stop => MediaCmd::Stop,
                    MediaControlEvent::SetPosition(MediaPosition(p)) => MediaCmd::Seek(p),
                    _ => return,
                };
                let _ = tx.send(cmd);
                ctx.request_repaint();
            })
            .ok()?;
        Some(Self { controls, rx })
    }

    // NB : pas de durée/position (timeline) envoyées à Windows : avec une timeline
    // figée, Windows renvoie un « pause » avant la fin de piste.
    pub fn set_metadata(&mut self, title: &str, artist: &str, album: &str, cover_url: Option<&str>) {
        let _ = self.controls.set_metadata(MediaMetadata {
            title: Some(title),
            artist: (!artist.is_empty()).then_some(artist),
            album: (!album.is_empty()).then_some(album),
            cover_url,
            duration: None,
        });
    }

    pub fn set_playing(&mut self, playing: bool) {
        let progress = None;
        let _ = self.controls.set_playback(if playing {
            MediaPlayback::Playing { progress }
        } else {
            MediaPlayback::Paused { progress }
        });
    }
}
