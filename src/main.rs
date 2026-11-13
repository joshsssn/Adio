#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod media_keys;
mod meta;
mod player;
mod queue;
mod storage;
mod ui;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let arg = std::env::args_os().nth(1).map(PathBuf::from).filter(|p| p.exists());

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Adio")
        .with_inner_size([400.0, 700.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_min_inner_size([280.0, 260.0])
        .with_drag_and_drop(true);
    if let Ok(img) = image::load_from_memory(include_bytes!("../assets/icon-256.png")) {
        let img = img.to_rgba8();
        viewport = viewport.with_icon(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() });
    }

    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("Adio", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, arg)))))
}
