use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::launch::tool_bin;
use crate::process::ManagedProcess;

const SUBDIR: &str = "simulator";
const BIN: &str = "autd3-rs-simulator";

const DEFAULT_GROUP: &str = "[::1]:44336";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulatorConfig {
    pub http_port: u16,
    pub group: String,
    pub geometry: String,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            http_port: 8081,
            group: DEFAULT_GROUP.to_string(),
            geometry: String::new(),
        }
    }
}

impl SimulatorConfig {
    fn args(&self) -> Vec<String> {
        let mut args = vec![
            "--http-port".to_string(),
            self.http_port.to_string(),
            "--group".to_string(),
            self.group.clone(),
        ];
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

                ui.label("Group address");
                ui.add_enabled(!running, egui::TextEdit::singleline(&mut self.config.group));
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
                self.stop();
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
        let bin = match tool_bin(SUBDIR, BIN) {
            Ok(bin) => bin,
            Err(e) => {
                self.error = Some(format!("cannot resolve {BIN}: {e}"));
                return;
            }
        };
        let args = self.config.args();
        match ManagedProcess::spawn(&bin, &args) {
            Ok(proc) => self.proc = Some(proc),
            Err(e) => self.error = Some(super::spawn_error(&bin, &e)),
        }
    }

    fn stop(&mut self) {
        if let Some(proc) = &mut self.proc {
            proc.kill();
        }
        self.proc = None;
    }

    fn open_browser(&mut self) {
        let url = format!("http://127.0.0.1:{}", self.config.http_port);
        if let Err(e) = open_url(&url) {
            self.error = Some(format!("failed to open browser: {e}"));
        }
    }
}

fn open_url(url: &str) -> std::io::Result<()> {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[url])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/C", "start", "", url])
    } else {
        ("xdg-open", &[url])
    };
    let mut command = std::process::Command::new(program);
    command.args(args);
    crate::process::no_window(&mut command);
    command.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_config_serves_the_default_group_with_one_device() {
        assert_eq!(
            SimulatorConfig::default().args(),
            ["--http-port", "8081", "--group", "[::1]:44336"]
        );
    }

    #[test]
    fn a_geometry_file_is_forwarded() {
        let config = SimulatorConfig {
            geometry: " geometry.json ".to_string(),
            ..SimulatorConfig::default()
        };
        assert_eq!(
            config.args()[4..],
            ["--geometry".to_string(), "geometry.json".to_string()]
        );
    }

    #[test]
    fn a_config_saved_with_a_link_port_still_loads() {
        let config: SimulatorConfig =
            serde_json::from_str(r#"{"http_port":9000,"link_port":8080}"#).unwrap();
        assert_eq!(config.http_port, 9000);
        assert_eq!(config.group, DEFAULT_GROUP);
    }
}
