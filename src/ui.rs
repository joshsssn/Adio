//! Design system : palettes claire/sombre, police Inter, icônes centrées optiquement, widgets.

use eframe::egui::{
    self, text::LayoutJob, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect,
    Response, Sense, Stroke, Ui, Vec2,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub use egui_phosphor::regular as icon;

// ---------------------------------------------------------------- palettes

pub struct Pal {
    pub card: Color32,
    pub text: Color32,
    pub soft: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub line: Color32,
    pub track: Color32,
    pub tile: Color32,
    pub hover: Color32,
    pub ink: Color32,
    pub on_ink: Color32,
    pub accent: Color32,
    pub border: Color32,
    pub shadow: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub static LIGHT: Pal = Pal {
    card: rgb(0xF7, 0xF6, 0xF3),
    text: rgb(0x24, 0x24, 0x23),
    soft: rgb(0x5C, 0x5B, 0x58),
    dim: rgb(0x8E, 0x8D, 0x88),
    faint: rgb(0xB2, 0xB1, 0xAC),
    line: rgb(0xE6, 0xE5, 0xE1),
    track: rgb(0xDD, 0xDC, 0xD8),
    tile: rgb(0xEB, 0xEA, 0xE6),
    hover: rgb(0xEE, 0xED, 0xE9),
    ink: rgb(0x2D, 0x2D, 0x2C),
    on_ink: rgb(0xFF, 0xFF, 0xFF),
    accent: rgb(0x8B, 0xA3, 0x86),
    border: rgb(0xE2, 0xE1, 0xDD),
    shadow: Color32::from_black_alpha(30),
};

pub static DARK: Pal = Pal {
    card: rgb(0x1C, 0x1C, 0x1E),
    text: rgb(0xF0, 0xEF, 0xEC),
    soft: rgb(0xC4, 0xC3, 0xBF),
    dim: rgb(0x8C, 0x8B, 0x87),
    faint: rgb(0x5E, 0x5E, 0x5B),
    line: rgb(0x2D, 0x2D, 0x30),
    track: rgb(0x3A, 0x3A, 0x3D),
    tile: rgb(0x2A, 0x2A, 0x2D),
    hover: rgb(0x27, 0x27, 0x2A),
    ink: rgb(0xEE, 0xED, 0xEA),
    on_ink: rgb(0x1C, 0x1C, 0x1E),
    accent: rgb(0x9C, 0xB5, 0x96),
    border: rgb(0x33, 0x33, 0x36),
    shadow: Color32::from_black_alpha(90),
};

static IS_DARK: AtomicBool = AtomicBool::new(false);

pub fn pal() -> &'static Pal {
    if IS_DARK.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

pub fn is_dark() -> bool {
    IS_DARK.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------- polices

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn ph(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("ph".into()))
}
pub fn phf(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("phf".into()))
}

pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "inter", include_bytes!("../assets/fonts/Inter-Regular.otf"));
    add(&mut fonts, "inter-medium", include_bytes!("../assets/fonts/Inter-Medium.otf"));
    add(&mut fonts, "inter-semibold", include_bytes!("../assets/fonts/Inter-SemiBold.otf"));
    fonts.font_data.insert("ph".into(), Arc::new(egui_phosphor::Variant::Regular.font_data()));
    fonts.font_data.insert("phf".into(), Arc::new(egui_phosphor::Variant::Fill.font_data()));

    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let fam = |first: &str| {
        let mut v = vec![first.to_string(), "ph".to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, fam("inter"));
    fonts.families.insert(FontFamily::Name("medium".into()), fam("inter-medium"));
    fonts.families.insert(FontFamily::Name("semibold".into()), fam("inter-semibold"));
    fonts.families.insert(FontFamily::Name("ph".into()), vec!["ph".into()]);
    fonts.families.insert(FontFamily::Name("phf".into()), vec!["phf".into()]);
    ctx.set_fonts(fonts);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 4.0);
        s.spacing.menu_margin = egui::Margin::same(5);
        s.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 4.0,
            floating_allocated_width: 0.0,
            dormant_background_opacity: 0.0,
            dormant_handle_opacity: 0.0,
            active_background_opacity: 0.0,
            interact_background_opacity: 0.0,
            active_handle_opacity: 0.3,
            interact_handle_opacity: 0.5,
            ..egui::style::ScrollStyle::floating()
        };
        s.text_styles.insert(egui::TextStyle::Body, regular(13.0));
        s.text_styles.insert(egui::TextStyle::Button, regular(13.0));
        s.text_styles.insert(egui::TextStyle::Small, regular(11.0));
        s.interaction.selectable_labels = false;
    });
}

/// Applique le thème clair ou sombre.
pub fn set_theme(ctx: &egui::Context, dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
    let p = pal();
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = if dark { rgb(0x24, 0x24, 0x27) } else { Color32::WHITE };
    v.extreme_bg_color = v.window_fill;
    v.faint_bg_color = p.hover;
    v.override_text_color = Some(p.text);
    v.selection.bg_fill = p.track;
    v.selection.stroke = Stroke::new(1.0, p.ink);
    v.text_cursor.stroke = Stroke::new(1.5, p.ink);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_stroke = Stroke::new(1.0, p.border);
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 6], blur: 20, spread: 0, color: p.shadow };
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(7);
        w.bg_stroke = Stroke::new(1.0, p.line);
    }
    v.widgets.inactive.weak_bg_fill = v.window_fill;
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.open.weak_bg_fill = p.hover;
    ctx.set_visuals(v);
}

// ---------------------------------------------------------------- texte & icônes

/// Texte sur une ligne, tronqué à `max_w`, ancré selon `align`.
pub fn text(ui: &Ui, pos: Pos2, align: Align2, s: &str, font: FontId, color: Color32, max_w: f32) -> Rect {
    let mut job = LayoutJob::simple_singleline(s.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(1.0));
    let galley = ui.fonts(|f| f.layout_job(job));
    let rect = align.anchor_size(pos, galley.size());
    ui.painter().galley(rect.min, galley, color);
    rect
}

pub fn text_width(ui: &Ui, s: &str, font: FontId) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(s.to_owned(), font, Color32::WHITE).size().x)
}

/// Icône centrée sur son tracé réel (et non sur la boîte de la police).
pub fn paint_icon(ui: &Ui, center: Pos2, glyph: &str, font: FontId, color: Color32) {
    let galley = ui.fonts(|f| f.layout_no_wrap(glyph.to_owned(), font, color));
    let ink = galley
        .rows
        .first()
        .and_then(|r| r.glyphs.first())
        .map(|g| Rect::from_min_size(g.pos + g.uv_rect.offset, g.uv_rect.size))
        .filter(|r| r.width() > 0.0);
    let origin = match ink {
        Some(r) => center - r.center().to_vec2(),
        None => center - galley.size() / 2.0,
    };
    ui.painter().galley(origin, galley, color);
}

// ---------------------------------------------------------------- widgets

fn hand(r: Response, enabled: bool) -> Response {
    if enabled { r.on_hover_cursor(egui::CursorIcon::PointingHand) } else { r }
}

/// Bouton-icône discret : fond léger au survol.
pub fn icon_button(ui: &mut Ui, id: &str, center: Pos2, size: f32, glyph: &str, font: FontId, color: Color32, enabled: bool) -> Response {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    let r = ui.interact(rect, ui.id().with(id), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && r.hovered() {
        ui.painter().rect_filled(rect, size * 0.3, pal().hover);
    }
    paint_icon(ui, center, glyph, font, if enabled { color } else { pal().faint });
    hand(r, enabled)
}

// Couleurs des pastilles de fenêtre (identiques dans les deux thèmes)
pub const DOT_CLOSE: Color32 = rgb(0x8B, 0xA3, 0x86);
pub const DOT_MIN: Color32 = rgb(0xE0, 0xBE, 0x98);
pub const DOT_MINI: Color32 = rgb(0xD3, 0xD2, 0xCE);

/// Pastille de fenêtre ; le symbole apparaît au survol de la zone des pastilles.
pub fn window_dot(ui: &mut Ui, id: &str, center: Pos2, color: Color32, glyph: &str, show_glyph: bool) -> Response {
    let r = ui.interact(Rect::from_center_size(center, Vec2::splat(16.0)), ui.id().with(id), Sense::click());
    ui.painter().circle_filled(center, 5.5, color);
    if show_glyph {
        paint_icon(ui, center, glyph, ph(8.0), Color32::from_black_alpha(150));
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Gros bouton lecture/pause.
pub fn play_button(ui: &mut Ui, center: Pos2, radius: f32, playing: bool, enabled: bool) -> Response {
    let rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    let r = ui.interact(rect, ui.id().with("play"), if enabled { Sense::click() } else { Sense::hover() });
    let p = pal();
    let k = if enabled && r.is_pointer_button_down_on() { 0.94 } else { 1.0 };
    let fill = if !enabled { p.track } else if r.hovered() { p.ink.gamma_multiply(0.88) } else { p.ink };
    ui.painter().circle_filled(center, radius * k, fill);
    // Le triangle est recentré sur son barycentre optique.
    let (glyph, dx) = if playing { (icon::PAUSE, 0.0) } else { (icon::PLAY, radius * 0.07) };
    paint_icon(ui, center + Vec2::new(dx, 0.0), glyph, phf(radius * 0.82), p.on_ink);
    hand(r, enabled)
}

/// Barre fine avec pastille. Renvoie la fraction pointée pendant l'interaction.
pub fn bar(ui: &mut Ui, id: &str, rect: Rect, frac: f32, enabled: bool, knob: f32) -> Option<f32> {
    let r = ui.interact(rect, ui.id().with(id), if enabled { Sense::click_and_drag() } else { Sense::hover() });
    let p = pal();
    let active = enabled && (r.hovered() || r.is_pointer_button_down_on());
    let h = 2.5;
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), h));
    let painter = ui.painter();
    painter.rect_filled(track, h / 2.0, p.track);
    let frac = frac.clamp(0.0, 1.0);
    let x = track.left() + track.width() * frac;
    if enabled {
        painter.rect_filled(Rect::from_min_max(track.min, Pos2::new(x, track.max.y)), h / 2.0, p.ink);
        painter.circle_filled(Pos2::new(x, track.center().y), if active { knob + 1.0 } else { knob }, p.ink);
    }
    let pointed = (enabled && r.is_pointer_button_down_on())
        .then(|| r.interact_pointer_pos())
        .flatten()
        .map(|pos| ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
    hand(r, enabled);
    pointed
}

/// Badge de format (MP3, FLAC…) : contour fin. Renvoie sa largeur.
pub fn badge(ui: &Ui, left_center: Pos2, label: &str) -> f32 {
    let font = medium(8.5);
    // Largeur fixe : les titres restent alignés quel que soit le format.
    let w = (text_width(ui, label, font.clone()) + 9.0).max(34.0);
    let r = Rect::from_min_size(Pos2::new(left_center.x, left_center.y - 7.5), Vec2::new(w, 15.0));
    ui.painter().rect_stroke(r, 3, Stroke::new(1.0, pal().faint), egui::StrokeKind::Inside);
    text(ui, r.center() + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, label, font, pal().dim, w);
    w
}

/// Onglets texte soulignés. Renvoie (index cliqué, abscisse de fin des onglets).
pub fn tabs(ui: &mut Ui, rect: Rect, labels: &[&str], selected: usize) -> (Option<usize>, f32) {
    let p = pal();
    let mut x = rect.left();
    let mut clicked = None;
    for (i, label) in labels.iter().enumerate() {
        let sel = i == selected;
        let font = if sel { medium(12.5) } else { regular(12.5) };
        let w = text_width(ui, label, font.clone());
        let hit = Rect::from_min_max(Pos2::new(x - 4.0, rect.top()), Pos2::new(x + w + 4.0, rect.bottom()));
        let r = ui.interact(hit, ui.id().with(("tab", i)), Sense::click());
        let col = if sel { p.text } else if r.hovered() { p.soft } else { p.dim };
        text(ui, Pos2::new(x, rect.center().y - 1.0), Align2::LEFT_CENTER, label, font, col, w + 2.0);
        if sel {
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(x, rect.bottom() - 2.0), Vec2::new(w, 2.0)), 1, p.ink);
        }
        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            clicked = Some(i);
        }
        x += w + 17.0;
    }
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.line));
    (clicked, x - 17.0)
}

/// Vignette carrée arrondie : texture ou tuile neutre avec icône.
pub fn thumb(ui: &Ui, rect: Rect, radius: u8, tex: Option<&egui::TextureHandle>, glyph: &str) {
    if let Some(t) = tex {
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        ui.painter().add(egui::epaint::RectShape::filled(rect, radius, Color32::WHITE).with_texture(t.id(), uv));
    } else {
        ui.painter().rect_filled(rect, radius, pal().tile);
        paint_icon(ui, rect.center(), glyph, ph(rect.width() * 0.44), pal().faint);
    }
}

pub const ROW_H: f32 = 48.0;
/// Espace réservé à droite des listes pour la barre de défilement.
pub const GUTTER: f32 = 12.0;

/// Ligne de liste : vignette · badge · titre · info à droite, séparateur fin.
pub fn list_row(
    ui: &mut Ui,
    tex: Option<&egui::TextureHandle>,
    glyph: &str,
    badge_label: Option<&str>,
    title: &str,
    right: &str,
    current: bool,
    separator: bool,
) -> Response {
    list_row_ex(ui, tex, glyph, badge_label, title, right, current, separator, None).0
}

/// Bouton d'action de ligne : (icône, actif, info-bulle).
pub type RowAction<'a> = Option<(&'a str, bool, &'a str)>;

/// Ligne de liste avec bouton d'action à droite (visible au survol, ou en permanence s'il est actif).
/// Renvoie (réponse de la ligne, action cliquée).
pub fn list_row_ex(
    ui: &mut Ui,
    tex: Option<&egui::TextureHandle>,
    glyph: &str,
    badge_label: Option<&str>,
    title: &str,
    right: &str,
    current: bool,
    separator: bool,
    action: RowAction,
) -> (Response, bool) {
    let p = pal();
    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width() - GUTTER, ROW_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return (r, false);
    }
    let hovered = ui.rect_contains_pointer(rect);
    if hovered {
        ui.painter().rect_filled(rect.expand2(Vec2::new(6.0, -2.0)), 8, p.hover);
    }
    let cy = rect.center().y;
    let t = Rect::from_min_size(Pos2::new(rect.left(), cy - 16.0), Vec2::splat(32.0));
    thumb(ui, t, 6, tex, glyph);

    let mut x = t.right() + 14.0;
    let sep_x = x;
    if let Some(b) = badge_label.filter(|b| !b.is_empty()) {
        x += badge(ui, Pos2::new(x, cy), b) + 10.0;
    }

    // Bouton d'action (⊕ / ✓) : remplace l'info de droite quand il est visible.
    let mut clicked = false;
    let show_action = action.is_some_and(|(_, active, _)| hovered || active);
    let right_w = if let (true, Some((g, active, tip))) = (show_action, action) {
        let c = Pos2::new(rect.right() - 10.0, cy);
        let br = Rect::from_center_size(c, Vec2::splat(26.0));
        let a = ui.interact(br, ui.id().with(("row_action", rect.top() as i32)), Sense::click());
        let col = if active { p.accent } else if a.hovered() { p.text } else { p.dim };
        let font = if active { phf(18.0) } else { ph(18.0) };
        paint_icon(ui, c, g, font, col);
        clicked = a.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        30.0
    } else if right.is_empty() {
        0.0
    } else {
        let col = if current { p.accent } else { p.faint };
        text(ui, Pos2::new(rect.right(), cy), Align2::RIGHT_CENTER, right, regular(11.5), col, 80.0).width() + 12.0
    };
    let (font, col) = if current { (medium(13.0), p.text) } else { (regular(13.0), if hovered { p.text } else { p.soft }) };
    text(ui, Pos2::new(x, cy), Align2::LEFT_CENTER, title, font, col, rect.right() - right_w - x);
    if separator {
        ui.painter().hline(sep_x..=rect.right(), rect.bottom() - 0.5, Stroke::new(1.0, p.line));
    }
    (hand(r, true), clicked)
}

/// Ligne de la file d'attente : glisser pour réordonner, ✕ pour retirer.
/// Renvoie (réponse de la ligne, retirer cliqué).
pub fn queue_row(
    ui: &mut Ui,
    tex: Option<&egui::TextureHandle>,
    badge_label: &str,
    title: &str,
    right: &str,
    dragging: bool,
    separator: bool,
) -> (Response, bool) {
    let p = pal();
    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width() - GUTTER, ROW_H), Sense::click_and_drag());
    if !ui.is_rect_visible(rect) {
        return (r, false);
    }
    let hovered = ui.rect_contains_pointer(rect) || dragging;
    if dragging {
        ui.painter().rect(rect.expand2(Vec2::new(6.0, -2.0)), 8, p.hover, Stroke::new(1.0, p.track), egui::StrokeKind::Inside);
    } else if hovered {
        ui.painter().rect_filled(rect.expand2(Vec2::new(6.0, -2.0)), 8, p.hover);
    }
    let cy = rect.center().y;
    let t = Rect::from_min_size(Pos2::new(rect.left(), cy - 16.0), Vec2::splat(32.0));
    thumb(ui, t, 6, tex, icon::MUSIC_NOTE);
    let mut x = t.right() + 14.0;
    let sep_x = x;
    x += badge(ui, Pos2::new(x, cy), badge_label) + 10.0;

    let mut removed = false;
    let right_w = if hovered {
        // Poignée de déplacement + bouton retirer
        paint_icon(ui, Pos2::new(rect.right() - 40.0, cy), icon::DOTS_SIX_VERTICAL, ph(16.0), p.dim);
        let c = Pos2::new(rect.right() - 10.0, cy);
        let a = ui.interact(Rect::from_center_size(c, Vec2::splat(26.0)), ui.id().with(("q_rm", rect.top() as i32)), Sense::click());
        paint_icon(ui, c, icon::X, ph(15.0), if a.hovered() { p.text } else { p.dim });
        removed = a.on_hover_text("Retirer de la file").on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        58.0
    } else if right.is_empty() {
        0.0
    } else {
        text(ui, Pos2::new(rect.right(), cy), Align2::RIGHT_CENTER, right, regular(11.5), p.faint, 80.0).width() + 12.0
    };
    let col = if hovered { p.text } else { p.soft };
    text(ui, Pos2::new(x, cy), Align2::LEFT_CENTER, title, regular(13.0), col, rect.right() - right_w - x);
    if separator {
        ui.painter().hline(sep_x..=rect.right(), rect.bottom() - 0.5, Stroke::new(1.0, p.line));
    }
    let cursor = if dragging { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab };
    (r.on_hover_cursor(cursor), removed)
}

/// Barre de recherche arrondie : loupe · champ · effacer. Renvoie (champ, effacer cliqué).
pub fn search_bar(ui: &mut Ui, rect: Rect, buf: &mut String, hint: &str, focus: bool) -> (Response, bool) {
    let p = pal();
    let id = ui.id().with("search_field");
    let focused = ui.memory(|m| m.has_focus(id));
    let stroke = if focused { Stroke::new(1.0, p.faint) } else { Stroke::new(1.0, p.line) };
    ui.painter().rect(rect, 10, p.tile, stroke, egui::StrokeKind::Inside);
    paint_icon(ui, Pos2::new(rect.left() + 16.0, rect.center().y), icon::MAGNIFYING_GLASS, ph(15.0), if focused { p.soft } else { p.dim });

    let has_text = !buf.is_empty();
    let field = Rect::from_min_max(
        Pos2::new(rect.left() + 32.0, rect.center().y - 9.0),
        Pos2::new(rect.right() - if has_text { 32.0 } else { 10.0 }, rect.center().y + 9.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
    let r = child.add(
        egui::TextEdit::singleline(buf)
            .id(id)
            .frame(false)
            .hint_text(egui::RichText::new(hint).color(p.faint))
            .font(regular(13.0))
            .text_color(p.text)
            .margin(egui::Margin::ZERO)
            .desired_width(field.width()),
    );
    if focus {
        r.request_focus();
    }
    let mut cleared = false;
    if has_text {
        let c = Pos2::new(rect.right() - 16.0, rect.center().y);
        let b = ui.interact(Rect::from_center_size(c, Vec2::splat(22.0)), ui.id().with("search_clear"), Sense::click());
        ui.painter().circle_filled(c, 8.0, if b.hovered() { p.dim } else { p.faint });
        paint_icon(ui, c, icon::X, ph(9.0), p.card);
        cleared = b.on_hover_text("Effacer la recherche").on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    }
    (r, cleared)
}

/// Rangée d'outils en tête de liste : intitulé à gauche, lien d'action à droite.
pub fn list_toolbar(ui: &mut Ui, id: &str, label: &str, action: Option<(&str, &str)>, danger: bool) -> bool {
    let p = pal();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width() - GUTTER, 30.0), Sense::hover());
    let cy = rect.center().y + 2.0;
    text(ui, Pos2::new(rect.left(), cy), Align2::LEFT_CENTER, label, semibold(10.5), p.dim, rect.width() * 0.6);
    let Some((glyph, link_label)) = action else { return false };
    let font = regular(11.5);
    let tw = text_width(ui, link_label, font.clone());
    let iw = if glyph.is_empty() { 0.0 } else { 17.0 };
    let hit = Rect::from_min_max(Pos2::new(rect.right() - tw - iw - 6.0, cy - 11.0), Pos2::new(rect.right() + 4.0, cy + 11.0));
    let r = ui.interact(hit, ui.id().with(id), Sense::click());
    let col = match (danger, r.hovered()) {
        (true, _) => rgb(0xC2, 0x5B, 0x4E),
        (false, true) => p.text,
        (false, false) => p.dim,
    };
    if !glyph.is_empty() {
        paint_icon(ui, Pos2::new(rect.right() - tw - 9.0, cy), glyph, ph(13.0), col);
    }
    text(ui, Pos2::new(rect.right(), cy), Align2::RIGHT_CENTER, link_label, font, col, tw + 2.0);
    r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Minuscules sans accents, pour une recherche tolérante.
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            _ => c,
        })
        .collect()
}

/// Petit intitulé de section (EN COURS, ENSUITE…).
pub fn section_label(ui: &mut Ui, label: &str) -> Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width() - GUTTER, 30.0), Sense::hover());
    text(ui, Pos2::new(rect.left(), rect.center().y + 3.0), Align2::LEFT_CENTER, label, semibold(10.5), pal().dim, rect.width());
    rect
}

/// Élément de menu contextuel.
pub fn menu_item(ui: &mut Ui, glyph: &str, label: &str) -> Response {
    let p = pal();
    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(190.0), 30.0), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, 6, p.hover);
    }
    paint_icon(ui, Pos2::new(rect.left() + 16.0, rect.center().y), glyph, ph(15.0), p.soft);
    text(ui, Pos2::new(rect.left() + 32.0, rect.center().y), Align2::LEFT_CENTER, label, regular(12.5), p.text, rect.width() - 40.0);
    hand(r, true)
}

pub fn menu_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 7.0), Sense::hover());
    ui.painter().hline(rect.x_range().shrink(4.0), rect.center().y, Stroke::new(1.0, pal().line));
}

/// Petit lien texte (« Effacer », « Ouvrir un dossier »…), ancré à droite.
pub fn link(ui: &mut Ui, id: &str, right_center: Pos2, label: &str, glyph: Option<&str>) -> Response {
    let p = pal();
    let font = regular(11.5);
    let tw = text_width(ui, label, font.clone());
    let iw = if glyph.is_some() { 18.0 } else { 0.0 };
    let rect = Rect::from_min_max(
        Pos2::new(right_center.x - tw - iw - 6.0, right_center.y - 11.0),
        Pos2::new(right_center.x + 6.0, right_center.y + 11.0),
    );
    let r = ui.interact(rect, ui.id().with(id), Sense::click());
    let col = if r.hovered() { p.text } else { p.dim };
    if let Some(g) = glyph {
        paint_icon(ui, Pos2::new(right_center.x - tw - 9.0, right_center.y), g, ph(13.0), col);
    }
    text(ui, right_center, Align2::RIGHT_CENTER, label, font, col, tw + 2.0);
    hand(r, true)
}

/// Bouton secondaire à contour.
pub fn outline_button(ui: &mut Ui, id: &str, center: Pos2, label: &str) -> Response {
    let p = pal();
    let font = medium(12.0);
    let w = text_width(ui, label, font.clone()) + 28.0;
    let rect = Rect::from_center_size(center, Vec2::new(w, 30.0));
    let r = ui.interact(rect, ui.id().with(id), Sense::click());
    let fill = if r.hovered() { p.hover } else { Color32::TRANSPARENT };
    ui.painter().rect(rect, 15, fill, Stroke::new(1.0, p.track), egui::StrokeKind::Inside);
    text(ui, rect.center(), Align2::CENTER_CENTER, label, font, p.text, w);
    hand(r, true)
}

/// Champ texte au style de la carte.
pub fn text_field(ui: &mut Ui, buf: &mut String, hint: &str) -> Response {
    let w = ui.available_width() - GUTTER;
    ui.add(
        egui::TextEdit::singleline(buf)
            .hint_text(egui::RichText::new(hint).color(pal().faint))
            .font(regular(13.0))
            .text_color(pal().text)
            .margin(egui::Margin::symmetric(10, 7))
            .desired_width(w),
    )
}
