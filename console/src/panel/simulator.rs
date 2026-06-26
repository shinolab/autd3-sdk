use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::launch::{tool_bin, tool_path};
use crate::process::ManagedProcess;

const SUBDIR: &str = "simulator";
const BIN: &str = "autd3-rs-simulator";

#[derive(Clone, Serialize, Deserialize)]
pub struct SimulatorConfig {
    pub http_port: u16,
    pub link_port: u16,
    pub devices: usize,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            http_port: 8081,
            link_port: 8080,
            devices: 1,
        }
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
            if !proc.is_running() {
                self.proc = None;
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.proc.is_some()
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

                ui.label("Link port");
                ui.add_enabled(
                    !running,
                    egui::DragValue::new(&mut self.config.link_port).range(1..=65535),
                );
                ui.end_row();

                ui.label("Devices");
                ui.add_enabled(
                    !running,
                    egui::DragValue::new(&mut self.config.devices).range(1..=256),
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
        let web = tool_path(SUBDIR, "web").unwrap_or_default();
        let args = vec![
            "--http-port".to_string(),
            self.config.http_port.to_string(),
            "--link-port".to_string(),
            self.config.link_port.to_string(),
            "--devices".to_string(),
            self.config.devices.to_string(),
            "--web-dir".to_string(),
            web.to_string_lossy().into_owned(),
        ];
        match ManagedProcess::spawn(&bin, &args) {
            Ok(proc) => self.proc = Some(proc),
            Err(e) => self.error = Some(format!("failed to start {BIN}: {e}")),
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
    std::process::Command::new(program).args(args).spawn()?;
    Ok(())
}
