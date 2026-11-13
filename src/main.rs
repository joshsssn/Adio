#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod media_keys;
mod meta;
mod player;
mod queue;
mod storage;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let arg = std::env::args_os().nth(1).map(PathBuf::from).filter(|p| p.exists());

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Adio")
        .with_inner_size([360.0, 560.0])
        .with_min_inner_size([300.0, 300.0])
        .with_drag_and_drop(true);
    if let Ok(img) = image::load_from_memory(include_bytes!("../assets/icon-256.png")) {
        let img = img.to_rgba8();
        viewport = viewport.with_icon(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() });
    }

    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("Adio", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, arg)))))
}
