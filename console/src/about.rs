use eframe::egui;

pub(crate) fn ui(ui: &mut egui::Ui, updater: &mut crate::update::Updater) {
    ui.add_space(4.0);
    ui.heading("AUTD3 Console");
    ui.label(format!("version {}", env!("CARGO_PKG_VERSION")));
    ui.hyperlink("https://github.com/shinolab/autd3-sdk");
    ui.separator();

    updater.settings_ui(ui);
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.collapsing("License (MIT)", |ui| {
                ui.monospace(include_str!(concat!(env!("OUT_DIR"), "/license.txt")));
            });
            ui.collapsing("Third-party licenses", |ui| {
                ui.monospace(include_str!(concat!(env!("OUT_DIR"), "/third-party.md")));
            });
        });
}
