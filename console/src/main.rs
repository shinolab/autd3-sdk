#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod launch;
mod panel;
mod process;

use eframe::egui;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([760.0, 560.0])
        .with_min_inner_size([560.0, 400.0])
        .with_title("AUTD3 Console");
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "autd3-console",
        options,
        Box::new(|cc| Ok(Box::new(app::ConsoleApp::new(cc)))),
    )
}
