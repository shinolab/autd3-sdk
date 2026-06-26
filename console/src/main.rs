#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod launch;
mod panel;
mod process;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([560.0, 400.0])
            .with_title("AUTD3 Console"),
        ..Default::default()
    };
    eframe::run_native(
        "autd3-console",
        options,
        Box::new(|cc| Ok(Box::new(app::ConsoleApp::new(cc)))),
    )
}
