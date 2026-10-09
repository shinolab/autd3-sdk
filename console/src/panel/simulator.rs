use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::launch::open_url;
use crate::process::ManagedProcess;

const BIN: &str = "autd3-rs-simulator";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulatorConfig {
    pub http_port: u16,
    pub geometry: String,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            http_port: 8081,
            geometry: String::new(),
        }
    }
}

impl SimulatorConfig {
    fn args(&self) -> Vec<String> {
        let mut args = vec!["--http-port".to_string(), self.http_port.to_string()];
        if !self.geometry.trim().is_empty() {
            args.push("--geometry".to_string());
            args.push(self.geometry.trim().to_string());
        }
        args
    }
}

#[derive(Default)]
pub struct SimulatorPanel {
    pub config: SimulatorConfig,
    proc: Option<ManagedProcess>,
    error: Option<String>,
}

impl SimulatorPanel {
    pub fn pump(&mut self) {
        if let Some(proc) = &mut self.proc {
            proc.pump();
        }
    }

    pub fn is_running(&self) -> bool {
        self.proc.as_ref().is_some_and(ManagedProcess::is_running)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let running = self.is_running();

        egui::Grid::new("simulator-config")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("HTTP port");
                ui.add_enabled(
                    !running,
                    egui::DragValue::new(&mut self.config.http_port).range(1..=65535),
                );
                ui.end_row();

                ui.label("Geometry JSON");
                ui.add_enabled(
                    !running,
                    egui::TextEdit::singleline(&mut self.config.geometry)
                        .hint_text("one AUTD3 at the origin"),
                );
                ui.end_row();
            });

        ui.separator();

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new("Start"))
                .clicked()
            {
                self.start();
            }
            if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
                self.proc = None;
            }
            if ui.button("Open browser").clicked() {
                self.open_browser();
            }
            ui.label(if running { "running" } else { "stopped" });
        });

        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }

        ui.separator();
        super::log_view(ui, self.proc.as_ref());
    }

    fn start(&mut self) {
        self.error = None;
        match super::spawn_tool(BIN, &self.config.args()) {
            Ok(proc) => self.proc = Some(proc),
            Err(e) => self.error = Some(e),
        }
    }

    fn open_browser(&mut self) {
        let url = format!("http://127.0.0.1:{}", self.config.http_port);
        if let Err(e) = open_url(&url) {
            self.error = Some(format!("failed to open browser: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_config_serves_one_device() {
        assert_eq!(SimulatorConfig::default().args(), ["--http-port", "8081"]);
    }

    #[test]
    fn a_geometry_file_is_forwarded() {
        let config = SimulatorConfig {
            geometry: " geometry.json ".to_string(),
            ..SimulatorConfig::default()
        };
        assert_eq!(
            config.args()[2..],
            ["--geometry".to_string(), "geometry.json".to_string()]
        );
    }

    #[test]
    fn a_config_saved_with_a_removed_field_still_loads() {
        let config: SimulatorConfig =
            serde_json::from_str(r#"{"http_port":9000,"link_port":8080,"group":"[::1]:44336"}"#)
                .unwrap();
        assert_eq!(config.http_port, 9000);
    }
}
