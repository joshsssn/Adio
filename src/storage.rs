//! Persistance portable : tout est stocké dans `AdioData/` à côté de l'exécutable
//! (repli sur %APPDATA%\Adio si ce dossier n'est pas inscriptible).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    pub fn cycle(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub last_path: Option<PathBuf>,
    /// Thème sombre (None = suivre Windows).
    pub dark: Option<bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { volume: 0.8, shuffle: false, repeat: Repeat::Off, last_path: None, dark: None }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryEntry {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub played_at: i64,
}

const HISTORY_MAX: usize = 1000;

pub struct Storage {
    pub dir: PathBuf,
}

impl Storage {
    pub fn init() -> Self {
        let candidates = [
            std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("AdioData"))),
            std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Adio")),
        ];
        for dir in candidates.into_iter().flatten() {
            if is_writable(&dir) {
                let _ = fs::create_dir_all(dir.join("playlists"));
                return Self { dir };
            }
        }
        Self { dir: std::env::temp_dir().join("Adio") }
    }

    fn read_json<T: for<'de> Deserialize<'de> + Default>(&self, name: &str) -> T {
        fs::read_to_string(self.dir.join(name))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn write_json<T: Serialize>(&self, name: &str, v: &T) {
        if let Ok(s) = serde_json::to_string_pretty(v) {
            let tmp = self.dir.join(format!("{name}.tmp"));
            if fs::write(&tmp, s).is_ok() {
                let _ = fs::rename(tmp, self.dir.join(name));
            }
        }
    }

    pub fn load_settings(&self) -> Settings {
        self.read_json("settings.json")
    }
    pub fn save_settings(&self, s: &Settings) {
        self.write_json("settings.json", s)
    }

    pub fn load_history(&self) -> Vec<HistoryEntry> {
        self.read_json("history.json")
    }
    pub fn save_history(&self, h: &mut Vec<HistoryEntry>) {
        if h.len() > HISTORY_MAX {
            let excess = h.len() - HISTORY_MAX;
            h.drain(..excess);
        }
        self.write_json("history.json", h)
    }

    // ---------- Playlists (.m3u8) ----------

    fn playlists_dir(&self) -> PathBuf {
        self.dir.join("playlists")
    }

    pub fn list_playlists(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.playlists_dir())
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| {
                        let p = e.path();
                        let ext = p.extension()?.to_str()?.to_ascii_lowercase();
                        (ext == "m3u8" || ext == "m3u").then(|| p.file_stem()?.to_str().map(String::from)).flatten()
                    })
                    .collect()
            })
            .unwrap_or_default();
        names.sort_by_key(|n| n.to_lowercase());
        names
    }

    fn playlist_path(&self, name: &str) -> PathBuf {
        let m3u8 = self.playlists_dir().join(format!("{name}.m3u8"));
        let m3u = self.playlists_dir().join(format!("{name}.m3u"));
        if !m3u8.exists() && m3u.exists() { m3u } else { m3u8 }
    }

    pub fn load_playlist(&self, name: &str) -> Vec<PathBuf> {
        let path = self.playlist_path(name);
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        fs::read_to_string(&path)
            .map(|s| {
                s.lines()
                    .map(|l| l.trim().trim_start_matches('\u{feff}'))
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(|l| {
                        let p = PathBuf::from(l);
                        if p.is_absolute() { p } else { base.join(p) }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn save_playlist(&self, name: &str, tracks: &[PathBuf]) {
        let mut s = String::from("#EXTM3U\n");
        for t in tracks {
            s.push_str(&t.to_string_lossy());
            s.push('\n');
        }
        let _ = fs::write(self.playlist_path(name), s);
    }

    pub fn delete_playlist(&self, name: &str) {
        let _ = fs::remove_file(self.playlist_path(name));
    }

    pub fn rename_playlist(&self, old: &str, new: &str) {
        let tracks = self.load_playlist(old);
        self.delete_playlist(old);
        self.save_playlist(new, &tracks);
    }
}

fn is_writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".probe");
    let ok = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(probe);
    ok
}

/// Nettoie un nom de playlist pour qu'il soit un nom de fichier valide.
pub fn sanitize_name(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .collect::<String>()
        .trim()
        .to_string()
}
