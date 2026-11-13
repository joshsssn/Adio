//! File de lecture : pistes d'un dossier ou d'une playlist, ordre normal ou aléatoire.

use crate::storage::Repeat;
use rand::seq::SliceRandom;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "wav", "wave", "aif", "aiff", "aifc", "flac", "m4a", "alac", "mp4", "aac", "ogg", "oga", "opus",
];

pub fn is_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

#[derive(Default)]
pub struct Queue {
    pub tracks: Vec<PathBuf>,
    order: Vec<usize>,
    pos: usize,
    shuffle: bool,
    /// Libellé de la source (nom du dossier ou de la playlist).
    pub source: String,
}

impl Queue {
    pub fn from_tracks(tracks: Vec<PathBuf>, start: usize, shuffle: bool, source: String) -> Self {
        let mut q = Self { order: (0..tracks.len()).collect(), tracks, pos: 0, shuffle: false, source };
        q.pos = start.min(q.tracks.len().saturating_sub(1));
        q.set_shuffle(shuffle);
        q
    }

    pub fn from_folder(dir: &Path, shuffle: bool) -> Self {
        let tracks = scan_folder(dir);
        let source = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let start = if shuffle && !tracks.is_empty() { rand::random::<usize>() % tracks.len() } else { 0 };
        Self::from_tracks(tracks, start, shuffle, source)
    }

    pub fn from_file(file: &Path, shuffle: bool) -> Self {
        let dir = file.parent().unwrap_or(Path::new("."));
        let mut tracks = scan_folder(dir);
        let idx = match tracks.iter().position(|t| t == file) {
            Some(i) => i,
            None => {
                tracks.push(file.to_path_buf());
                tracks.len() - 1
            }
        };
        let source = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Self::from_tracks(tracks, idx, shuffle, source)
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn current_index(&self) -> Option<usize> {
        self.order.get(self.pos).copied()
    }

    pub fn current(&self) -> Option<&PathBuf> {
        self.current_index().map(|i| &self.tracks[i])
    }

    pub fn set_shuffle(&mut self, on: bool) {
        let cur = self.current_index();
        self.shuffle = on;
        self.order = (0..self.tracks.len()).collect();
        if on {
            self.order.shuffle(&mut rand::thread_rng());
            // La piste courante passe en tête : le cycle aléatoire part d'elle.
            if let Some(c) = cur {
                if let Some(p) = self.order.iter().position(|&x| x == c) {
                    self.order.swap(0, p);
                }
            }
            self.pos = 0;
        } else {
            self.pos = cur.unwrap_or(0);
        }
    }

    /// Sélectionne directement la piste d'index `i` (dans `tracks`).
    pub fn jump(&mut self, i: usize) -> Option<&PathBuf> {
        self.pos = self.order.iter().position(|&x| x == i)?;
        self.current()
    }

    /// Piste suivante. `auto` = enchaînement automatique en fin de piste.
    pub fn next(&mut self, repeat: Repeat, auto: bool) -> Option<&PathBuf> {
        if self.tracks.is_empty() {
            return None;
        }
        if auto && repeat == Repeat::One {
            return self.current();
        }
        if self.pos + 1 < self.order.len() {
            self.pos += 1;
        } else if repeat != Repeat::Off || !auto {
            // Fin de liste : on reboucle (nouveau tirage si aléatoire).
            if self.shuffle {
                let last = self.current_index();
                self.order.shuffle(&mut rand::thread_rng());
                if self.order.len() > 1 && self.order.first().copied() == last {
                    let n = self.order.len();
                    self.order.swap(0, n - 1);
                }
            }
            self.pos = 0;
        } else {
            return None;
        }
        self.current()
    }

    pub fn prev(&mut self) -> Option<&PathBuf> {
        if self.tracks.is_empty() {
            return None;
        }
        self.pos = if self.pos == 0 { self.order.len() - 1 } else { self.pos - 1 };
        self.current()
    }
}

pub fn scan_folder(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_audio(p)).collect())
        .unwrap_or_default();
    v.sort_by(|a, b| {
        natural_cmp(
            &a.file_name().unwrap_or_default().to_string_lossy().to_lowercase(),
            &b.file_name().unwrap_or_default().to_string_lossy().to_lowercase(),
        )
    });
    v
}

/// Tri « naturel » : « 2 - x » passe avant « 10 - x ».
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = a.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    a.next();
                }
                let mut nb = String::new();
                while let Some(c) = b.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    b.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["10 b", "2 a", "1 c", "track01", "track1"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["1 c", "2 a", "10 b", "track01", "track1"]);
    }

    fn q(n: usize, shuffle: bool) -> Queue {
        Queue::from_tracks((0..n).map(|i| PathBuf::from(format!("{i}.mp3"))).collect(), 0, shuffle, String::new())
    }

    #[test]
    fn repeat_modes() {
        let mut a = q(3, false);
        assert_eq!(a.next(Repeat::Off, true).unwrap(), &PathBuf::from("1.mp3"));
        a.next(Repeat::Off, true);
        assert!(a.next(Repeat::Off, true).is_none());
        assert_eq!(a.next(Repeat::All, true).unwrap(), &PathBuf::from("0.mp3"));
        assert_eq!(a.next(Repeat::One, true).unwrap(), &PathBuf::from("0.mp3"));
        assert_eq!(a.next(Repeat::One, false).unwrap(), &PathBuf::from("1.mp3"));
    }

    #[test]
    fn shuffle_plays_each_once() {
        let mut a = q(20, true);
        let mut seen = vec![a.current_index().unwrap()];
        while let Some(_) = a.next(Repeat::Off, true) {
            seen.push(a.current_index().unwrap());
        }
        seen.sort();
        assert_eq!(seen, (0..20).collect::<Vec<_>>());
    }
}
