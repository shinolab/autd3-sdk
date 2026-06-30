use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::launch::tool_bin;
use crate::process::ManagedProcess;

const SUBDIR: &str = "firmware";
const BIN: &str = "autd3-firmware";

#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    #[default]
    Both,
    Fpga,
    Cpu,
}

impl Target {
    fn arg(self) -> &'static str {
        match self {
            Target::Both => "both",
            Target::Fpga => "fpga",
            Target::Cpu => "cpu",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Target::Both => "Both",
            Target::Fpga => "FPGA",
            Target::Cpu => "CPU",
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct FirmwareConfig {
    pub version: Option<String>,
    pub target: Target,
}

#[derive(Default)]
pub struct FirmwarePanel {
    pub config: FirmwareConfig,
    proc: Option<ManagedProcess>,
    list_proc: Option<ManagedProcess>,
    versions: Vec<String>,
    listed: bool,
    error: Option<String>,
}

impl FirmwarePanel {
    pub fn pump(&mut self) {
        if let Some(proc) = &mut self.proc {
            proc.pump();
        }
        if let Some(list) = &mut self.list_proc {
            list.pump();
            if !list.is_running() {
                self.versions = list
                    .logs()
                    .iter()
                    .filter(|l| l.as_bytes().first().is_some_and(u8::is_ascii_digit))
                    .cloned()
                    .collect();
                if self.config.version.is_none() {
                    self.config.version = self.versions.first().cloned();
                }
                self.list_proc = None;
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.proc.as_ref().is_some_and(ManagedProcess::is_running)
            || self
                .list_proc
                .as_ref()
                .is_some_and(ManagedProcess::is_running)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if !self.listed {
            self.refresh_versions();
            self.listed = true;
        }

        let flashing = self.proc.as_ref().is_some_and(ManagedProcess::is_running);
        let listing = self.list_proc.is_some();
        let busy = flashing || listing;

        egui::Grid::new("firmware-config")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Version");
                ui.horizontal(|ui| {
                    let selected = self
                        .config
                        .version
                        .clone()
                        .unwrap_or_else(|| "(select)".to_string());
                    ui.add_enabled_ui(!busy, |ui| {
                        egui::ComboBox::from_id_salt("firmware-version")
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                for v in &self.versions {
                                    ui.selectable_value(
                                        &mut self.config.version,
                                        Some(v.clone()),
                                        v,
                                    );
                                }
                            });
                    });
                    if ui
                        .add_enabled(!busy, egui::Button::new("Refresh"))
                        .clicked()
                    {
                        self.refresh_versions();
                    }
                });
                ui.end_row();

                ui.label("Target");
                ui.add_enabled_ui(!flashing, |ui| {
                    ui.horizontal(|ui| {
                        for t in [Target::Both, Target::Fpga, Target::Cpu] {
                            ui.selectable_value(&mut self.config.target, t, t.label());
                        }
                    });
                });
                ui.end_row();
            });

        ui.separator();
        ui.weak(
            "Flashing requires SEGGER J-Link (CPU) and Xilinx Vivado / vivado_lab (FPGA) \
             installed and on PATH. Connect the configuration cable and power on the AUTD3.",
        );
        ui.separator();

        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !flashing && self.config.version.is_some(),
                    egui::Button::new("Flash"),
                )
                .clicked()
            {
                self.start();
            }
            if ui
                .add_enabled(flashing, egui::Button::new("Stop"))
                .clicked()
            {
                self.stop();
            }
            ui.label(if listing {
                "fetching versions..."
            } else if flashing {
                "flashing"
            } else {
                "idle"
            });
        });

        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }

        ui.separator();
        super::log_view(ui, self.proc.as_ref());
    }

    fn refresh_versions(&mut self) {
        self.error = None;
        let bin = match tool_bin(SUBDIR, BIN) {
            Ok(bin) => bin,
            Err(e) => {
                self.error = Some(format!("cannot resolve {BIN}: {e}"));
                return;
            }
        };
        match ManagedProcess::spawn(&bin, &["--list".to_string()]) {
            Ok(proc) => self.list_proc = Some(proc),
            Err(e) => self.error = Some(super::spawn_error(&bin, &e)),
        }
    }

    fn start(&mut self) {
        self.error = None;
        let Some(version) = self.config.version.clone() else {
            return;
        };
        let bin = match tool_bin(SUBDIR, BIN) {
            Ok(bin) => bin,
            Err(e) => {
                self.error = Some(format!("cannot resolve {BIN}: {e}"));
                return;
            }
        };
        let args = vec![
            "--version".to_string(),
            version,
            "--target".to_string(),
            self.config.target.arg().to_string(),
        ];
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
}
