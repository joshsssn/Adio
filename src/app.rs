//! Logique de lecture + interface (carte unique : lecteur en haut, liste en bas).

use crate::media_keys::{MediaCmd, MediaKeys};
use crate::meta::{self, TrackInfo};
use crate::player::Player;
use crate::queue::{self, Queue};
use crate::storage::{sanitize_name, HistoryEntry, Repeat, Settings, Storage};
use crate::ui::{self, icon};
use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Stroke, Vec2};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

/// Marge transparente autour de la carte (ombre + poignées de redimensionnement).
const MARGIN: f32 = 12.0;
const RADIUS: u8 = 16;
/// Taille de fenêtre du mode mini.
const MINI_W: f32 = 300.0;
const MINI_H: f32 = 476.0;

/// Contenu de la zone basse de la carte.
#[derive(PartialEq, Clone)]
enum Panel {
    /// Pistes du dossier / de la playlist en cours (le « contexte »).
    Queue,
    /// File d'attente façon Spotify : titres ajoutés à la main, joués en priorité.
    UpNext,
    History,
    Playlists,
    Playlist(String),
}

/// Actions collectées pendant le rendu, appliquées ensuite (évite les conflits d'emprunt).
enum Act {
    PlayIndex(usize),
    OpenFile(PathBuf),
    AddToPlaylist(String, PathBuf),
    NewPlaylistWith(PathBuf),
    PlayPlaylist(String, usize),
    RemoveFromPlaylist(usize),
    Go(Panel),
    PickFolder,
    PickFiles,
    /// Ajoute à la fin de la file d'attente.
    QueueAdd(PathBuf),
    /// Insère en tête de la file d'attente (« Lire ensuite »).
    QueuePlayNext(PathBuf),
    QueueRemove(usize),
    /// Déplace l'élément `from` pour l'insérer avant la position `to` (0..=len).
    QueueMove(usize, usize),
    /// Saute directement à l'élément `i` de la file (les précédents sont retirés).
    QueuePlayAt(usize),
    QueueClear,
    ToggleMini,
}

pub struct App {
    storage: Storage,
    settings: Settings,
    settings_dirty: bool,
    history: Vec<HistoryEntry>,
    player: Player,
    queue: Queue,

    cur_info: TrackInfo,
    cover: Option<egui::TextureHandle>,
    cover_flip: bool,
    meta_cache: HashMap<PathBuf, TrackInfo>,
    thumbs: HashMap<PathBuf, egui::TextureHandle>,
    meta_tx: Sender<(PathBuf, TrackInfo)>,
    meta_rx: Receiver<(PathBuf, TrackInfo)>,

    media: Option<MediaKeys>,
    media_state: Option<bool>,

    panel: Panel,
    error: Option<String>,
    seek_drag: Option<f32>,
    scroll_to_current: bool,
    title_dirty: bool,

    playlists: Vec<(String, usize)>,
    pl_tracks: Vec<PathBuf>,
    new_pl: Option<String>,
    rename_pl: Option<String>,

    /// Titre réellement en lecture (peut venir de la file d'attente).
    now: Option<PathBuf>,
    /// File d'attente utilisateur, prioritaire sur la suite du dossier.
    up_next: Vec<PathBuf>,
    /// Élément de la file en cours de glisser-déposer.
    drag_from: Option<usize>,
    /// Mode mini (pochette + commandes de base) et taille à restaurer.
    mini: bool,
    full_size: Option<Vec2>,

    /// Recherche dans l'onglet courant.
    search_open: bool,
    search: String,
    search_focus: bool,
    /// « Effacer l'historique » attend une confirmation.
    confirm_clear: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, arg: Option<PathBuf>) -> Self {
        ui::setup_fonts(&cc.egui_ctx);
        let storage = Storage::init();
        let settings = storage.load_settings();
        let system_dark = cc.egui_ctx.system_theme() == Some(egui::Theme::Dark);
        ui::set_theme(&cc.egui_ctx, settings.dark.unwrap_or(system_dark));
        let history = storage.load_history();
        let (meta_tx, meta_rx) = channel();
        let media = MediaKeys::new(window_hwnd(cc), cc.egui_ctx.clone());

        let mut app = Self {
            player: Player::new(settings.volume),
            storage,
            settings,
            settings_dirty: false,
            history,
            queue: Queue::default(),
            cur_info: TrackInfo::default(),
            cover: None,
            cover_flip: false,
            meta_cache: HashMap::new(),
            thumbs: HashMap::new(),
            meta_tx,
            meta_rx,
            media,
            media_state: None,
            panel: Panel::Queue,
            error: None,
            seek_drag: None,
            scroll_to_current: true,
            title_dirty: true,
            playlists: Vec::new(),
            pl_tracks: Vec::new(),
            new_pl: None,
            rename_pl: None,
            now: None,
            up_next: Vec::new(),
            drag_from: None,
            mini: false,
            full_size: None,
            search_open: false,
            search: String::new(),
            search_focus: false,
            confirm_clear: false,
        };
        app.refresh_playlists();
        if !app.player.has_output() {
            app.error = Some("Aucune sortie audio détectée".into());
        }

        let ctx = cc.egui_ctx.clone();
        match arg {
            Some(p) => app.open_path(&ctx, &p, true),
            None => {
                // Reprise de la session précédente, en pause.
                if let Some(p) = app.settings.last_path.clone().filter(|p| p.exists()) {
                    app.open_path(&ctx, &p, false);
                }
            }
        }
        app
    }

    // ================================================================ logique

    fn open_path(&mut self, ctx: &egui::Context, p: &Path, autoplay: bool) {
        let shuffle = self.settings.shuffle;
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let q = if p.is_dir() {
            Queue::from_folder(p, shuffle)
        } else if ext == "m3u8" || ext == "m3u" {
            Queue::from_tracks(read_m3u(p), 0, shuffle, meta::file_stem(p))
        } else {
            Queue::from_file(p, shuffle)
        };
        if q.is_empty() {
            self.error = Some(format!("Aucun son dans « {} »", meta::file_stem(p)));
            return;
        }
        self.set_queue(ctx, q, autoplay);
    }

    fn open_files(&mut self, ctx: &egui::Context, files: Vec<PathBuf>) {
        let files: Vec<PathBuf> = files.into_iter().filter(|f| queue::is_audio(f)).collect();
        if !files.is_empty() {
            let q = Queue::from_tracks(files, 0, self.settings.shuffle, "Sélection".into());
            self.set_queue(ctx, q, true);
        }
    }

    fn scan_missing(&self, ctx: &egui::Context, tracks: &[PathBuf]) {
        let missing: Vec<PathBuf> = tracks.iter().filter(|t| !self.meta_cache.contains_key(*t)).cloned().collect();
        if !missing.is_empty() {
            meta::spawn_scan(missing, self.meta_tx.clone(), ctx.clone());
        }
    }

    fn set_queue(&mut self, ctx: &egui::Context, q: Queue, autoplay: bool) {
        self.queue = q;
        self.scan_missing(ctx, &self.queue.tracks);
        self.scroll_to_current = true;
        self.panel = Panel::Queue;
        self.play_current(ctx, autoplay);
    }

    fn play_current(&mut self, ctx: &egui::Context, autoplay: bool) {
        let Some(path) = self.queue.current().cloned() else { return };
        self.play_path(ctx, path, autoplay);
    }

    /// Charge et (éventuellement) lance n'importe quel fichier.
    fn play_path(&mut self, ctx: &egui::Context, path: PathBuf, autoplay: bool) {
        self.now = Some(path.clone());
        let (info, cover_bytes) = meta::read(&path, true);
        self.meta_cache.insert(path.clone(), info.clone());
        self.cur_info = info;
        self.seek_drag = None;
        self.title_dirty = true;
        self.scroll_to_current = true;

        match self.player.load(&path, autoplay) {
            Ok(()) => self.error = None,
            Err(e) => {
                self.error = Some(e);
                self.player.stop();
            }
        }

        // Pochette : texture + miniature de liste + copie fichier pour l'overlay Windows.
        self.cover = None;
        let mut cover_url = None;
        if let Some(bytes) = cover_bytes {
            if let Ok(img) = image::load_from_memory(&bytes) {
                let img = img.resize_to_fill(300, 300, image::imageops::FilterType::Triangle).to_rgba8();
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                self.cover = Some(ctx.load_texture("cover", ci, egui::TextureOptions::LINEAR));
            }
            if !self.thumbs.contains_key(&path) {
                if let Some(t) = meta::make_thumb(&bytes, 96) {
                    self.thumbs.insert(path.clone(), ctx.load_texture("thumb", t, egui::TextureOptions::LINEAR));
                }
            }
            // Deux fichiers en alternance : Windows peut encore tenir l'ancien ouvert.
            self.cover_flip = !self.cover_flip;
            let f = self.storage.dir.join(if self.cover_flip { "cover_a.img" } else { "cover_b.img" });
            if std::fs::write(&f, &bytes).is_ok() {
                cover_url = Some(format!("file://{}", f.display()));
            }
        }

        if let Some(m) = &mut self.media {
            let i = &self.cur_info;
            m.set_metadata(&i.title, &i.artist, &i.album, cover_url.as_deref());
        }
        self.media_state = None;

        if autoplay && self.error.is_none() {
            self.history.push(HistoryEntry {
                path: path.clone(),
                title: self.cur_info.title.clone(),
                artist: self.cur_info.artist.clone(),
                played_at: chrono::Local::now().timestamp(),
            });
            self.storage.save_history(&mut self.history);
        }
        self.settings.last_path = Some(path);
        self.settings_dirty = true;
    }

    /// Vrai si le titre en lecture est celui du contexte (et non un titre de la file).
    fn playing_context(&self) -> bool {
        self.now.is_some() && self.now.as_ref() == self.queue.current()
    }

    fn next(&mut self, ctx: &egui::Context, auto: bool) {
        // Répéter le titre : on rejoue ce qui est en cours, d'où qu'il vienne.
        if auto && self.settings.repeat == Repeat::One {
            if let Some(p) = self.now.clone() {
                self.play_path(ctx, p, true);
                return;
            }
        }
        // La file d'attente passe avant la suite du dossier.
        if !self.up_next.is_empty() {
            let p = self.up_next.remove(0);
            self.play_path(ctx, p, true);
            return;
        }
        // Après un titre de la file, on reprend le contexte au titre suivant.
        if self.queue.next(self.settings.repeat, auto).is_some() {
            self.play_current(ctx, true);
        } else {
            self.player.stop();
            self.media_state = None;
        }
    }

    fn prev(&mut self, ctx: &egui::Context) {
        if self.player.position() > Duration::from_secs(3) {
            self.player.seek(Duration::ZERO);
            self.media_state = None;
        } else if !self.playing_context() && self.queue.current().is_some() {
            // On sortait de la file d'attente : retour au titre du contexte.
            self.play_current(ctx, true);
        } else if self.queue.prev().is_some() {
            self.play_current(ctx, true);
        }
    }

    fn toggle(&mut self, ctx: &egui::Context) {
        if self.player.is_loaded() && !self.player.is_finished() {
            self.player.toggle();
        } else if let Some(p) = self.now.clone() {
            self.play_path(ctx, p, true);
        } else if !self.queue.is_empty() {
            self.play_current(ctx, true);
        }
    }

    fn seek_rel(&mut self, delta: f32) {
        let dur = self.duration().as_secs_f32();
        let p = (self.player.position().as_secs_f32() + delta).clamp(0.0, dur.max(0.0));
        self.player.seek(Duration::from_secs_f32(p));
        self.media_state = None;
    }

    fn set_volume(&mut self, v: f32) {
        self.player.set_volume(v);
        self.settings.volume = self.player.volume();
        self.settings_dirty = true;
    }

    fn duration(&self) -> Duration {
        self.cur_info.duration.or(self.player.decoder_duration).unwrap_or_default()
    }

    fn toggle_shuffle(&mut self) {
        self.settings.shuffle = !self.settings.shuffle;
        self.queue.set_shuffle(self.settings.shuffle);
        self.settings_dirty = true;
    }

    fn cycle_repeat(&mut self) {
        self.settings.repeat = self.settings.repeat.cycle();
        self.settings_dirty = true;
    }

    fn toggle_mute(&mut self) {
        let v = if self.player.volume() > 0.0 { 0.0 } else { 0.8 };
        self.set_volume(v);
    }

    fn pick_folder(&mut self, ctx: &egui::Context) {
        if let Some(d) = rfd::FileDialog::new().set_title("Ouvrir un dossier").pick_folder() {
            self.open_path(ctx, &d, true);
        }
    }

    fn pick_files(&mut self, ctx: &egui::Context) {
        if let Some(f) = rfd::FileDialog::new()
            .set_title("Ouvrir des sons")
            .add_filter("Audio", queue::AUDIO_EXTS)
            .add_filter("Playlist", &["m3u8", "m3u"])
            .pick_files()
        {
            if f.len() == 1 { self.open_path(ctx, &f[0], true) } else { self.open_files(ctx, f) }
        }
    }

    fn refresh_playlists(&mut self) {
        self.playlists = self
            .storage
            .list_playlists()
            .into_iter()
            .map(|n| {
                let c = self.storage.load_playlist(&n).len();
                (n, c)
            })
            .collect();
    }

    fn go(&mut self, ctx: &egui::Context, p: Panel) {
        match &p {
            Panel::Playlist(name) => {
                self.pl_tracks = self.storage.load_playlist(name);
                self.scan_missing(ctx, &self.pl_tracks);
            }
            Panel::Queue => self.scroll_to_current = true,
            Panel::UpNext => {
                let mut v = self.up_next.clone();
                v.extend(self.queue.upcoming(40).into_iter().map(|i| self.queue.tracks[i].clone()));
                self.scan_missing(ctx, &v);
            }
            Panel::Playlists => self.refresh_playlists(),
            Panel::History => {
                let recent: Vec<PathBuf> =
                    self.history.iter().rev().take(300).map(|h| h.path.clone()).filter(|p| p.exists()).collect();
                self.scan_missing(ctx, &recent);
            }
        }
        self.new_pl = None;
        self.rename_pl = None;
        self.confirm_clear = false;
        self.panel = p;
    }

    fn apply(&mut self, ctx: &egui::Context, act: Act) {
        match act {
            Act::PlayIndex(i) => {
                if self.queue.jump(i).is_some() {
                    self.play_current(ctx, true);
                }
            }
            Act::OpenFile(p) => {
                if p.exists() {
                    self.open_path(ctx, &p, true);
                } else {
                    self.error = Some(format!("Fichier introuvable : {}", meta::file_stem(&p)));
                }
            }
            Act::AddToPlaylist(name, p) => {
                let mut t = self.storage.load_playlist(&name);
                t.push(p);
                self.storage.save_playlist(&name, &t);
                if self.panel == Panel::Playlist(name) {
                    self.pl_tracks = t;
                }
                self.refresh_playlists();
            }
            Act::NewPlaylistWith(p) => {
                let mut n = 1;
                let name = loop {
                    let c = format!("Playlist {n}");
                    if !self.playlists.iter().any(|(x, _)| *x == c) {
                        break c;
                    }
                    n += 1;
                };
                self.storage.save_playlist(&name, &[p]);
                self.refresh_playlists();
            }
            Act::PlayPlaylist(name, start) => {
                let tracks = self.storage.load_playlist(&name);
                if !tracks.is_empty() {
                    let q = Queue::from_tracks(tracks, start, self.settings.shuffle, name);
                    self.set_queue(ctx, q, true);
                }
            }
            Act::RemoveFromPlaylist(i) => {
                if let Panel::Playlist(name) = &self.panel {
                    if i < self.pl_tracks.len() {
                        self.pl_tracks.remove(i);
                        self.storage.save_playlist(name, &self.pl_tracks);
                    }
                }
            }
            Act::Go(p) => self.go(ctx, p),
            Act::PickFolder => self.pick_folder(ctx),
            Act::PickFiles => self.pick_files(ctx),
            Act::QueueAdd(p) => {
                self.scan_missing(ctx, std::slice::from_ref(&p));
                self.up_next.push(p);
            }
            Act::QueuePlayNext(p) => {
                self.scan_missing(ctx, std::slice::from_ref(&p));
                self.up_next.insert(0, p);
            }
            Act::QueueRemove(i) => {
                if i < self.up_next.len() {
                    self.up_next.remove(i);
                }
            }
            Act::QueueMove(from, to) => {
                if from < self.up_next.len() && to <= self.up_next.len() && to != from && to != from + 1 {
                    let item = self.up_next.remove(from);
                    let to = if to > from { to - 1 } else { to };
                    self.up_next.insert(to, item);
                }
            }
            Act::QueuePlayAt(i) => {
                if i < self.up_next.len() {
                    let mut rest = self.up_next.split_off(i);
                    let p = rest.remove(0);
                    self.up_next = rest;
                    self.play_path(ctx, p, true);
                }
            }
            Act::QueueClear => self.up_next.clear(),
            Act::ToggleMini => self.toggle_mini(ctx),
        }
    }

    fn open_search(&mut self) {
        self.search_open = true;
        self.search_focus = true;
    }

    fn close_search(&mut self) {
        self.search_open = false;
        self.search.clear();
    }

    /// Requête normalisée, si une recherche est active.
    fn query(&self) -> Option<String> {
        let q = self.search.trim();
        (self.search_open && !q.is_empty()).then(|| ui::fold(q))
    }

    /// Tous les mots de la requête apparaissent dans le titre, l'artiste, l'album ou le nom du fichier.
    fn text_matches(hay: &str, q: &str) -> bool {
        let hay = ui::fold(hay);
        q.split_whitespace().all(|w| hay.contains(w))
    }

    fn track_matches(&self, p: &Path, q: &str) -> bool {
        let stem = meta::file_stem(p);
        let hay = match self.meta_cache.get(p) {
            Some(i) => format!("{} {} {} {}", i.title, i.artist, i.album, stem),
            None => stem,
        };
        Self::text_matches(&hay, q)
    }

    fn no_result(ui: &mut egui::Ui, q: &str) {
        Self::empty_state(ui, &format!("Aucun résultat pour « {q} »"), None);
    }

    fn toggle_mini(&mut self, ctx: &egui::Context) {
        self.mini = !self.mini;
        if self.mini {
            self.full_size = ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(MINI_W, MINI_H)));
        } else {
            let size = self.full_size.unwrap_or(Vec2::new(400.0, 700.0));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
    }

    // ================================================================ entrées

    fn handle_input(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if dropped.len() == 1 {
            self.open_path(ctx, &dropped[0], true);
        } else if dropped.len() > 1 {
            self.open_files(ctx, dropped);
        }

        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        use egui::Modifiers as M;
        let table = [
            (M::NONE, Key::Space),
            (M::CTRL, Key::ArrowRight),
            (M::CTRL, Key::ArrowLeft),
            (M::NONE, Key::ArrowRight),
            (M::NONE, Key::ArrowLeft),
            (M::NONE, Key::ArrowUp),
            (M::NONE, Key::ArrowDown),
            (M::NONE, Key::S),
            (M::NONE, Key::R),
            (M::NONE, Key::M),
            (M::CTRL, Key::O),
            (M::NONE, Key::Escape),
            (M::NONE, Key::Q),
            (M::NONE, Key::H),
            (M::NONE, Key::P),
            (M::NONE, Key::D),
            (M::NONE, Key::N),
            (M::CTRL, Key::F),
        ];
        let mut keys: Vec<usize> = Vec::new();
        ctx.input_mut(|i| {
            for (k, (m, key)) in table.into_iter().enumerate() {
                if i.consume_key(m, key) {
                    keys.push(k);
                }
            }
        });
        for k in keys {
            match k {
                0 => self.toggle(ctx),
                1 => self.next(ctx, false),
                2 => self.prev(ctx),
                3 => self.seek_rel(5.0),
                4 => self.seek_rel(-5.0),
                5 => self.set_volume(self.player.volume() + 0.05),
                6 => self.set_volume(self.player.volume() - 0.05),
                7 => self.toggle_shuffle(),
                8 => self.cycle_repeat(),
                9 => self.toggle_mute(),
                10 => self.pick_folder(ctx),
                11 if self.search_open => self.close_search(),
                11 => {
                    let back = if matches!(self.panel, Panel::Playlist(_)) { Panel::Playlists } else { Panel::Queue };
                    self.go(ctx, back);
                }
                12 => self.go(ctx, Panel::UpNext),
                13 => self.go(ctx, Panel::History),
                14 => self.go(ctx, Panel::Playlists),
                15 => self.go(ctx, Panel::Queue),
                16 => self.toggle_mini(ctx),
                17 => self.open_search(),
                _ => {}
            }
        }
    }

    fn handle_media(&mut self, ctx: &egui::Context) {
        let cmds: Vec<MediaCmd> = match &self.media {
            Some(m) => m.rx.try_iter().collect(),
            None => return,
        };
        for c in cmds {
            match c {
                MediaCmd::Play => {
                    if self.player.is_loaded() && !self.player.is_finished() { self.player.play() } else { self.toggle(ctx) }
                }
                MediaCmd::Pause | MediaCmd::Stop => self.player.pause(),
                MediaCmd::Toggle => self.toggle(ctx),
                MediaCmd::Next => self.next(ctx, false),
                MediaCmd::Prev => self.prev(ctx),
                MediaCmd::Seek(p) => {
                    self.player.seek(p);
                    self.media_state = None;
                }
            }
        }
        let playing = self.player.is_playing();
        if self.media_state != Some(playing) {
            self.media_state = Some(playing);
            if let Some(m) = &mut self.media {
                m.set_playing(playing);
            }
        }
    }

    // ================================================================ fenêtre (sans bordure)

    /// Poignées de redimensionnement dans la marge transparente autour de la carte.
    fn window_resize(ctx: &egui::Context, card: Rect) {
        use egui::viewport::ResizeDirection as D;
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else { return };
        if card.shrink(4.0).contains(pos) {
            return;
        }
        let (l, r) = (pos.x < card.left() + 4.0, pos.x > card.right() - 4.0);
        let (t, b) = (pos.y < card.top() + 4.0, pos.y > card.bottom() - 4.0);
        let dir = match (l, r, t, b) {
            (true, _, true, _) => D::NorthWest,
            (_, true, true, _) => D::NorthEast,
            (true, _, _, true) => D::SouthWest,
            (_, true, _, true) => D::SouthEast,
            (true, ..) => D::West,
            (_, true, ..) => D::East,
            (_, _, true, _) => D::North,
            (_, _, _, true) => D::South,
            _ => return,
        };
        let cursor = match dir {
            D::NorthWest | D::SouthEast => egui::CursorIcon::ResizeNwSe,
            D::NorthEast | D::SouthWest => egui::CursorIcon::ResizeNeSw,
            D::West | D::East => egui::CursorIcon::ResizeHorizontal,
            D::North | D::South => egui::CursorIcon::ResizeVertical,
        };
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }

    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        ui::set_theme(ctx, dark);
        self.settings.dark = Some(dark);
        self.settings_dirty = true;
    }

    /// Chemin court du dossier en cours, pour la barre de titre.
    fn location(&self) -> String {
        let Some(dir) = self.queue.current().and_then(|p| p.parent()) else { return "Adio".into() };
        // Playlist ou sélection : on affiche son nom plutôt qu'un chemin.
        if dir.file_name().map(|n| n.to_string_lossy() != self.queue.source).unwrap_or(true) {
            return self.queue.source.clone();
        }
        let parts: Vec<String> = dir
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let tail = parts.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join("/");
        if parts.len() > 2 { format!("…/{tail}") } else { format!("/{tail}") }
    }

    // ================================================================ carte

    fn ui_card(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let ctx = ui.ctx().clone();
        let p = ui::pal();
        let card = ui.max_rect().shrink(MARGIN);

        // Fond + ombre douce
        let shadow = egui::epaint::Shadow { offset: [0, 8], blur: 22, spread: 0, color: p.shadow };
        ui.painter().add(shadow.as_shape(card, RADIUS));
        ui.painter().rect(card, RADIUS, p.card, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);

        // Déplacement en tirant la carte (les widgets dessinés après restent prioritaires)
        let drag = ui.interact(card, ui.id().with("drag"), egui::Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        Self::window_resize(&ctx, card);

        // ---- barre de titre : emplacement centré · pastilles à droite
        let bar_h = 36.0;
        let ty = card.top() + bar_h / 2.0;
        let dots_zone = Rect::from_min_max(Pos2::new(card.right() - 80.0, card.top()), Pos2::new(card.right(), card.top() + bar_h));
        let show = ui.rect_contains_pointer(dots_zone);
        if ui::window_dot(ui, "close", Pos2::new(card.right() - 20.0, ty), ui::DOT_CLOSE, icon::X, show).on_hover_text("Fermer").clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ui::window_dot(ui, "min", Pos2::new(card.right() - 38.0, ty), ui::DOT_MIN, icon::MINUS, show).on_hover_text("Réduire").clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        let (mini_glyph, mini_tip) =
            if self.mini { (icon::ARROWS_OUT_SIMPLE, "Mode complet (N)") } else { (icon::ARROWS_IN_SIMPLE, "Mode mini (N)") };
        if ui::window_dot(ui, "mini", Pos2::new(card.right() - 56.0, ty), ui::DOT_MINI, mini_glyph, show).on_hover_text(mini_tip).clicked() {
            acts.push(Act::ToggleMini);
        }
        let loc = self.location();
        let loc_max = card.width() - 190.0;
        let lw = ui::text_width(ui, &loc, ui::regular(11.5)).min(loc_max);
        let loc_rect = Rect::from_center_size(Pos2::new(card.center().x, ty), Vec2::new(lw + 16.0, 22.0));
        let loc_r = ui.interact(loc_rect, ui.id().with("loc"), egui::Sense::click());
        let loc_col = if loc_r.hovered() { p.soft } else { p.dim };
        ui::text(ui, loc_rect.center(), Align2::CENTER_CENTER, &loc, ui::regular(11.5), loc_col, loc_max);
        let loc_r = loc_r.on_hover_text("Ouvrir un dossier (Ctrl+O) — clic droit : fichiers").on_hover_cursor(egui::CursorIcon::PointingHand);
        if loc_r.clicked() {
            acts.push(Act::PickFolder);
        }
        loc_r.context_menu(|ui| {
            if ui::menu_item(ui, icon::FOLDER_OPEN, "Ouvrir un dossier…").clicked() {
                acts.push(Act::PickFolder);
                ui.close_menu();
            }
            if ui::menu_item(ui, icon::FILE_AUDIO, "Ouvrir des fichiers…").clicked() {
                acts.push(Act::PickFiles);
                ui.close_menu();
            }
        });
        ui.painter().hline(card.x_range(), card.top() + bar_h, Stroke::new(1.0, p.line));

        if self.mini {
            self.ui_mini(ui, card, bar_h, acts);
            return;
        }

        let left = card.left() + 24.0;
        let right = card.right() - 24.0;
        let w = right - left;
        let has_track = self.queue.current().is_some();

        // ---- en-tête : pochette · titre / artiste / format
        let cov = Rect::from_min_size(Pos2::new(left, card.top() + bar_h + 22.0), Vec2::splat(64.0));
        if self.cover.is_some() {
            let s = egui::epaint::Shadow { offset: [0, 3], blur: 10, spread: 0, color: p.shadow };
            ui.painter().add(s.as_shape(cov, 9));
        }
        ui::thumb(ui, cov, 9, self.cover.as_ref(), icon::MUSIC_NOTE);
        let cov_r = ui.interact(cov, ui.id().with("cover"), egui::Sense::click());
        if cov_r.hovered() {
            ui.painter().rect_filled(cov, 9, Color32::from_black_alpha(70));
            ui::paint_icon(ui, cov.center(), icon::ARROWS_IN_SIMPLE, ui::ph(20.0), Color32::WHITE);
        }
        if cov_r.on_hover_text("Mode mini").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            acts.push(Act::ToggleMini);
        }

        let tx = cov.right() + 16.0;
        let tmax = right - 30.0 - tx;
        let (title, sub) = if has_track {
            let a = if self.cur_info.artist.is_empty() { self.cur_info.album.as_str() } else { self.cur_info.artist.as_str() };
            (self.cur_info.title.as_str(), a)
        } else {
            ("Aucun son", "Glisse un dossier ou un fichier ici")
        };
        ui::text(ui, Pos2::new(tx, cov.top() + 5.0), Align2::LEFT_TOP, title, ui::semibold(17.0), p.text, tmax);
        ui::text(ui, Pos2::new(tx, cov.top() + 28.0), Align2::LEFT_TOP, sub, ui::regular(13.0), p.dim, tmax);
        if let Some(e) = &self.error {
            ui::text(ui, Pos2::new(tx, cov.top() + 47.0), Align2::LEFT_TOP, e, ui::regular(11.0), Color32::from_rgb(0xC2, 0x5B, 0x4E), tmax);
        } else if has_track {
            ui::text(ui, Pos2::new(tx, cov.top() + 47.0), Align2::LEFT_TOP, &self.cur_info.tech, ui::regular(11.0), p.faint, tmax);
        }
        let add_r = ui::icon_button(ui, "addpl", Pos2::new(right - 8.0, cov.top() + 12.0), 28.0, icon::LIST_PLUS, ui::ph(16.0), p.dim, has_track)
            .on_hover_text("Ajouter à une playlist");
        self.add_to_playlist_popup(ui, &add_r, acts);

        // ---- progression
        let py = cov.bottom() + 24.0;
        let dur = self.duration().as_secs_f32();
        let pos = self.seek_drag.unwrap_or_else(|| self.player.position().as_secs_f32()).min(dur);
        let frac = if dur > 0.0 { pos / dur } else { 0.0 };
        let seek_rect = Rect::from_min_size(Pos2::new(left, py - 8.0), Vec2::new(w, 16.0));
        if let Some(f) = ui::bar(ui, "seek", seek_rect, frac, dur > 0.0 && self.player.is_loaded(), 5.0) {
            self.seek_drag = Some(f * dur);
        } else if let Some(t) = self.seek_drag.take() {
            self.player.seek(Duration::from_secs_f32(t));
            self.media_state = None;
        }
        let tf = ui::regular(11.0);
        ui::text(ui, Pos2::new(left, py + 10.0), Align2::LEFT_TOP, &clock(pos), tf.clone(), p.dim, 80.0);
        ui::text(ui, Pos2::new(right, py + 10.0), Align2::RIGHT_TOP, &clock(dur), tf, p.dim, 80.0);

        // ---- commandes · volume (une seule rangée, un seul axe)
        let cy = (py + 54.0).round();
        let step = 46.0;
        let x0 = left + 9.0;
        let (on, off) = (p.text, p.dim);
        let sh = self.settings.shuffle;
        if ui::icon_button(ui, "shuffle", Pos2::new(x0, cy), 32.0, icon::SHUFFLE, ui::ph(18.0), if sh { on } else { off }, true)
            .on_hover_text("Aléatoire (S)")
            .clicked()
        {
            self.toggle_shuffle();
        }
        if self.settings.shuffle {
            ui.painter().circle_filled(Pos2::new(x0, cy + 15.0), 2.0, p.accent);
        }
        if ui::icon_button(ui, "prev", Pos2::new(x0 + step, cy), 34.0, icon::SKIP_BACK, ui::ph(20.0), p.text, has_track)
            .on_hover_text("Précédent (Ctrl+←)")
            .clicked()
        {
            self.prev(&ctx);
        }
        let play_x = x0 + step * 2.0 + 4.0;
        if ui::play_button(ui, Pos2::new(play_x, cy), 19.0, self.player.is_playing(), has_track)
            .on_hover_text("Lecture / pause (Espace)")
            .clicked()
        {
            self.toggle(&ctx);
        }
        let next_x = play_x + step + 4.0;
        if ui::icon_button(ui, "next", Pos2::new(next_x, cy), 34.0, icon::SKIP_FORWARD, ui::ph(20.0), p.text, has_track)
            .on_hover_text("Suivant (Ctrl+→)")
            .clicked()
        {
            self.next(&ctx, false);
        }
        let rep = self.settings.repeat;
        let rep_x = next_x + step;
        let rep_tip = match rep {
            Repeat::Off => "Répéter : non (R)",
            Repeat::All => "Répéter : tout (R)",
            Repeat::One => "Répéter : ce titre (R)",
        };
        let rep_col = if rep != Repeat::Off { on } else { off };
        if ui::icon_button(ui, "repeat", Pos2::new(rep_x, cy), 32.0, icon::REPEAT, ui::ph(18.0), rep_col, true).on_hover_text(rep_tip).clicked() {
            self.cycle_repeat();
        }
        match rep {
            Repeat::All => {
                ui.painter().circle_filled(Pos2::new(rep_x, cy + 15.0), 2.0, p.accent);
            }
            Repeat::One => {
                let b = Pos2::new(rep_x + 9.0, cy - 9.0);
                ui.painter().circle_filled(b, 6.0, p.accent);
                ui::text(ui, b + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, "1", ui::semibold(8.5), Color32::WHITE, 10.0);
            }
            Repeat::Off => {}
        }
        // Volume : curseur + haut-parleur (molette acceptée)
        let vol = self.player.volume();
        let spk = Pos2::new(right - 9.0, cy);
        let vol_icon = if vol == 0.0 { icon::SPEAKER_X } else if vol < 0.5 { icon::SPEAKER_LOW } else { icon::SPEAKER_HIGH };
        let spk_r = ui::icon_button(ui, "mute", spk, 30.0, vol_icon, ui::ph(18.0), p.soft, true).on_hover_text("Muet (M)");
        if spk_r.clicked() {
            self.toggle_mute();
        }
        let vx0 = rep_x + 30.0;
        let vx1 = right - 28.0;
        if vx1 - vx0 >= 40.0 {
            let vw = (vx1 - vx0).min(110.0);
            let vr = Rect::from_min_size(Pos2::new(vx1 - vw, cy - 8.0), Vec2::new(vw, 16.0));
            if let Some(f) = ui::bar(ui, "vol", vr, vol, true, 5.0) {
                self.set_volume(f);
            }
            if ui.rect_contains_pointer(vr.union(spk_r.rect)) {
                let d = ctx.input(|i| i.smooth_scroll_delta.y);
                if d != 0.0 {
                    self.set_volume(vol + d / 800.0);
                }
            }
        }

        // ---- onglets + liste + pied
        let tabs_top = cy + 30.0;
        let footer_h = 34.0;
        let list_bottom = card.bottom() - footer_h;
        if list_bottom - tabs_top < 90.0 {
            return; // fenêtre réduite : lecteur seul
        }
        let tabs_rect = Rect::from_min_max(Pos2::new(left, tabs_top), Pos2::new(right, tabs_top + 32.0));
        let sel = match self.panel {
            Panel::Queue => 0,
            Panel::UpNext => 1,
            Panel::History => 2,
            Panel::Playlists | Panel::Playlist(_) => 3,
        };
        let ctx_label = if self.is_folder_context() { "Dossier" } else { "Liste" };
        let n_up = self.up_next.len();
        let up_label = if n_up > 0 { format!("File d'attente · {n_up}") } else { "File d'attente".into() };
        let (tab_click, tabs_end) = ui::tabs(ui, tabs_rect, &[ctx_label, &up_label, "Historique", "Playlists"], sel);
        if let Some(i) = tab_click {
            acts.push(Act::Go([Panel::Queue, Panel::UpNext, Panel::History, Panel::Playlists][i].clone()));
        }
        // Loupe à droite des onglets : recherche dans l'onglet courant
        let sc = Pos2::new(right - 10.0, tabs_rect.center().y - 1.0);
        if tabs_end + 6.0 < sc.x - 12.0 {
            let col = if self.search_open { p.text } else { p.dim };
            let r = ui::icon_button(ui, "search_btn", sc, 26.0, icon::MAGNIFYING_GLASS, ui::ph(16.0), col, true)
                .on_hover_text("Rechercher dans cet onglet (Ctrl+F)");
            if r.clicked() {
                if self.search_open { self.close_search() } else { self.open_search() }
            }
        }

        // Pied de carte : infos · bascule jour / nuit
        ui.painter().hline(card.x_range(), list_bottom, Stroke::new(1.0, p.line));
        let fy = list_bottom + footer_h / 2.0;
        ui::text(ui, Pos2::new(left, fy), Align2::LEFT_CENTER, &self.footer_info(), ui::regular(11.0), p.dim, w * 0.6);
        let (tg, tl) = if ui::is_dark() { (icon::SUN, "Mode jour") } else { (icon::MOON, "Mode nuit") };
        if ui::link(ui, "theme", Pos2::new(right, fy), tl, Some(tg)).clicked() {
            let d = !ui::is_dark();
            self.set_dark(&ctx, d);
        }

        // Barre de recherche (sous les onglets)
        let mut list_top = tabs_rect.bottom() + 4.0;
        if self.search_open {
            let bar = Rect::from_min_max(Pos2::new(left, list_top + 6.0), Pos2::new(right, list_top + 40.0));
            let hint = match self.panel {
                Panel::Queue => "Rechercher dans le dossier",
                Panel::UpNext => "Rechercher dans la file d'attente",
                Panel::History => "Rechercher dans l'historique",
                Panel::Playlists => "Rechercher une playlist",
                Panel::Playlist(_) => "Rechercher dans la playlist",
            };
            let focus = std::mem::take(&mut self.search_focus);
            let (r, cleared) = ui::search_bar(ui, bar, &mut self.search, hint, focus);
            if cleared {
                self.search.clear();
                self.search_focus = true;
            }
            if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                self.close_search();
            } else if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                self.close_search();
            }
            list_top = bar.bottom() + 4.0;
        }

        // Liste (la barre de défilement déborde dans la marge droite)
        let area = Rect::from_min_max(Pos2::new(left, list_top), Pos2::new(right + ui::GUTTER, list_bottom - 2.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
        child.set_clip_rect(area.expand2(Vec2::new(8.0, 0.0)));
        match self.panel.clone() {
            Panel::Queue => self.ui_queue(&mut child, acts),
            Panel::UpNext => self.ui_upnext(&mut child, acts),
            Panel::History => self.ui_history(&mut child, acts),
            Panel::Playlists => self.ui_playlists(&mut child, acts),
            Panel::Playlist(name) => self.ui_playlist(&mut child, name, acts),
        }
    }

    /// Vrai si le contexte est un dossier (et non une playlist ou une sélection).
    fn is_folder_context(&self) -> bool {
        self.queue
            .current()
            .and_then(|p| p.parent())
            .and_then(|d| d.file_name())
            .is_some_and(|n| n.to_string_lossy() == self.queue.source)
    }

    // ================================================================ mode mini

    fn ui_mini(&mut self, ui: &mut egui::Ui, card: Rect, bar_h: f32, acts: &mut Vec<Act>) {
        let ctx = ui.ctx().clone();
        let p = ui::pal();
        let left = card.left() + 18.0;
        let right = card.right() - 18.0;
        let cx = (left + right) / 2.0;
        let has_track = self.queue.current().is_some() || self.now.is_some();

        // Pochette pleine largeur : un clic ramène au mode complet
        let size = (right - left).min(card.bottom() - card.top() - bar_h - 170.0).max(80.0);
        let art = Rect::from_min_size(Pos2::new(cx - size / 2.0, card.top() + bar_h + 16.0), Vec2::splat(size));
        if self.cover.is_some() {
            let s = egui::epaint::Shadow { offset: [0, 6], blur: 18, spread: 0, color: p.shadow };
            ui.painter().add(s.as_shape(art, 12));
        }
        ui::thumb(ui, art, 12, self.cover.as_ref(), icon::MUSIC_NOTE);
        let ar = ui.interact(art, ui.id().with("mini_art"), egui::Sense::click());
        if ar.hovered() {
            let c = Pos2::new(art.right() - 20.0, art.top() + 20.0);
            ui.painter().circle_filled(c, 14.0, Color32::from_black_alpha(110));
            ui::paint_icon(ui, c, icon::ARROWS_OUT_SIMPLE, ui::ph(14.0), Color32::WHITE);
        }
        if ar.on_hover_text("Mode complet").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            acts.push(Act::ToggleMini);
        }

        // Titre / artiste
        let y = art.bottom() + 16.0;
        let w = right - left;
        let title = if has_track { self.cur_info.title.as_str() } else { "Aucun son" };
        ui::text(ui, Pos2::new(cx, y), Align2::CENTER_TOP, title, ui::semibold(14.5), p.text, w);
        ui::text(ui, Pos2::new(cx, y + 21.0), Align2::CENTER_TOP, &self.cur_info.artist, ui::regular(12.0), p.dim, w);

        // Progression
        let py = y + 58.0;
        let dur = self.duration().as_secs_f32();
        let pos = self.seek_drag.unwrap_or_else(|| self.player.position().as_secs_f32()).min(dur);
        let frac = if dur > 0.0 { pos / dur } else { 0.0 };
        let seek_rect = Rect::from_min_size(Pos2::new(left, py - 7.0), Vec2::new(w, 14.0));
        if let Some(f) = ui::bar(ui, "mini_seek", seek_rect, frac, dur > 0.0 && self.player.is_loaded(), 4.5) {
            self.seek_drag = Some(f * dur);
        } else if let Some(t) = self.seek_drag.take() {
            self.player.seek(Duration::from_secs_f32(t));
            self.media_state = None;
        }

        // Précédent · lecture · suivant
        let cy = py + 38.0;
        if ui::icon_button(ui, "mini_prev", Pos2::new(cx - 60.0, cy), 34.0, icon::SKIP_BACK, ui::ph(20.0), p.text, has_track).clicked() {
            self.prev(&ctx);
        }
        if ui::play_button(ui, Pos2::new(cx, cy), 20.0, self.player.is_playing(), has_track).clicked() {
            self.toggle(&ctx);
        }
        if ui::icon_button(ui, "mini_next", Pos2::new(cx + 60.0, cy), 34.0, icon::SKIP_FORWARD, ui::ph(20.0), p.text, has_track).clicked() {
            self.next(&ctx, false);
        }
    }

    fn footer_info(&self) -> String {
        let n = self.queue.tracks.len();
        if n == 0 {
            return "Portable · aucune installation".into();
        }
        let total: f32 = self.queue.tracks.iter().filter_map(|t| self.meta_cache.get(t)?.duration).map(|d| d.as_secs_f32()).sum();
        let mins = (total / 60.0).round() as u64;
        if mins >= 60 {
            format!("{n} titre{} · {} h {:02}", plural(n), mins / 60, mins % 60)
        } else {
            format!("{n} titre{} · {mins} min", plural(n))
        }
    }

    fn add_to_playlist_popup(&self, ui: &mut egui::Ui, anchor: &egui::Response, acts: &mut Vec<Act>) {
        let Some(cur) = self.queue.current().cloned() else { return };
        let id = ui.make_persistent_id("add_pl");
        if anchor.clicked() {
            ui.memory_mut(|m| m.toggle_popup(id));
        }
        egui::popup_below_widget(ui, id, anchor, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(200.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            self.playlist_choices(ui, &cur, acts);
        });
    }

    fn playlist_choices(&self, ui: &mut egui::Ui, p: &Path, acts: &mut Vec<Act>) {
        for (name, _) in &self.playlists {
            if ui::menu_item(ui, icon::PLAYLIST, name).clicked() {
                acts.push(Act::AddToPlaylist(name.clone(), p.to_path_buf()));
                ui.close_menu();
            }
        }
        if !self.playlists.is_empty() {
            ui::menu_separator(ui);
        }
        if ui::menu_item(ui, icon::PLUS, "Nouvelle playlist").clicked() {
            acts.push(Act::NewPlaylistWith(p.to_path_buf()));
            ui.close_menu();
        }
    }

    // ================================================================ listes

    fn empty_state(ui: &mut egui::Ui, msg: &str, button: Option<&str>) -> bool {
        let r = ui.available_rect_before_wrap();
        let r = r.with_max_x(r.right() - ui::GUTTER);
        let c = Pos2::new(r.center().x, r.top() + (r.height() * 0.28).clamp(16.0, 70.0));
        for (k, line) in msg.lines().enumerate() {
            ui::text(ui, c + Vec2::new(0.0, k as f32 * 18.0), Align2::CENTER_TOP, line, ui::regular(12.5), ui::pal().dim, r.width());
        }
        let Some(label) = button else { return false };
        let by = c.y + msg.lines().count() as f32 * 18.0 + 28.0;
        ui::outline_button(ui, "empty_btn", Pos2::new(c.x, by), label).clicked()
    }

    fn track_texts(&self, p: &Path) -> (String, String, String) {
        match self.meta_cache.get(p) {
            Some(i) => (i.title.clone(), i.kind.clone(), i.duration.map(|d| clock(d.as_secs_f32())).unwrap_or_default()),
            None => (meta::file_stem(p), meta::ext_kind(p), String::new()),
        }
    }

    fn track_menu(&self, ui: &mut egui::Ui, p: &Path, play: Act, acts: &mut Vec<Act>) {
        if ui::menu_item(ui, icon::PLAY, "Lire").clicked() {
            acts.push(play);
            ui.close_menu();
        }
        if ui::menu_item(ui, icon::ARROW_BEND_DOWN_RIGHT, "Lire ensuite").clicked() {
            acts.push(Act::QueuePlayNext(p.to_path_buf()));
            ui.close_menu();
        }
        if ui::menu_item(ui, icon::PLUS_CIRCLE, "Ajouter à la file d'attente").clicked() {
            acts.push(Act::QueueAdd(p.to_path_buf()));
            ui.close_menu();
        }
        ui::menu_separator(ui);
        self.playlist_choices(ui, p, acts);
    }

    /// Bouton ⊕ / ✓ de mise en file d'attente pour un titre.
    fn queue_action(&self, p: &Path) -> ui::RowAction<'static> {
        if self.up_next.iter().any(|q| q == p) {
            Some((icon::CHECK_CIRCLE, true, "Dans la file d'attente — cliquer pour retirer"))
        } else {
            Some((icon::PLUS_CIRCLE, false, "Ajouter à la file d'attente"))
        }
    }

    /// Clic sur ⊕ / ✓ : ajoute ou retire le titre de la file.
    fn queue_toggle(&self, p: &Path, acts: &mut Vec<Act>) {
        match self.up_next.iter().position(|q| q == p) {
            Some(i) => acts.push(Act::QueueRemove(i)),
            None => acts.push(Act::QueueAdd(p.to_path_buf())),
        }
    }

    fn ui_queue(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let n = self.queue.tracks.len();
        if n == 0 {
            if Self::empty_state(ui, "Aucun son pour l'instant.", Some("Ouvrir un dossier")) {
                acts.push(Act::PickFolder);
            }
            return;
        }
        let q = self.query();
        let idx: Vec<usize> = match &q {
            Some(q) => (0..n).filter(|&i| self.track_matches(&self.queue.tracks[i], q)).collect(),
            None => (0..n).collect(),
        };
        if idx.is_empty() {
            Self::no_result(ui, self.search.trim());
            return;
        }
        let cur = if self.playing_context() { self.queue.current_index() } else { None };
        let mut sa = egui::ScrollArea::vertical().auto_shrink(false).id_salt("queue");
        if self.scroll_to_current {
            if let Some(row) = cur.and_then(|c| idx.iter().position(|&i| i == c)) {
                sa = sa.vertical_scroll_offset((row as f32 * ui::ROW_H - ui.available_height() / 2.0 + ui::ROW_H / 2.0).max(0.0));
            }
            self.scroll_to_current = false;
        }
        let m = idx.len();
        sa.show_rows(ui, ui::ROW_H, m, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for k in range {
                let i = idx[k];
                let p = &self.queue.tracks[i];
                let (title, kind, d) = self.track_texts(p);
                let (r, add) = ui::list_row_ex(
                    ui, self.thumbs.get(p), icon::MUSIC_NOTE, Some(&kind), &title, &d, cur == Some(i), k + 1 < m, self.queue_action(p),
                );
                if add {
                    self.queue_toggle(p, acts);
                } else if r.clicked() {
                    acts.push(Act::PlayIndex(i));
                }
                r.context_menu(|ui| self.track_menu(ui, p, Act::PlayIndex(i), acts));
            }
        });
    }

    /// File d'attente façon Spotify : en cours · ta file (réordonnable) · suite du contexte.
    fn ui_upnext(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let p = ui::pal();
        let q = self.query();
        let mut upcoming = self.queue.upcoming(40);
        if let Some(q) = &q {
            upcoming.retain(|&i| self.track_matches(&self.queue.tracks[i], q));
        }
        let shown: Vec<usize> = match &q {
            Some(q) => (0..self.up_next.len()).filter(|&i| self.track_matches(&self.up_next[i], q)).collect(),
            None => (0..self.up_next.len()).collect(),
        };
        if q.is_some() && shown.is_empty() && upcoming.is_empty() {
            Self::no_result(ui, self.search.trim());
            return;
        }
        if self.now.is_none() && self.up_next.is_empty() && upcoming.is_empty() {
            Self::empty_state(ui, "La file d'attente est vide.\nAjoute des titres avec ⊕ dans les autres onglets.", None);
            return;
        }
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let released = ui.input(|i| i.pointer.any_released());
        egui::ScrollArea::vertical().auto_shrink(false).id_salt("upnext").show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;

            // En cours (masqué pendant une recherche)
            if let (Some(now), None) = (self.now.clone(), &q) {
                ui::section_label(ui, "EN COURS");
                let (title, kind, d) = self.track_texts(&now);
                let r = ui::list_row(ui, self.thumbs.get(&now), icon::MUSIC_NOTE, Some(&kind), &title, &d, true, false);
                r.context_menu(|ui| self.playlist_choices(ui, &now, acts));
            }

            // Ta file : glisser pour réordonner
            if !shown.is_empty() {
                ui.add_space(6.0);
                if ui::list_toolbar(ui, "clear_q", "ENSUITE DANS TA FILE", Some((icon::TRASH, "Vider")), false) {
                    acts.push(Act::QueueClear);
                }
                let n = self.up_next.len();
                let mut rects = Vec::with_capacity(n);
                // Pendant une recherche, pas de glisser-déposer (les positions seraient trompeuses).
                let can_drag = q.is_none();
                for (k, &i) in shown.iter().enumerate() {
                    let item = &self.up_next[i];
                    let (title, kind, d) = self.track_texts(item);
                    let dragging = self.drag_from == Some(i);
                    let (r, removed) = ui::queue_row(ui, self.thumbs.get(item), &kind, &title, &d, dragging, k + 1 < shown.len());
                    rects.push(r.rect);
                    if removed {
                        acts.push(Act::QueueRemove(i));
                    } else if can_drag && r.drag_started() {
                        self.drag_from = Some(i);
                    } else if r.clicked() {
                        acts.push(Act::QueuePlayAt(i));
                    }
                    r.context_menu(|ui| {
                        if ui::menu_item(ui, icon::PLAY, "Lire maintenant").clicked() {
                            acts.push(Act::QueuePlayAt(i));
                            ui.close_menu();
                        }
                        if i > 0 && ui::menu_item(ui, icon::ARROW_UP, "Monter").clicked() {
                            acts.push(Act::QueueMove(i, i - 1));
                            ui.close_menu();
                        }
                        if i + 1 < n && ui::menu_item(ui, icon::ARROW_DOWN, "Descendre").clicked() {
                            acts.push(Act::QueueMove(i, i + 2));
                            ui.close_menu();
                        }
                        if ui::menu_item(ui, icon::X, "Retirer de la file").clicked() {
                            acts.push(Act::QueueRemove(i));
                            ui.close_menu();
                        }
                    });
                }
                // Glisser-déposer : ligne d'insertion puis déplacement au relâchement
                if let (Some(from), Some(pos), true) = (self.drag_from, pointer, can_drag) {
                    let target = rects.iter().filter(|r| r.center().y < pos.y).count();
                    let y = if target < rects.len() { rects[target].top() } else { rects[rects.len() - 1].bottom() };
                    let x0 = rects[0].left();
                    let x1 = rects[0].right();
                    ui.painter().hline(x0..=x1, y, Stroke::new(2.0, p.accent));
                    ui.painter().circle_filled(Pos2::new(x0, y), 3.5, p.accent);
                    if released {
                        acts.push(Act::QueueMove(from, target));
                        self.drag_from = None;
                    }
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
                if released {
                    self.drag_from = None;
                }
            }

            // Suite du contexte
            if !upcoming.is_empty() {
                ui.add_space(6.0);
                let label = format!("ENSUITE DEPUIS : {}", self.queue.source.to_uppercase());
                ui::section_label(ui, &label);
                let n = upcoming.len();
                for (k, &i) in upcoming.iter().enumerate() {
                    let t = &self.queue.tracks[i];
                    let (title, kind, d) = self.track_texts(t);
                    let (r, add) =
                        ui::list_row_ex(ui, self.thumbs.get(t), icon::MUSIC_NOTE, Some(&kind), &title, &d, false, k + 1 < n, self.queue_action(t));
                    if add {
                        self.queue_toggle(t, acts);
                    } else if r.clicked() {
                        acts.push(Act::PlayIndex(i));
                    }
                    r.context_menu(|ui| self.track_menu(ui, t, Act::PlayIndex(i), acts));
                }
            }
            ui.add_space(8.0);
        });
    }

    fn ui_history(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let n = self.history.len();
        if n == 0 {
            Self::empty_state(ui, "Aucune écoute pour l'instant.", None);
            return;
        }
        let q = self.query();
        // Index dans l'ordre d'affichage (le plus récent d'abord)
        let idx: Vec<usize> = (0..n)
            .rev()
            .filter(|&j| {
                q.as_ref().is_none_or(|q| {
                    let h = &self.history[j];
                    Self::text_matches(&format!("{} {} {}", h.title, h.artist, meta::file_stem(&h.path)), q)
                })
            })
            .collect();
        if q.is_none() {
            let label = format!("{n} ÉCOUTE{}", if n > 1 { "S" } else { "" });
            let (g, l) = if self.confirm_clear { ("", "Confirmer l'effacement") } else { (icon::TRASH, "Effacer") };
            if ui::list_toolbar(ui, "clear_hist", &label, Some((g, l)), self.confirm_clear) {
                if self.confirm_clear {
                    self.history.clear();
                    self.storage.save_history(&mut self.history);
                    self.confirm_clear = false;
                    return;
                }
                self.confirm_clear = true;
            }
        }
        if idx.is_empty() {
            Self::no_result(ui, self.search.trim());
            return;
        }
        let today = chrono::Local::now().date_naive();
        let m = idx.len();
        egui::ScrollArea::vertical().auto_shrink(false).id_salt("hist").show_rows(ui, ui::ROW_H, m, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for k in range {
                let h = &self.history[idx[k]];
                let when = chrono::DateTime::from_timestamp(h.played_at, 0)
                    .map(|d| {
                        let d = d.with_timezone(&chrono::Local);
                        if d.date_naive() == today { d.format("%H:%M").to_string() } else { d.format("%d/%m").to_string() }
                    })
                    .unwrap_or_default();
                let title = if h.artist.is_empty() { h.title.clone() } else { format!("{} — {}", h.title, h.artist) };
                let kind = meta::ext_kind(&h.path);
                let (r, add) = ui::list_row_ex(
                    ui, self.thumbs.get(&h.path), icon::MUSIC_NOTE, Some(&kind), &title, &when, false, k + 1 < m, self.queue_action(&h.path),
                );
                let r = r.on_hover_text(h.path.display().to_string());
                if add {
                    self.queue_toggle(&h.path, acts);
                } else if r.clicked() {
                    acts.push(Act::OpenFile(h.path.clone()));
                }
                r.context_menu(|ui| self.track_menu(ui, &h.path, Act::OpenFile(h.path.clone()), acts));
            }
        });
    }

    fn ui_playlists(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let q = self.query();
        if q.is_none() && !self.playlists.is_empty() && self.new_pl.is_none() {
            let np = self.playlists.len();
            let label = format!("{np} PLAYLIST{}", if np > 1 { "S" } else { "" });
            if ui::list_toolbar(ui, "new_pl", &label, Some((icon::PLUS, "Nouvelle")), false) {
                self.new_pl = Some(String::new());
            }
        }
        if let Some(buf) = &mut self.new_pl {
            ui.add_space(8.0);
            let r = ui::text_field(ui, buf, "Nom de la playlist, puis Entrée");
            r.request_focus();
            let (enter, esc) = ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
            if enter || esc {
                let name = sanitize_name(buf);
                if enter && !name.is_empty() && !self.playlists.iter().any(|(x, _)| *x == name) {
                    self.storage.save_playlist(&name, &[]);
                    self.refresh_playlists();
                }
                self.new_pl = None;
            }
            ui.add_space(4.0);
        }
        if self.playlists.is_empty() {
            if self.new_pl.is_none() && Self::empty_state(ui, "Aucune playlist.", Some("Créer une playlist")) {
                self.new_pl = Some(String::new());
            }
            return;
        }
        let shown: Vec<&(String, usize)> =
            self.playlists.iter().filter(|(name, _)| q.as_ref().is_none_or(|q| Self::text_matches(name, q))).collect();
        if shown.is_empty() {
            Self::no_result(ui, self.search.trim());
            return;
        }
        let n = shown.len();
        egui::ScrollArea::vertical().auto_shrink(false).id_salt("pls").show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (k, (name, count)) in shown.into_iter().enumerate() {
                let info = format!("{count} titre{}", plural(*count));
                let r = ui::list_row(ui, None, icon::PLAYLIST, None, name, &info, false, k + 1 < n);
                if r.clicked() {
                    acts.push(Act::Go(Panel::Playlist(name.clone())));
                }
                r.context_menu(|ui| {
                    if ui::menu_item(ui, icon::PLAY, "Lire").clicked() {
                        acts.push(Act::PlayPlaylist(name.clone(), 0));
                        ui.close_menu();
                    }
                });
            }
        });
    }

    fn ui_playlist(&mut self, ui: &mut egui::Ui, name: String, acts: &mut Vec<Act>) {
        let p = ui::pal();
        let n = self.pl_tracks.len();
        // Sous-en-tête : retour · nom · actions
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width() - ui::GUTTER, 40.0), egui::Sense::hover());
        let cy = rect.center().y;
        if let Some(buf) = &mut self.rename_pl {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(0.0, 4.0))));
            let r = child.add(
                egui::TextEdit::singleline(buf)
                    .font(ui::medium(13.0))
                    .text_color(p.text)
                    .margin(egui::Margin::symmetric(8, 5))
                    .desired_width(rect.width()),
            );
            r.request_focus();
            let (enter, esc) = ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
            if enter || esc {
                let new = sanitize_name(buf);
                if enter && !new.is_empty() && new != name && !self.playlists.iter().any(|(x, _)| *x == new) {
                    self.storage.rename_playlist(&name, &new);
                    self.refresh_playlists();
                    self.panel = Panel::Playlist(new);
                }
                self.rename_pl = None;
            }
        } else {
            if ui::icon_button(ui, "pl_back", Pos2::new(rect.left() + 10.0, cy), 26.0, icon::CARET_LEFT, ui::ph(15.0), p.soft, true)
                .on_hover_text("Retour (Échap)")
                .clicked()
            {
                acts.push(Act::Go(Panel::Playlists));
            }
            let nr = ui::text(ui, Pos2::new(rect.left() + 28.0, cy), Align2::LEFT_CENTER, &name, ui::medium(13.0), p.text, rect.width() - 130.0);
            let count = format!("{n} titre{}", plural(n));
            ui::text(ui, Pos2::new(nr.right() + 8.0, cy), Align2::LEFT_CENTER, &count, ui::regular(11.5), p.faint, 80.0);
            if ui::icon_button(ui, "pl_del", Pos2::new(rect.right() - 12.0, cy), 28.0, icon::TRASH, ui::ph(15.0), p.dim, true)
                .on_hover_text("Double-clic pour supprimer la playlist")
                .double_clicked()
            {
                self.storage.delete_playlist(&name);
                acts.push(Act::Go(Panel::Playlists));
            }
            if ui::icon_button(ui, "pl_ren", Pos2::new(rect.right() - 42.0, cy), 28.0, icon::PENCIL_SIMPLE, ui::ph(15.0), p.dim, true)
                .on_hover_text("Renommer")
                .clicked()
            {
                self.rename_pl = Some(name.clone());
            }
            if ui::icon_button(ui, "pl_play", Pos2::new(rect.right() - 72.0, cy), 28.0, icon::PLAY, ui::phf(14.0), p.text, n > 0)
                .on_hover_text("Lire la playlist")
                .clicked()
            {
                acts.push(Act::PlayPlaylist(name.clone(), 0));
            }
        }
        if n == 0 {
            Self::empty_state(ui, "Playlist vide.\nAjoute le titre en cours depuis l'en-tête\nou par clic droit dans la file.", None);
            return;
        }
        let q = self.query();
        let idx: Vec<usize> = (0..n).filter(|&i| q.as_ref().is_none_or(|q| self.track_matches(&self.pl_tracks[i], q))).collect();
        if idx.is_empty() {
            Self::no_result(ui, self.search.trim());
            return;
        }
        let m = idx.len();
        egui::ScrollArea::vertical().auto_shrink(false).id_salt("pl").show_rows(ui, ui::ROW_H, m, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for k in range {
                let i = idx[k];
                let tp = &self.pl_tracks[i];
                let (title, kind, d) = self.track_texts(tp);
                let (r, add) =
                    ui::list_row_ex(ui, self.thumbs.get(tp), icon::MUSIC_NOTE, Some(&kind), &title, &d, false, k + 1 < m, self.queue_action(tp));
                if add {
                    self.queue_toggle(tp, acts);
                } else if r.clicked() {
                    acts.push(Act::PlayPlaylist(name.clone(), i));
                }
                r.context_menu(|ui| {
                    if ui::menu_item(ui, icon::PLAY, "Lire à partir d'ici").clicked() {
                        acts.push(Act::PlayPlaylist(name.clone(), i));
                        ui.close_menu();
                    }
                    if ui::menu_item(ui, icon::ARROW_BEND_DOWN_RIGHT, "Lire ensuite").clicked() {
                        acts.push(Act::QueuePlayNext(tp.clone()));
                        ui.close_menu();
                    }
                    if ui::menu_item(ui, icon::X, "Retirer de la playlist").clicked() {
                        acts.push(Act::RemoveFromPlaylist(i));
                        ui.close_menu();
                    }
                });
            }
        });
    }
}

fn plural(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}

/// Temps au format 01:12 (ou 1:02:03 au-delà d'une heure).
fn clock(secs: f32) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for (p, mut info) in self.meta_rx.try_iter() {
            if let Some(img) = info.thumb.take() {
                let tex = ctx.load_texture("thumb", img, egui::TextureOptions::LINEAR);
                self.thumbs.entry(p.clone()).or_insert(tex);
            }
            self.meta_cache.insert(p, info);
        }

        self.handle_input(ctx);
        self.handle_media(ctx);

        // Enchaînement automatique en fin de piste
        if self.player.is_loaded() && self.player.is_finished() {
            self.next(ctx, true);
        }

        if self.title_dirty {
            self.title_dirty = false;
            let t = if self.queue.current().is_some() {
                if self.cur_info.artist.is_empty() {
                    format!("{} — Adio", self.cur_info.title)
                } else {
                    format!("{} · {} — Adio", self.cur_info.title, self.cur_info.artist)
                }
            } else {
                "Adio".into()
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(t));
        }

        let mut acts = Vec::new();
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| self.ui_card(ui, &mut acts));
        for a in acts {
            self.apply(ctx, a);
        }

        // Glisser-déposer : voile + message
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let p = ui::pal();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            let card = ctx.screen_rect().shrink(MARGIN);
            painter.rect_filled(card, RADIUS, p.card.gamma_multiply(0.97));
            painter.rect_stroke(card.shrink(12.0), 12, Stroke::new(1.0, p.track), egui::StrokeKind::Inside);
            painter.text(card.center() - Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, icon::DOWNLOAD_SIMPLE, ui::ph(26.0), p.text);
            painter.text(card.center() + Vec2::new(0.0, 18.0), Align2::CENTER_CENTER, "Déposer pour lire", ui::medium(13.0), p.text);
        }

        if self.settings_dirty {
            self.settings_dirty = false;
            self.storage.save_settings(&self.settings);
        }

        // Rafraîchissement uniquement pendant la lecture : ~0 % CPU en pause.
        if self.player.is_playing() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}

fn read_m3u(p: &Path) -> Vec<PathBuf> {
    let base = p.parent().map(Path::to_path_buf).unwrap_or_default();
    std::fs::read_to_string(p)
        .map(|s| {
            s.lines()
                .map(|l| l.trim().trim_start_matches('\u{feff}'))
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| {
                    let q = PathBuf::from(l);
                    if q.is_absolute() { q } else { base.join(q) }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn window_hwnd(cc: &eframe::CreationContext<'_>) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match cc.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as *mut std::ffi::c_void),
        _ => None,
    }
}
