//! Moteur audio : un `rodio::Sink` par piste (décodage via symphonia).

use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

pub struct Player {
    stream: Option<OutputStream>,
    sink: Option<Sink>,
    volume: f32,
    /// Durée donnée par le décodeur (peut être absente, on complète avec les tags).
    pub decoder_duration: Option<Duration>,
}

impl Player {
    pub fn new(volume: f32) -> Self {
        let stream = OutputStreamBuilder::open_default_stream().ok().map(|mut s| {
            s.log_on_drop(false);
            s
        });
        Self { stream, sink: None, volume, decoder_duration: None }
    }

    pub fn has_output(&self) -> bool {
        self.stream.is_some()
    }

    pub fn load(&mut self, path: &Path, autoplay: bool) -> Result<(), String> {
        self.stop();
        let stream = self.stream.as_ref().ok_or("Aucune sortie audio disponible")?;
        let file = File::open(path).map_err(|e| e.to_string())?;
        let len = file.metadata().map(|m| m.len()).map_err(|e| e.to_string())?;
        let mut builder = Decoder::builder()
            .with_data(BufReader::with_capacity(256 * 1024, file))
            .with_byte_len(len)
            .with_seekable(true)
            .with_gapless(true);
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            builder = builder.with_hint(&ext.to_ascii_lowercase());
        }
        let decoder = builder.build().map_err(|e| format!("Format non supporté : {e}"))?;
        self.decoder_duration = decoder.total_duration();
        let sink = Sink::connect_new(stream.mixer());
        sink.set_volume(self.volume * self.volume);
        if !autoplay {
            sink.pause();
        }
        sink.append(decoder);
        self.sink = Some(sink);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
        self.decoder_duration = None;
    }

    pub fn is_loaded(&self) -> bool {
        self.sink.is_some()
    }

    pub fn is_playing(&self) -> bool {
        self.sink.as_ref().is_some_and(|s| !s.is_paused() && !s.empty())
    }

    /// Vrai quand la piste chargée est arrivée au bout.
    pub fn is_finished(&self) -> bool {
        self.sink.as_ref().is_some_and(|s| s.empty())
    }

    pub fn play(&self) {
        if let Some(s) = &self.sink {
            s.play();
        }
    }

    pub fn pause(&self) {
        if let Some(s) = &self.sink {
            s.pause();
        }
    }

    pub fn toggle(&self) {
        if let Some(s) = &self.sink {
            if s.is_paused() { s.play() } else { s.pause() }
        }
    }

    pub fn position(&self) -> Duration {
        self.sink.as_ref().map(|s| s.get_pos()).unwrap_or_default()
    }

    pub fn seek(&self, pos: Duration) {
        if let Some(s) = &self.sink {
            let _ = s.try_seek(pos);
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        if let Some(s) = &self.sink {
            // Courbe perceptuelle : le slider linéaire « sonne » plus naturel.
            s.set_volume(self.volume * self.volume);
        }
    }
}
