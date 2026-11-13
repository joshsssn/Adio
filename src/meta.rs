//! Lecture des tags (titre, artiste, album, durée, pochette) via lofty.

use lofty::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

#[derive(Clone, Default)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<Duration>,
    /// Format court pour les badges (MP3, FLAC, ALAC…).
    pub kind: String,
    /// Ligne technique : « FLAC · 44.1 kHz · 16-bit ».
    pub tech: String,
    /// Miniature de pochette (pour les listes).
    pub thumb: Option<std::sync::Arc<eframe::egui::ColorImage>>,
}

/// Réduit une pochette en miniature carrée pour les listes.
pub fn make_thumb(bytes: &[u8], size: u32) -> Option<std::sync::Arc<eframe::egui::ColorImage>> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = img.resize_to_fill(size, size, image::imageops::FilterType::Triangle).to_rgba8();
    let ci = eframe::egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    Some(std::sync::Arc::new(ci))
}

pub fn ext_kind(p: &Path) -> String {
    let e = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_uppercase();
    match e.as_str() {
        "AIF" | "AIFC" => "AIFF".into(),
        "WAVE" => "WAV".into(),
        "OGA" => "OGG".into(),
        "MP4" => "M4A".into(),
        _ => e,
    }
}

pub fn file_stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Lit les tags. `with_cover` = renvoie aussi les octets de la première image.
pub fn read(path: &Path, with_cover: bool) -> (TrackInfo, Option<Vec<u8>>) {
    let mut info = TrackInfo { title: file_stem(path), kind: ext_kind(path), ..Default::default() };
    let mut cover = None;
    let opts = lofty::config::ParseOptions::new().read_cover_art(with_cover);
    let Ok(tagged) = lofty::probe::Probe::open(path).and_then(|p| p.options(opts).read()) else {
        return (info, None);
    };
    let props = tagged.properties();
    let d = props.duration();
    if !d.is_zero() {
        info.duration = Some(d);
    }
    // m4a : ALAC si profondeur de bits connue (sans perte), sinon AAC.
    if info.kind == "M4A" {
        info.kind = if props.bit_depth().is_some() { "ALAC".into() } else { "AAC".into() };
    }
    let mut parts = vec![info.kind.clone()];
    let lossless = matches!(info.kind.as_str(), "FLAC" | "WAV" | "AIFF" | "ALAC");
    if !lossless {
        if let Some(b) = props.audio_bitrate().filter(|b| *b > 0) {
            parts.push(format!("{b} kbps"));
        }
    }
    if let Some(sr) = props.sample_rate() {
        let khz = sr as f32 / 1000.0;
        parts.push(if khz.fract() == 0.0 { format!("{khz:.0} kHz") } else { format!("{khz:.1} kHz") });
    }
    if lossless {
        if let Some(b) = props.bit_depth() {
            parts.push(format!("{b}-bit"));
        }
    }
    info.tech = parts.join("  ·  ");
    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        if let Some(t) = tag.title().filter(|t| !t.trim().is_empty()) {
            info.title = t.into_owned();
        }
        info.artist = tag.artist().map(|s| s.into_owned()).unwrap_or_default();
        info.album = tag.album().map(|s| s.into_owned()).unwrap_or_default();
        if with_cover {
            let pics = tag.pictures();
            let front = pics
                .iter()
                .find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront)
                .or_else(|| pics.first());
            cover = front.map(|p| p.data().to_vec());
        }
    }
    (info, cover)
}

/// Charge les tags d'une liste de pistes en tâche de fond.
pub fn spawn_scan(paths: Vec<PathBuf>, tx: Sender<(PathBuf, TrackInfo)>, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        for (i, p) in paths.into_iter().enumerate() {
            let (mut info, cover) = read(&p, true);
            info.thumb = cover.and_then(|c| make_thumb(&c, 96));
            if tx.send((p, info)).is_err() {
                return;
            }
            if i % 32 == 31 {
                ctx.request_repaint();
            }
        }
        ctx.request_repaint();
    });
}


#[cfg(test)]
mod tests {
    /// Vérifie décodage + seek + tags sur un dossier de sons réels.
    /// `ADIO_TEST_DIR=chemin cargo test formats -- --nocapture`
    #[test]
    fn formats() {
        use rodio::Source;
        let Some(dir) = std::env::var_os("ADIO_TEST_DIR") else { return };
        for p in crate::queue::scan_folder(std::path::Path::new(&dir)) {
            let f = std::fs::File::open(&p).unwrap();
            let len = f.metadata().unwrap().len();
            let ext = p.extension().unwrap().to_str().unwrap().to_ascii_lowercase();
            let mut dec = rodio::Decoder::builder().with_data(std::io::BufReader::new(f)).with_byte_len(len)
                .with_seekable(true).with_hint(&ext).build().unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            dec.try_seek(std::time::Duration::from_secs(3)).unwrap_or_else(|e| panic!("seek {}: {e}", p.display()));
            assert!(dec.next().is_some());
            let (info, cover) = super::read(&p, true);
            println!("{:<16} title={:<12} artist={:<14} dur={:?} cover={}", p.file_name().unwrap().to_string_lossy(),
                info.title, info.artist, info.duration, cover.map(|c| c.len()).unwrap_or(0));
        }
    }
}
