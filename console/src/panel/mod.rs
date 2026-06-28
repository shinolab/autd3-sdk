mod simulator;
mod twincat;

pub use simulator::SimulatorPanel;
pub use twincat::TwinCatPanel;

use std::io;
use std::path::Path;

use eframe::egui;

use crate::process::ManagedProcess;

fn spawn_error(bin: &Path, e: &io::Error) -> String {
    let os = e
        .raw_os_error()
        .map_or_else(String::new, |code| format!(" (os error {code})"));
    format!("failed to start {}: {}{os}", bin.display(), e.kind())
}

fn log_view(ui: &mut egui::Ui, proc: Option<&ManagedProcess>) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| match proc {
            Some(proc) => {
                for line in proc.logs() {
                    ui.monospace(line);
                }
            }
            None => {
                ui.weak("no output yet");
            }
        });
}
