mod simulator;
mod twincat;

pub use simulator::SimulatorPanel;
pub use twincat::TwinCatPanel;

use eframe::egui;

use crate::process::ManagedProcess;

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
                ui.weak("not running");
            }
        });
}
