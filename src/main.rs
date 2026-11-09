#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tabex::app;

fn main() -> eframe::Result {
    tabex::log::init_panic_hook();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([760.0, 480.0])
            .with_title("Tabex")
            .with_app_id("tabex")
            .with_icon(std::sync::Arc::new(egui::IconData {
                rgba: include_bytes!("../assets/icon-64.rgba").to_vec(),
                width: 64,
                height: 64,
            })),
        ..Default::default()
    };
    eframe::run_native("Tabex", options, Box::new(|cc| Ok(Box::new(app::TabexApp::new(cc)))))
}
