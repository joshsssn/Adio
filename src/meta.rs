//! Lecture des tags (titre, artiste, album, durée, pochette) via lofty.

use lofty::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

#[derive(Clone, Default, Debug)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<Duration>,
}

pub fn file_stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Lit les tags. `with_cover` = renvoie aussi les octets de la première image.
pub fn read(path: &Path, with_cover: bool) -> (TrackInfo, Option<Vec<u8>>) {
    let mut info = TrackInfo { title: file_stem(path), ..Default::default() };
    let mut cover = None;
    let opts = lofty::config::ParseOptions::new().read_cover_art(with_cover);
    let Ok(tagged) = lofty::probe::Probe::open(path).and_then(|p| p.options(opts).read()) else {
        return (info, None);
    };
    let d = tagged.properties().duration();
    if !d.is_zero() {
        info.duration = Some(d);
    }
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
            let (info, _) = read(&p, false);
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

pub fn fmt_dur(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
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
