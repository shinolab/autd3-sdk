use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UdpConfig {
    pub interface: String,
    pub heartbeat_us: Option<u64>,
    pub response_timeout_ms: Option<u64>,
    pub enumeration_timeout_ms: Option<u64>,
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

const DEFAULT_HEARTBEAT_US: u64 = 10_000;
const DEFAULT_RESPONSE_TIMEOUT_MS: u64 = 200;
const DEFAULT_ENUMERATION_TIMEOUT_MS: u64 = 10_000;
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
        push_opt(args, "--heartbeat-us", u.heartbeat_us);
        push_opt(args, "--response-timeout-ms", u.response_timeout_ms);
        push_opt(args, "--enumeration-timeout-ms", u.enumeration_timeout_ms);
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

        optional_row(
            ui,
            "Heartbeat",
            &mut u.heartbeat_us,
            DEFAULT_HEARTBEAT_US,
            " µs",
        );
    }

    fn udp_advanced_ui(&mut self, ui: &mut egui::Ui) {
        let u = &mut self.udp;
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
    }

    pub fn hint() -> String {
        format!(
            "Devices on the UDP firmware (v0.10.0 or newer) need the host firewall to accept \
             UDP from fe80::/10 port {DEVICE_PORT} on the interface they hang off; no \
             administrator rights or IP address are needed."
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
    ui.label(label);
    ui.horizontal(|ui| {
        let mut custom = value.is_some();
        if ui.checkbox(&mut custom, "").changed() {
            *value = custom.then_some(default);
        }
        match value {
            Some(v) => {
                ui.add(egui::DragValue::new(v).suffix(suffix));
            }
            None => {
                ui.weak(format!("default ({default}{suffix})"));
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
                heartbeat_us: Some(20_000),
                response_timeout_ms: Some(300),
                enumeration_timeout_ms: Some(20_000),
            },
            ..OtaConfig::default()
        };
        assert_eq!(
            config.args("1.0.0", "cpu")[COMMON..],
            [
                "--interface",
                "eth0",
                "--heartbeat-us",
                "20000",
                "--response-timeout-ms",
                "300",
                "--enumeration-timeout-ms",
                "20000",
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

    #[test]
    fn a_config_saved_with_the_removed_udp_timeouts_still_loads() {
        let config: OtaConfig = serde_json::from_str(
            r#"{"reboot_wait_secs":30,"udp":{"heartbeat_us":20000,"reply_timeout_us":1500,"sync_timeout_ms":8000}}"#,
        )
        .unwrap();
        assert_eq!(config.reboot_wait_secs, 30);
        assert_eq!(
            config.udp,
            UdpConfig {
                heartbeat_us: Some(20_000),
                ..UdpConfig::default()
            }
        );
    }
}
