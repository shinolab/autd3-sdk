mod confirm;
mod ota;

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::launch::tool_bin;
use crate::process::ManagedProcess;

use confirm::{Answer, Confirm};
use ota::OtaConfig;

const SUBDIR: &str = "firmware";
const BIN: &str = "autd3-firmware-writer";
const OTA_BIN: &str = "autd3-rs-firmware-ota";
const MIN_OTA_VERSION: (u64, u64, u64) = (0, 9, 0);

fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim_start_matches('v').split('.');
    let mut next = || parts.next()?.parse::<u64>().ok();
    Some((next()?, next()?, next()?))
}

fn supports_ota(version: &str) -> bool {
    parse_version(version).is_some_and(|v| v >= MIN_OTA_VERSION)
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Method {
    #[default]
    Ethercat,
    Jtag,
}

impl Method {
    fn label(self) -> &'static str {
        match self {
            Method::Jtag => "JTAG",
            Method::Ethercat => "EtherCAT (OTA)",
        }
    }

    fn bin(self) -> &'static str {
        match self {
            Method::Jtag => BIN,
            Method::Ethercat => OTA_BIN,
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    #[default]
    Both,
    Fpga,
    Cpu,
}

impl Target {
    fn cpu(self) -> bool {
        matches!(self, Target::Both | Target::Cpu)
    }

    fn fpga(self) -> bool {
        matches!(self, Target::Both | Target::Fpga)
    }

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
#[serde(default)]
pub struct FirmwareConfig {
    pub version: Option<String>,
    pub target: Target,
    pub method: Method,
    pub ota: OtaConfig,
}

impl FirmwareConfig {
    fn args(&self) -> Result<Vec<String>, String> {
        let version = self
            .version
            .clone()
            .ok_or_else(|| "select a version".to_string())?;
        match self.method {
            Method::Jtag => Ok(vec![
                "--version".to_string(),
                version,
                "--target".to_string(),
                self.target.arg().to_string(),
            ]),
            Method::Ethercat if !supports_ota(&version) => Err(format!(
                "v{version} predates EtherCAT updates (needs v{}.{}.{} or newer); choose Method = JTAG",
                MIN_OTA_VERSION.0, MIN_OTA_VERSION.1, MIN_OTA_VERSION.2
            )),
            Method::Ethercat => self.ota.args(&version, self.target.arg()),
        }
    }

    fn verify_args(&self) -> Result<Vec<String>, String> {
        let mut args = self.args()?;
        args.push("--verify-only".to_string());
        Ok(args)
    }
}

#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    Verify,
    Confirm(Confirm),
    Flash,
}

#[derive(Default)]
pub struct FirmwarePanel {
    pub config: FirmwareConfig,
    phase: Phase,
    proc: Option<ManagedProcess>,
    list_proc: Option<ManagedProcess>,
    versions: Vec<String>,
    listed: bool,
    error: Option<String>,
}

impl FirmwarePanel {
    pub fn pump(&mut self) {
        let finished = self.proc.as_mut().is_some_and(|proc| {
            proc.pump();
            !proc.is_running()
        });
        if finished {
            match self.phase {
                Phase::Verify => self.finish_verify(),
                Phase::Flash => self.phase = Phase::Idle,
                Phase::Idle | Phase::Confirm(_) => {}
            }
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

        let running = self.proc.as_ref().is_some_and(ManagedProcess::is_running);
        let confirming = matches!(self.phase, Phase::Confirm(_));
        let listing = self.list_proc.is_some();
        let locked = running || confirming;
        let busy = locked || listing;

        self.config_ui(ui, busy, locked);

        ui.separator();
        self.method_ui(ui, locked);
        ui.separator();

        let args = self.config.args();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy && args.is_ok(), egui::Button::new("Flash"))
                .clicked()
            {
                self.start();
            }
            if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
                self.stop();
            }
            ui.label(if listing {
                "fetching versions..."
            } else {
                match self.phase {
                    Phase::Idle => "idle",
                    Phase::Verify => "reading the current firmware version...",
                    Phase::Confirm(_) => "waiting for confirmation",
                    Phase::Flash => "flashing",
                }
            });
        });

        self.error_ui(ui, args.as_ref().err());

        ui.separator();
        super::log_view(ui, self.proc.as_ref());

        self.confirm_ui(ui);
    }

    fn confirm_ui(&mut self, ui: &egui::Ui) {
        let answer = match &self.phase {
            Phase::Confirm(confirm) => confirm.ui(ui),
            _ => Answer::Pending,
        };
        match answer {
            Answer::Update => self.start_flash(),
            Answer::Cancel => self.phase = Phase::Idle,
            Answer::Pending => {}
        }
    }

    fn config_ui(&mut self, ui: &mut egui::Ui, busy: bool, locked: bool) {
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
                                    let ota_only =
                                        self.config.method == Method::Ethercat && !supports_ota(v);
                                    let label = if ota_only {
                                        format!("{v} (JTAG only)")
                                    } else {
                                        v.clone()
                                    };
                                    ui.add_enabled_ui(!ota_only, |ui| {
                                        ui.selectable_value(
                                            &mut self.config.version,
                                            Some(v.clone()),
                                            label,
                                        );
                                    });
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
                ui.add_enabled_ui(!locked, |ui| {
                    ui.horizontal(|ui| {
                        for t in [Target::Both, Target::Fpga, Target::Cpu] {
                            ui.selectable_value(&mut self.config.target, t, t.label());
                        }
                    });
                });
                ui.end_row();

                ui.label("Method");
                ui.add_enabled_ui(!locked, |ui| {
                    ui.horizontal(|ui| {
                        for m in [Method::Ethercat, Method::Jtag] {
                            ui.selectable_value(&mut self.config.method, m, m.label());
                        }
                    });
                });
                ui.end_row();
            });
    }

    fn error_ui(&self, ui: &mut egui::Ui, invalid: Option<&String>) {
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        } else if let Some(invalid) = invalid
            && self.config.version.is_some()
        {
            ui.colored_label(egui::Color32::LIGHT_RED, invalid);
        }
        if self.config.method == Method::Ethercat
            && self
                .proc
                .as_ref()
                .and_then(|p| p.logs().last())
                .is_some_and(|last| last == ota::MISSING_DLL_EXIT)
        {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "autd3-rs-firmware-ota could not start: a required DLL is missing \
                 (install Npcap for EtherCAT updates).",
            );
        }
    }

    fn method_ui(&mut self, ui: &mut egui::Ui, locked: bool) {
        match self.config.method {
            Method::Jtag => {
                ui.weak(
                    "Flashing requires SEGGER J-Link (CPU) and Xilinx Vivado / vivado_lab (FPGA) \
                     installed and on PATH. Connect the configuration cable and power on the AUTD3.",
                );
            }
            Method::Ethercat => {
                egui::ScrollArea::vertical()
                    .id_salt("ota-settings")
                    .max_height(ui.available_height() * 0.5)
                    .show(ui, |ui| self.config.ota.ui(ui, !locked));
                ui.weak(
                    "Updates the firmware over EtherCAT without a cable. The devices must \
                     already run CPU/FPGA firmware v0.9 or newer (write it once via JTAG \
                     otherwise); CPU is updated first, then FPGA. Transducer output stops \
                     during the update; do not power off the devices while it runs.",
                );
                let bin = tool_bin(SUBDIR, OTA_BIN)
                    .map_or_else(|_| OTA_BIN.to_string(), |p| p.display().to_string());
                ui.weak(self.config.ota.hint(&bin));
            }
        }
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
        match self.config.method {
            Method::Ethercat => self.start_verify(),
            Method::Jtag => self.start_flash(),
        }
    }

    fn start_verify(&mut self) {
        let Ok(args) = self
            .config
            .verify_args()
            .inspect_err(|e| self.error = Some(e.clone()))
        else {
            return;
        };
        if self.spawn(OTA_BIN, &args) {
            self.phase = Phase::Verify;
        }
    }

    fn finish_verify(&mut self) {
        self.phase = Phase::Idle;
        let logs = self.proc.as_ref().map_or(&[][..], ManagedProcess::logs);
        match confirm::outcome(logs) {
            Ok(devices) => {
                self.phase = Phase::Confirm(Confirm {
                    version: self.config.version.clone().unwrap_or_default(),
                    target: self.config.target,
                    devices,
                });
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn start_flash(&mut self) {
        self.phase = Phase::Idle;
        self.error = None;
        let Ok(args) = self
            .config
            .args()
            .inspect_err(|e| self.error = Some(e.clone()))
        else {
            return;
        };
        let name = self.config.method.bin();
        if self.spawn(name, &args) {
            self.phase = Phase::Flash;
        }
    }

    fn spawn(&mut self, name: &str, args: &[String]) -> bool {
        let bin = match tool_bin(SUBDIR, name) {
            Ok(bin) => bin,
            Err(e) => {
                self.error = Some(format!("cannot resolve {name}: {e}"));
                return false;
            }
        };
        match ManagedProcess::spawn(&bin, args) {
            Ok(proc) => {
                self.proc = Some(proc);
                true
            }
            Err(e) => {
                self.error = Some(super::spawn_error(&bin, &e));
                false
            }
        }
    }

    fn stop(&mut self) {
        if let Some(proc) = &mut self.proc {
            proc.kill();
            proc.pump();
        }
        self.phase = Phase::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jtag_keeps_the_writer_arguments() {
        let config = FirmwareConfig {
            version: Some("0.9.0".to_string()),
            target: Target::Fpga,
            method: Method::Jtag,
            ota: OtaConfig::default(),
        };
        assert_eq!(
            config.args().unwrap(),
            ["--version", "0.9.0", "--target", "fpga"]
        );
        assert_eq!(config.method.bin(), "autd3-firmware-writer");
    }

    #[test]
    fn ethercat_hands_the_version_and_target_to_the_ota_tool() {
        let config = FirmwareConfig {
            version: Some("0.9.0".to_string()),
            target: Target::Cpu,
            method: Method::Ethercat,
            ota: OtaConfig::default(),
        };
        let args = config.args().unwrap();
        assert_eq!(args[..4], ["--version", "0.9.0", "--target", "cpu"]);
        assert_eq!(config.method.bin(), "autd3-rs-firmware-ota");
    }

    #[test]
    fn the_version_is_read_back_before_the_write_with_the_very_same_arguments() {
        let config = FirmwareConfig {
            version: Some("0.9.0".to_string()),
            target: Target::Both,
            method: Method::Ethercat,
            ota: OtaConfig::default(),
        };
        let args = config.args().unwrap();
        let verify = config.verify_args().unwrap();
        assert_eq!(verify[..args.len()], args[..]);
        assert_eq!(verify[args.len()..], ["--verify-only"]);
        assert!(config.target.cpu() && config.target.fpga());
        assert!(!Target::Fpga.cpu() && Target::Fpga.fpga());
        assert!(Target::Cpu.cpu() && !Target::Cpu.fpga());
    }

    #[test]
    fn no_version_means_nothing_to_flash() {
        assert!(FirmwareConfig::default().args().is_err());
    }

    #[test]
    fn ethercat_refuses_versions_that_predate_ota_but_jtag_takes_them() {
        let mut config = FirmwareConfig {
            version: Some("0.8.0".to_string()),
            target: Target::Both,
            method: Method::Ethercat,
            ota: OtaConfig::default(),
        };
        let err = config.args().unwrap_err();
        assert!(err.contains("JTAG"), "{err}");
        config.method = Method::Jtag;
        assert!(config.args().is_ok());
        assert!(supports_ota("0.9.0"));
        assert!(supports_ota("v1.0.0"));
        assert!(!supports_ota("0.8.99"));
        assert!(!supports_ota("latest"));
    }

    #[test]
    fn a_config_saved_before_the_ota_method_existed_loads_with_the_default_method() {
        let config: FirmwareConfig =
            serde_json::from_str(r#"{"version":"0.8.0","target":"Cpu"}"#).unwrap();
        assert_eq!(config.version.as_deref(), Some("0.8.0"));
        assert!(config.target == Target::Cpu);
        assert_eq!(config.method, Method::Ethercat);
        assert_eq!(config.ota, OtaConfig::default());
    }
}
