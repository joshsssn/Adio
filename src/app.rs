//! Interface egui et logique de lecture.

use crate::media_keys::{MediaCmd, MediaKeys};
use crate::meta::{self, fmt_dur, TrackInfo};
use crate::player::Player;
use crate::queue::{self, Queue};
use crate::storage::{sanitize_name, HistoryEntry, Repeat, Settings, Storage};
use eframe::egui::{self, Align, Color32, Key, Layout, RichText, TextWrapMode, Vec2};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

pub const ACCENT: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x4A);
const ROW_H: f32 = 22.0;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Queue,
    History,
    Playlists,
}

/// Actions collectées pendant le rendu, appliquées ensuite (évite les conflits d'emprunt).
enum Act {
    PlayIndex(usize),
    OpenFile(PathBuf),
    AddToPlaylist(String, PathBuf),
    NewPlaylistWith(PathBuf),
    PlayPlaylist(String, usize),
    RemoveFromPlaylist(usize),
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
    meta_tx: Sender<(PathBuf, TrackInfo)>,
    meta_rx: Receiver<(PathBuf, TrackInfo)>,

    media: Option<MediaKeys>,
    media_state: Option<bool>,

    tab: Tab,
    error: Option<String>,
    seek_drag: Option<f32>,
    scroll_to_current: bool,
    title_dirty: bool,

    playlists: Vec<String>,
    open_pl: Option<String>,
    open_pl_tracks: Vec<PathBuf>,
    new_pl_name: String,
    rename_buf: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, arg: Option<PathBuf>) -> Self {
        setup_style(&cc.egui_ctx);
        let storage = Storage::init();
        let settings = storage.load_settings();
        let history = storage.load_history();
        let playlists = storage.list_playlists();
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
            meta_tx,
            meta_rx,
            media,
            media_state: None,
            tab: Tab::Queue,
            error: None,
            seek_drag: None,
            scroll_to_current: true,
            title_dirty: true,
            playlists,
            open_pl: None,
            open_pl_tracks: Vec::new(),
            new_pl_name: String::new(),
            rename_buf: String::new(),
        };
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

    // ---------------------------------------------------------------- logique

    fn open_path(&mut self, ctx: &egui::Context, p: &Path, autoplay: bool) {
        let shuffle = self.settings.shuffle;
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let q = if p.is_dir() {
            Queue::from_folder(p, shuffle)
        } else if ext == "m3u8" || ext == "m3u" {
            let tracks = read_m3u(p);
            Queue::from_tracks(tracks, 0, shuffle, meta::file_stem(p))
        } else {
            Queue::from_file(p, shuffle)
        };
        if q.is_empty() {
            self.error = Some(format!("Aucun son trouvé dans « {} »", p.display()));
            return;
        }
        self.set_queue(ctx, q, autoplay);
    }

    fn open_files(&mut self, ctx: &egui::Context, files: Vec<PathBuf>) {
        let files: Vec<PathBuf> = files.into_iter().filter(|f| queue::is_audio(f)).collect();
        if files.is_empty() {
            return;
        }
        let q = Queue::from_tracks(files, 0, self.settings.shuffle, "Sélection".into());
        self.set_queue(ctx, q, true);
    }

    fn set_queue(&mut self, ctx: &egui::Context, q: Queue, autoplay: bool) {
        self.queue = q;
        let missing: Vec<PathBuf> =
            self.queue.tracks.iter().filter(|t| !self.meta_cache.contains_key(*t)).cloned().collect();
        if !missing.is_empty() {
            meta::spawn_scan(missing, self.meta_tx.clone(), ctx.clone());
        }
        self.scroll_to_current = true;
        self.play_current(ctx, autoplay);
    }

    fn play_current(&mut self, ctx: &egui::Context, autoplay: bool) {
        let Some(path) = self.queue.current().cloned() else { return };
        let (info, cover_bytes) = meta::read(&path, true);
        self.meta_cache.insert(path.clone(), info.clone());
        self.cur_info = info;
        self.seek_drag = None;
        self.title_dirty = true;
        self.scroll_to_current = true;

        match self.player.load(&path, autoplay) {
            Ok(()) => self.error = None,
            Err(e) => {
                self.error = Some(format!("{} : {e}", meta::file_stem(&path)));
                self.player.stop();
            }
        }

        // Pochette : texture egui + copie fichier pour l'overlay Windows.
        self.cover = None;
        let mut cover_url = None;
        if let Some(bytes) = cover_bytes {
            if let Ok(img) = image::load_from_memory(&bytes) {
                let img = img.thumbnail(512, 512).to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                self.cover = Some(ctx.load_texture("cover", ci, egui::TextureOptions::LINEAR));
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

    fn next(&mut self, ctx: &egui::Context, auto: bool) {
        let repeat = self.settings.repeat;
        if self.queue.next(repeat, auto).is_some() {
            self.play_current(ctx, true);
        } else {
            // Fin de liste sans répétition : on revient au début, en pause.
            self.player.stop();
            self.media_state = None;
        }
    }

    fn prev(&mut self, ctx: &egui::Context) {
        if self.player.position() > Duration::from_secs(3) {
            self.player.seek(Duration::ZERO);
            self.media_state = None;
        } else if self.queue.prev().is_some() {
            self.play_current(ctx, true);
        }
    }

    fn toggle(&mut self, ctx: &egui::Context) {
        if self.player.is_loaded() && !self.player.is_finished() {
            self.player.toggle();
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

    fn refresh_playlists(&mut self) {
        self.playlists = self.storage.list_playlists();
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
                    self.error = Some(format!("Fichier introuvable : {}", p.display()));
                }
            }
            Act::AddToPlaylist(name, p) => {
                let mut t = self.storage.load_playlist(&name);
                t.push(p);
                self.storage.save_playlist(&name, &t);
                if self.open_pl.as_deref() == Some(name.as_str()) {
                    self.open_pl_tracks = t;
                }
            }
            Act::NewPlaylistWith(p) => {
                let mut n = 1;
                let name = loop {
                    let candidate = format!("Playlist {n}");
                    if !self.playlists.contains(&candidate) {
                        break candidate;
                    }
                    n += 1;
                };
                self.storage.save_playlist(&name, &[p]);
                self.refresh_playlists();
            }
            Act::PlayPlaylist(name, start) => {
                let tracks: Vec<PathBuf> = self.storage.load_playlist(&name);
                if tracks.is_empty() {
                    return;
                }
                let q = Queue::from_tracks(tracks, start, false, name);
                let mut q = q;
                if self.settings.shuffle {
                    q.set_shuffle(true);
                }
                self.set_queue(ctx, q, true);
                self.tab = Tab::Queue;
            }
            Act::RemoveFromPlaylist(i) => {
                if let Some(name) = self.open_pl.clone() {
                    if i < self.open_pl_tracks.len() {
                        self.open_pl_tracks.remove(i);
                        self.storage.save_playlist(&name, &self.open_pl_tracks);
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------- entrées

    fn handle_input(&mut self, ctx: &egui::Context) {
        // Glisser-déposer
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if dropped.len() == 1 {
            self.open_path(ctx, &dropped[0], true);
        } else if dropped.len() > 1 {
            self.open_files(ctx, dropped);
        }

        // Raccourcis (sauf si on tape dans un champ texte)
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        use egui::Modifiers as M;
        let mut acts: Vec<u8> = Vec::new();
        ctx.input_mut(|i| {
            if i.consume_key(M::NONE, Key::Space) { acts.push(0) }
            if i.consume_key(M::CTRL, Key::ArrowRight) { acts.push(1) }
            if i.consume_key(M::CTRL, Key::ArrowLeft) { acts.push(2) }
            if i.consume_key(M::NONE, Key::ArrowRight) { acts.push(3) }
            if i.consume_key(M::NONE, Key::ArrowLeft) { acts.push(4) }
            if i.consume_key(M::NONE, Key::ArrowUp) { acts.push(5) }
            if i.consume_key(M::NONE, Key::ArrowDown) { acts.push(6) }
            if i.consume_key(M::NONE, Key::S) { acts.push(7) }
            if i.consume_key(M::NONE, Key::R) { acts.push(8) }
            if i.consume_key(M::NONE, Key::M) { acts.push(9) }
            if i.consume_key(M::CTRL, Key::O) { acts.push(10) }
        });
        for a in acts {
            match a {
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
            if f.len() == 1 {
                self.open_path(ctx, &f[0], true);
            } else {
                self.open_files(ctx, f);
            }
        }
    }

    // ---------------------------------------------------------------- rendu

    fn ui_header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if ui.button("📂").on_hover_text("Ouvrir un dossier (Ctrl+O)").clicked() {
                self.pick_folder(ctx);
            }
            if ui.button("🎵").on_hover_text("Ouvrir des fichiers / une playlist").clicked() {
                self.pick_files(ctx);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if self.settings.show_list { "⏶" } else { "☰" };
                if ui.button(label).on_hover_text("Afficher / masquer la liste").clicked() {
                    self.settings.show_list = !self.settings.show_list;
                    self.settings_dirty = true;
                    let h = if self.settings.show_list { 560.0 } else { 470.0 };
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(ui.ctx().screen_rect().width(), h)));
                }
                ui.add(egui::Label::new(RichText::new(&self.queue.source).weak()).truncate());
            });
        });
    }

    fn ui_now_playing(&mut self, ui: &mut egui::Ui) {
        let compact = self.settings.show_list;
        let size = if compact { 96.0 } else { (ui.available_width() - 16.0).clamp(120.0, 320.0) };

        let draw_cover = |ui: &mut egui::Ui, cover: &Option<egui::TextureHandle>| {
            if let Some(t) = cover {
                ui.add(egui::Image::new(t).fit_to_exact_size(Vec2::splat(size)).corner_radius(6.0));
            } else {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
                ui.painter().rect_filled(r, 6.0, Color32::from_gray(38));
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    "♪",
                    egui::FontId::proportional(size * 0.45),
                    Color32::from_gray(90),
                );
            }
        };

        let info = self.cur_info.clone();
        let has_track = self.queue.current().is_some();
        let text_block = |ui: &mut egui::Ui| {
            ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
            let title = if has_track { info.title.as_str() } else { "Aucun son" };
            ui.label(RichText::new(title).size(17.0).strong().color(Color32::WHITE));
            if !info.artist.is_empty() {
                ui.label(RichText::new(&info.artist).size(14.0));
            }
            if !info.album.is_empty() {
                ui.label(RichText::new(&info.album).weak());
            }
            if !has_track {
                ui.label(RichText::new("Glisse un dossier ou un son ici").weak());
            }
        };

        if compact {
            ui.horizontal(|ui| {
                draw_cover(ui, &self.cover);
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    text_block(ui);
                });
            });
        } else {
            ui.vertical_centered(|ui| {
                draw_cover(ui, &self.cover);
                ui.add_space(6.0);
                text_block(ui);
            });
        }
    }

    fn ui_progress(&mut self, ui: &mut egui::Ui) {
        let dur = self.duration().as_secs_f32().max(0.0);
        let pos = self.player.position().as_secs_f32().min(dur);
        let mut v = self.seek_drag.unwrap_or(pos);

        ui.spacing_mut().slider_width = ui.available_width();
        let r = ui.add_enabled(
            dur > 0.0,
            egui::Slider::new(&mut v, 0.0..=dur.max(0.001)).show_value(false).trailing_fill(true),
        );
        if r.changed() {
            self.seek_drag = Some(v);
        }
        if let Some(target) = self.seek_drag {
            if !r.dragged() && !r.is_pointer_button_down_on() {
                self.player.seek(Duration::from_secs_f32(target));
                self.seek_drag = None;
                self.media_state = None;
            }
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(fmt_dur(Duration::from_secs_f32(v))).monospace().weak());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(fmt_dur(Duration::from_secs_f32(dur))).monospace().weak());
            });
        });
    }

    fn ui_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let big = |s: &str| RichText::new(s).size(22.0);
        let toggle_col = |on: bool| if on { ACCENT } else { Color32::from_gray(120) };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let sh = self.settings.shuffle;
            if ui.add(egui::Button::new(RichText::new("🔀").size(17.0).color(toggle_col(sh))).frame(false))
                .on_hover_text("Aléatoire (S)").clicked()
            {
                self.toggle_shuffle();
            }
            if ui.add(egui::Button::new(big("⏮")).frame(false)).on_hover_text("Précédent (Ctrl+←)").clicked() {
                self.prev(ctx);
            }
            let play_icon = if self.player.is_playing() { "⏸" } else { "▶" };
            let play = egui::Button::new(RichText::new(play_icon).size(24.0).color(Color32::BLACK))
                .fill(ACCENT)
                .corner_radius(20.0)
                .min_size(Vec2::new(46.0, 40.0));
            if ui.add(play).on_hover_text("Lecture / pause (Espace)").clicked() {
                self.toggle(ctx);
            }
            if ui.add(egui::Button::new(big("⏭")).frame(false)).on_hover_text("Suivant (Ctrl+→)").clicked() {
                self.next(ctx, false);
            }
            let (ic, tip) = match self.settings.repeat {
                Repeat::Off => ("🔁", "Répéter : non (R)"),
                Repeat::All => ("🔁", "Répéter : tout (R)"),
                Repeat::One => ("🔂", "Répéter : un titre (R)"),
            };
            let on = self.settings.repeat != Repeat::Off;
            if ui.add(egui::Button::new(RichText::new(ic).size(17.0).color(toggle_col(on))).frame(false))
                .on_hover_text(tip).clicked()
            {
                self.cycle_repeat();
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut vol = self.player.volume();
                ui.spacing_mut().slider_width = ui.available_width().clamp(40.0, 90.0) - 26.0;
                let icon = if vol == 0.0 { "🔇" } else if vol < 0.5 { "🔉" } else { "🔊" };
                if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false)).changed() {
                    self.set_volume(vol);
                }
                if ui.add(egui::Button::new(icon).frame(false)).on_hover_text("Muet (M)").clicked() {
                    self.toggle_mute();
                }
            });
        });
    }

    fn ui_tabs(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Queue, format!("File ({})", self.queue.tracks.len()));
            ui.selectable_value(&mut self.tab, Tab::History, "Historique");
            ui.selectable_value(&mut self.tab, Tab::Playlists, "Playlists");
        });
        ui.separator();
        ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
        match self.tab {
            Tab::Queue => self.ui_queue(ui, acts),
            Tab::History => self.ui_history(ui, acts),
            Tab::Playlists => self.ui_playlists(ui, acts),
        }
    }

    fn track_label(&self, p: &Path) -> (String, String) {
        match self.meta_cache.get(p) {
            Some(i) => (i.title.clone(), i.artist.clone()),
            None => (meta::file_stem(p), String::new()),
        }
    }

    fn add_to_playlist_menu(&self, ui: &mut egui::Ui, p: &Path, acts: &mut Vec<Act>) {
        ui.menu_button("Ajouter à une playlist", |ui| {
            for name in &self.playlists {
                if ui.button(name).clicked() {
                    acts.push(Act::AddToPlaylist(name.clone(), p.to_path_buf()));
                    ui.close_menu();
                }
            }
            if !self.playlists.is_empty() {
                ui.separator();
            }
            if ui.button("➕ Nouvelle playlist").clicked() {
                acts.push(Act::NewPlaylistWith(p.to_path_buf()));
                ui.close_menu();
            }
        });
    }

    fn ui_queue(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let n = self.queue.tracks.len();
        let cur = self.queue.current_index();
        let mut sa = egui::ScrollArea::vertical().auto_shrink(false).id_salt("queue");
        if self.scroll_to_current {
            if let Some(c) = cur {
                let row = ROW_H + ui.spacing().item_spacing.y;
                sa = sa.vertical_scroll_offset((c as f32 * row - ui.available_height() / 2.0).max(0.0));
            }
            self.scroll_to_current = false;
        }
        sa.show_rows(ui, ROW_H, n, |ui, range| {
            ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                for i in range {
                    let p = &self.queue.tracks[i];
                    let (title, artist) = self.track_label(p);
                    let is_cur = cur == Some(i);
                    let text = if artist.is_empty() { title } else { format!("{title}  ·  {artist}") };
                    let r = row(ui, is_cur, &text);
                    if r.clicked() {
                        acts.push(Act::PlayIndex(i));
                    }
                    r.context_menu(|ui| {
                        if ui.button("▶ Lire").clicked() {
                            acts.push(Act::PlayIndex(i));
                            ui.close_menu();
                        }
                        self.add_to_playlist_menu(ui, p, acts);
                    });
                }
            });
        });
    }

    fn ui_history(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} écoutes", self.history.len())).weak());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.history.is_empty() && ui.small_button("Effacer").clicked() {
                    self.history.clear();
                    self.storage.save_history(&mut self.history);
                }
            });
        });
        let n = self.history.len();
        egui::ScrollArea::vertical().auto_shrink(false).id_salt("hist").show_rows(ui, ROW_H, n, |ui, range| {
            ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                for k in range {
                    let h = &self.history[n - 1 - k];
                    let when = chrono::DateTime::from_timestamp(h.played_at, 0)
                        .map(|d| d.with_timezone(&chrono::Local).format("%d/%m %H:%M").to_string())
                        .unwrap_or_default();
                    let main = if h.artist.is_empty() { h.title.clone() } else { format!("{}  ·  {}", h.title, h.artist) };
                    let r = row(ui, false, &format!("{when}   {main}")).on_hover_text(h.path.display().to_string());
                    if r.clicked() {
                        acts.push(Act::OpenFile(h.path.clone()));
                    }
                    r.context_menu(|ui| self.add_to_playlist_menu(ui, &h.path, acts));
                }
            });
        });
    }

    fn ui_playlists(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        if let Some(name) = self.open_pl.clone() {
            // ---- Détail d'une playlist
            ui.horizontal(|ui| {
                if ui.button("⬅").clicked() {
                    self.open_pl = None;
                }
                let r = ui.add(egui::TextEdit::singleline(&mut self.rename_buf).desired_width(120.0));
                if r.lost_focus() {
                    let new = sanitize_name(&self.rename_buf);
                    if !new.is_empty() && new != name && !self.playlists.contains(&new) {
                        self.storage.rename_playlist(&name, &new);
                        self.open_pl = Some(new);
                        self.refresh_playlists();
                    } else {
                        self.rename_buf = name.clone();
                    }
                }
                if ui.button("▶").on_hover_text("Lire la playlist").clicked() {
                    acts.push(Act::PlayPlaylist(name.clone(), 0));
                }
                if let Some(cur) = self.queue.current() {
                    if ui.button("➕").on_hover_text("Ajouter le titre en cours").clicked() {
                        acts.push(Act::AddToPlaylist(name.clone(), cur.clone()));
                    }
                }
                if ui.button("🗑").on_hover_text("Supprimer la playlist").clicked() {
                    self.storage.delete_playlist(&name);
                    self.open_pl = None;
                    self.refresh_playlists();
                }
            });
            if self.open_pl.is_none() {
                return;
            }
            let n = self.open_pl_tracks.len();
            egui::ScrollArea::vertical().auto_shrink(false).id_salt("pl").show_rows(ui, ROW_H, n, |ui, range| {
                for i in range {
                    let p = &self.open_pl_tracks[i];
                    let (title, artist) = self.track_label(p);
                    ui.horizontal(|ui| {
                        if ui.small_button("✖").on_hover_text("Retirer").clicked() {
                            acts.push(Act::RemoveFromPlaylist(i));
                        }
                        let text = if artist.is_empty() { title } else { format!("{title}  ·  {artist}") };
                        let r = row(ui, false, &text).on_hover_text(p.display().to_string());
                        if r.clicked() {
                            acts.push(Act::PlayPlaylist(name.clone(), i));
                        }
                    });
                }
            });
        } else {
            // ---- Liste des playlists
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.new_pl_name).hint_text("Nouvelle playlist…").desired_width(170.0),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if (ui.button("Créer").clicked() || enter) && !sanitize_name(&self.new_pl_name).is_empty() {
                    let name = sanitize_name(&self.new_pl_name);
                    if !self.playlists.contains(&name) {
                        self.storage.save_playlist(&name, &[]);
                        self.refresh_playlists();
                    }
                    self.new_pl_name.clear();
                }
            });
            egui::ScrollArea::vertical().auto_shrink(false).id_salt("pls").show(ui, |ui| {
                ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                    if self.playlists.is_empty() {
                        ui.label(RichText::new("Aucune playlist. Clic droit sur un titre pour l'ajouter.").weak());
                    }
                    for name in self.playlists.clone() {
                        let r = row(ui, false, &format!("🎶  {name}"));
                        if r.clicked() {
                            self.open_pl_tracks = self.storage.load_playlist(&name);
                            let missing: Vec<PathBuf> =
                                self.open_pl_tracks.iter().filter(|t| !self.meta_cache.contains_key(*t)).cloned().collect();
                            if !missing.is_empty() {
                                meta::spawn_scan(missing, self.meta_tx.clone(), ui.ctx().clone());
                            }
                            self.rename_buf = name.clone();
                            self.open_pl = Some(name.clone());
                        }
                        if r.double_clicked() {
                            acts.push(Act::PlayPlaylist(name.clone(), 0));
                        }
                    }
                });
            });
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Métadonnées chargées en arrière-plan
        for (p, info) in self.meta_rx.try_iter() {
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
        egui::CentralPanel::default().show(ctx, |ui| {
            self.ui_header(ui, ctx);
            ui.add_space(4.0);
            self.ui_now_playing(ui);
            ui.add_space(8.0);
            self.ui_progress(ui);
            self.ui_controls(ui, ctx);
            if let Some(e) = &self.error {
                ui.label(RichText::new(format!("⚠ {e}")).color(Color32::from_rgb(255, 120, 100)).small());
            }
            if self.settings.show_list {
                ui.add_space(4.0);
                self.ui_tabs(ui, &mut acts);
            }
        });
        for a in acts {
            self.apply(ctx, a);
        }

        // Indicateur visuel pendant un glisser-déposer
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            let r = ctx.screen_rect();
            painter.rect_filled(r, 0.0, Color32::from_black_alpha(200));
            painter.text(r.center(), egui::Align2::CENTER_CENTER, "Déposer pour lire", egui::FontId::proportional(20.0), ACCENT);
        }

        if self.settings_dirty {
            self.settings_dirty = false;
            self.storage.save_settings(&self.settings);
        }

        // Rafraîchissement uniquement pendant la lecture : ~0 % CPU en pause.
        if self.player.is_playing() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

/// Ligne de liste cliquable, texte aligné à gauche et tronqué.
fn row(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, 4.0, ACCENT.linear_multiply(0.22));
            p.rect_filled(egui::Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())), 1.5, ACCENT);
        } else if r.hovered() {
            p.rect_filled(rect, 4.0, Color32::from_gray(40));
        }
        let color = if selected { Color32::WHITE } else if r.hovered() { Color32::WHITE } else { Color32::from_gray(190) };
        let font = egui::FontId::proportional(14.0);
        let galley = egui::WidgetText::from(RichText::new(text).font(font).color(color)).into_galley(
            ui,
            Some(TextWrapMode::Truncate),
            rect.width() - 16.0,
            egui::TextStyle::Body,
        );
        p.galley(egui::pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0), galley, color);
    }
    r
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

fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(0x16, 0x16, 0x1a);
    v.window_fill = Color32::from_rgb(0x1e, 0x1e, 0x24);
    v.extreme_bg_color = Color32::from_rgb(0x10, 0x10, 0x13);
    v.selection.bg_fill = ACCENT.linear_multiply(0.35);
    v.selection.stroke.color = ACCENT;
    v.widgets.inactive.bg_fill = Color32::from_rgb(0x2a, 0x2a, 0x32);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x26, 0x26, 0x2d);
    v.slider_trailing_fill = true;
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 4.0);
        s.spacing.button_padding = Vec2::new(8.0, 4.0);
        s.spacing.slider_rail_height = 5.0;
    });
}
