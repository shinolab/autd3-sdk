use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UdpConfig {
    pub interface: String,
    pub cycle_us: Option<u64>,
    pub reply_timeout_us: Option<u64>,
    pub response_timeout_ms: Option<u64>,
    pub enumeration_timeout_ms: Option<u64>,
    pub sync_timeout_ms: Option<u64>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OtaConfig {
    pub reboot_wait_secs: u64,
    pub udp: UdpConfig,
}

impl Default for OtaConfig {
    fn default() -> Self {
        Self {
            reboot_wait_secs: 10,
            udp: UdpConfig::default(),
        }
    }
}

const DEFAULT_CYCLE_US: u64 = 1000;
const DEFAULT_REPLY_TIMEOUT_US: u64 = 1000;
const DEFAULT_RESPONSE_TIMEOUT_MS: u64 = 200;
const DEFAULT_ENUMERATION_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_SYNC_TIMEOUT_MS: u64 = 5000;
const DEVICE_PORT: u16 = 44336;

fn push_opt(args: &mut Vec<String>, flag: &str, value: Option<u64>) {
    if let Some(value) = value {
        args.push(flag.to_string());
        args.push(value.to_string());
    }
}

impl OtaConfig {
    pub fn args(&self, version: &str, target: &str) -> Vec<String> {
        let mut args = vec![
            "--version".to_string(),
            version.to_string(),
            "--target".to_string(),
            target.to_string(),
            "--reboot-wait-secs".to_string(),
            self.reboot_wait_secs.to_string(),
        ];
        self.udp_args(&mut args);
        args
    }

    fn udp_args(&self, args: &mut Vec<String>) {
        let u = &self.udp;
        let interface = u.interface.trim();
        if !interface.is_empty() {
            args.push("--interface".to_string());
            args.push(interface.to_string());
        }
        push_opt(args, "--cycle-us", u.cycle_us);
        push_opt(args, "--reply-timeout-us", u.reply_timeout_us);
        push_opt(args, "--response-timeout-ms", u.response_timeout_ms);
        push_opt(args, "--enumeration-timeout-ms", u.enumeration_timeout_ms);
        push_opt(args, "--sync-timeout-ms", u.sync_timeout_ms);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.add_enabled_ui(enabled, |ui| {
            egui::Grid::new("ota-common")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Reboot wait");
                    ui.add(
                        egui::DragValue::new(&mut self.reboot_wait_secs)
                            .range(1..=600)
                            .suffix(" s"),
                    );
                    ui.end_row();

                    self.udp_ui(ui);
                });
            egui::CollapsingHeader::new("Advanced UDP options")
                .id_salt("ota-udp-advanced")
                .show(ui, |ui| {
                    egui::Grid::new("ota-udp-advanced-grid")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| self.udp_advanced_ui(ui));
                });
        });
    }

    fn udp_ui(&mut self, ui: &mut egui::Ui) {
        let u = &mut self.udp;
        ui.label("Interface");
        ui.add(
            egui::TextEdit::singleline(&mut u.interface)
                .hint_text("auto")
                .desired_width(240.0),
        );
        ui.end_row();

        optional_row(ui, "Cycle", &mut u.cycle_us, DEFAULT_CYCLE_US, " µs");
    }

    fn udp_advanced_ui(&mut self, ui: &mut egui::Ui) {
        let u = &mut self.udp;
        optional_row(
            ui,
            "Reply timeout",
            &mut u.reply_timeout_us,
            DEFAULT_REPLY_TIMEOUT_US,
            " µs",
        );
        optional_row(
            ui,
            "Response timeout",
            &mut u.response_timeout_ms,
            DEFAULT_RESPONSE_TIMEOUT_MS,
            " ms",
        );
        optional_row(
            ui,
            "Enumeration timeout",
            &mut u.enumeration_timeout_ms,
            DEFAULT_ENUMERATION_TIMEOUT_MS,
            " ms",
        );
        optional_row(
            ui,
            "Sync timeout",
            &mut u.sync_timeout_ms,
            DEFAULT_SYNC_TIMEOUT_MS,
            " ms",
        );
    }

    pub fn hint() -> String {
        format!(
            "Devices on the UDP firmware (v0.10.0 or newer) need the host firewall to accept \
             UDP from fe80::/10 port {DEVICE_PORT} on the interface they hang off; no \
             administrator rights or IP address are needed. Devices on the EtherCAT firmware \
             v0.9.x are reached over EtherCAT (Npcap on Windows, the cap_net_raw capability on \
             Linux) and moved to the UDP firmware."
        )
    }
}

fn optional_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<u64>,
    default: u64,
    suffix: &str,
) {
    let default_text = format!("default ({default}{suffix})");
    optional_row_with(ui, label, value, &default_text, default, suffix);
}

fn optional_row_with(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<u64>,
    unset_text: &str,
    initial: u64,
    suffix: &str,
) {
    ui.label(label);
    ui.horizontal(|ui| {
        let mut custom = value.is_some();
        if ui.checkbox(&mut custom, "").changed() {
            *value = custom.then_some(initial);
        }
        match value {
            Some(v) => {
                ui.add(egui::DragValue::new(v).suffix(suffix));
            }
            None => {
                ui.weak(unset_text);
            }
        }
    });
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMON: usize = 6;

    #[test]
    fn the_common_arguments_lead_every_invocation() {
        let config = OtaConfig {
            reboot_wait_secs: 20,
            ..OtaConfig::default()
        };
        assert_eq!(
            config.args("0.10.0", "both"),
            [
                "--version",
                "0.10.0",
                "--target",
                "both",
                "--reboot-wait-secs",
                "20",
            ]
        );
    }

    #[test]
    fn only_customized_transport_options_are_passed() {
        let config = OtaConfig {
            udp: UdpConfig {
                interface: "  eth0 ".to_string(),
                cycle_us: Some(2000),
                reply_timeout_us: Some(1500),
                response_timeout_ms: Some(300),
                enumeration_timeout_ms: Some(20_000),
                sync_timeout_ms: Some(8000),
            },
            ..OtaConfig::default()
        };
        assert_eq!(
            config.args("1.0.0", "cpu")[COMMON..],
            [
                "--interface",
                "eth0",
                "--cycle-us",
                "2000",
                "--reply-timeout-us",
                "1500",
                "--response-timeout-ms",
                "300",
                "--enumeration-timeout-ms",
                "20000",
                "--sync-timeout-ms",
                "8000",
            ]
        );
        assert_eq!(OtaConfig::default().args("1.0.0", "cpu").len(), COMMON);
    }

    #[test]
    fn a_config_saved_with_a_remote_link_still_loads() {
        let config: OtaConfig = serde_json::from_str(
            r#"{"link":"Remote","reboot_wait_secs":30,"remote":{"addr":"10.0.0.5:9000"}}"#,
        )
        .unwrap();
        assert_eq!(config.reboot_wait_secs, 30);
        assert_eq!(config.udp, UdpConfig::default());
    }
}
