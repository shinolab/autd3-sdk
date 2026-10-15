use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::process::ManagedProcess;

const BIN: &str = "autd3-simulator";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulatorConfig {
    pub port: u16,
    pub http_port: u16,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            port: 44336,
            http_port: 8081,
        }
    }
}

impl SimulatorConfig {
    fn args(&self) -> Vec<String> {
        vec![
            "--port".to_string(),
            self.port.to_string(),
            "--http-port".to_string(),
            self.http_port.to_string(),
        ]
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
                ui.label("UDP port");
                ui.add_enabled(
                    !running,
                    egui::DragValue::new(&mut self.config.port).range(1..=65534),
                );
                ui.end_row();
                ui.label("HTTP port");
                ui.add_enabled(
                    !running,
                    egui::DragValue::new(&mut self.config.http_port).range(1..=65535),
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
                let url = format!("http://127.0.0.1:{}", self.config.http_port);
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_names_the_udp_and_http_ports() {
        assert_eq!(
            SimulatorConfig::default().args(),
            ["--port", "44336", "--http-port", "8081"]
        );
    }

    #[test]
    fn a_config_saved_with_a_removed_field_still_loads() {
        let config: SimulatorConfig = serde_json::from_str(
            r#"{"http_port":9000,"link_port":8080,"group":"[::1]:44336","geometry":"g.json"}"#,
        )
        .unwrap();
        assert_eq!(config.http_port, 9000);
        assert_eq!(config.port, 44336);
    }
}
