#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fs;
mod model;
mod shortcuts;
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([760.0, 480.0])
            .with_title("Tabex")
            .with_app_id("tabex"),
        ..Default::default()
    };
    eframe::run_native("Tabex", options, Box::new(|cc| Ok(Box::new(app::TabexApp::new(cc)))))
}
