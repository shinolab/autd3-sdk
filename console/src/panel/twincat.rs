use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::launch::tool_bin;
use crate::process::ManagedProcess;

const SUBDIR: &str = "twincat";
const BIN: &str = "twincat-cli";
const BASE_TIMES: &[&str] = &[
    "50us", "62.5us", "66.6us", "71.4us", "76.9us", "83.3us", "100us", "125us", "200us", "250us",
    "333us", "500us", "1ms", "none",
];

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Action {
    #[default]
    Run,
    Open,
    Doctor,
    InstallEsi,
}

impl Action {
    fn sub(self) -> &'static str {
        match self {
            Action::Run => "run",
            Action::Open => "open",
            Action::Doctor => "doctor",
            Action::InstallEsi => "install-esi",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Action::Run => "Run",
            Action::Open => "Open",
            Action::Doctor => "Doctor",
            Action::InstallEsi => "Install ESI",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TwinCatConfig {
    pub client: String,
    pub device_name: String,
    pub sync0: i64,
    pub task: i64,
    pub base: String,
    pub keep: bool,
}

impl Default for TwinCatConfig {
    fn default() -> Self {
        Self {
            client: String::new(),
            device_name: String::new(),
            sync0: 2,
            task: 1,
            base: "1ms".to_string(),
            keep: true,
        }
    }
}

#[derive(Default)]
pub struct TwinCatPanel {
    pub config: TwinCatConfig,
    action: Action,
    proc: Option<ManagedProcess>,
    error: Option<String>,
}

impl TwinCatPanel {
    pub fn pump(&mut self) {
        if let Some(proc) = &mut self.proc {
            proc.pump();
        }
    }

    pub fn is_running(&self) -> bool {
        self.proc.as_ref().is_some_and(ManagedProcess::is_running)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if !cfg!(target_os = "windows") {
            ui.weak("TwinCAT setup is available on Windows only.");
            return;
        }

        let running = self.is_running();

        ui.horizontal(|ui| {
            for action in [
                Action::Run,
                Action::Open,
                Action::Doctor,
                Action::InstallEsi,
            ] {
                ui.selectable_value(&mut self.action, action, action.label());
            }
        });

        ui.separator();

        match self.action {
            Action::Run => self.run_form(ui, running),
            Action::Open => {
                ui.weak("Open the already-saved TwinCAT project.");
            }
            Action::Doctor => {
                ui.weak("Diagnose virtualization-based security (must be OFF for real-time).");
            }
            Action::InstallEsi => {
                ui.weak("Install the bundled AUTD ESI (AUTD.xml) into the TwinCAT config dir.");
            }
        }

        ui.separator();

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new(self.action.label()))
                .clicked()
            {
                self.start();
            }
            if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
                self.stop();
            }
            ui.label(if running { "running" } else { "idle" });
        });

        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }

        ui.separator();
        super::log_view(ui, self.proc.as_ref());
    }

    fn run_form(&mut self, ui: &mut egui::Ui, running: bool) {
        egui::Grid::new("twincat-config")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Client IP");
                ui.add_enabled(
                    !running,
                    egui::TextEdit::singleline(&mut self.config.client).hint_text("localhost"),
                );
                ui.end_row();

                ui.label("Device name");
                ui.add_enabled(
                    !running,
                    egui::TextEdit::singleline(&mut self.config.device_name).hint_text("auto"),
                );
                ui.end_row();

                ui.label("Sync0 (x500us)");
                ui.add_enabled(
                    !running,
                    egui::DragValue::new(&mut self.config.sync0).range(1..=64),
                );
                ui.end_row();

                ui.label("Task (x base time)");
                ui.add_enabled(
                    !running,
                    egui::DragValue::new(&mut self.config.task).range(1..=64),
                );
                ui.end_row();

                ui.label("Base time");
                ui.add_enabled_ui(!running, |ui| {
                    egui::ComboBox::from_id_salt("twincat-base")
                        .selected_text(&self.config.base)
                        .show_ui(ui, |ui| {
                            for base in BASE_TIMES {
                                ui.selectable_value(
                                    &mut self.config.base,
                                    (*base).to_string(),
                                    *base,
                                );
                            }
                        });
                });
                ui.end_row();

                ui.label("Keep XAE Shell open");
                ui.add_enabled(
                    !running,
                    egui::Checkbox::without_text(&mut self.config.keep),
                );
                ui.end_row();
            });
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
        let mut args = vec![self.action.sub().to_string()];
        if self.action == Action::Run {
            if !self.config.client.is_empty() {
                args.push("--client".to_string());
                args.push(self.config.client.clone());
            }
            if !self.config.device_name.is_empty() {
                args.push("--device_name".to_string());
                args.push(self.config.device_name.clone());
            }
            args.push("--sync0".to_string());
            args.push(self.config.sync0.to_string());
            args.push("--task".to_string());
            args.push(self.config.task.to_string());
            if !self.config.base.is_empty() {
                args.push("--base".to_string());
                args.push(self.config.base.clone());
            }
            if self.config.keep {
                args.push("--keep".to_string());
            }
        }

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
