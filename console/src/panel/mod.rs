mod firmware;
mod simulator;

pub use firmware::FirmwarePanel;
pub use simulator::SimulatorPanel;

use std::io;
use std::path::Path;

use eframe::egui;

use crate::launch::tool_bin;
use crate::process::ManagedProcess;

fn spawn_tool(name: &str, args: &[String]) -> Result<ManagedProcess, String> {
    let bin = tool_bin(name).map_err(|e| format!("cannot resolve {name}: {e}"))?;
    ManagedProcess::spawn(&bin, args).map_err(|e| spawn_error(&bin, &e))
}

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
