use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct UdpConfig {
    pub interface: String,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct OtaConfig {
    pub devices: usize,
    pub reboot_wait_secs: u64,
    pub udp: UdpConfig,
}

impl Default for OtaConfig {
    fn default() -> Self {
        Self {
            devices: 1,
            reboot_wait_secs: 10,
            udp: UdpConfig::default(),
        }
    }
}

const DEVICE_PORT: u16 = 44336;
const MAX_DEVICES: usize = 128;

impl OtaConfig {
    pub(crate) fn args(&self, version: &str, target: &str) -> Vec<String> {
        let mut args = vec![
            "--version".to_string(),
            version.to_string(),
            "--target".to_string(),
            target.to_string(),
            "--devices".to_string(),
            self.devices.to_string(),
            "--reboot-wait-secs".to_string(),
            self.reboot_wait_secs.to_string(),
        ];
        self.udp_args(&mut args);
        args
    }

    fn udp_args(&self, args: &mut Vec<String>) {
        let interface = self.udp.interface.trim();
        if !interface.is_empty() {
            args.push("--interface".to_string());
            args.push(interface.to_string());
        }
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.add_enabled_ui(enabled, |ui| {
            egui::Grid::new("ota-common")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Devices");
                    ui.add(egui::DragValue::new(&mut self.devices).range(1..=MAX_DEVICES));
                    ui.end_row();

                    ui.label("Reboot wait");
                    ui.add(
                        egui::DragValue::new(&mut self.reboot_wait_secs)
                            .range(1..=600)
                            .suffix(" s"),
                    );
                    ui.end_row();

                    self.udp_ui(ui);
                });
        });
    }

    fn udp_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Interface");
        ui.add(
            egui::TextEdit::singleline(&mut self.udp.interface)
                .hint_text("auto")
                .desired_width(240.0),
        );
        ui.end_row();
    }

    pub(crate) fn hint() -> String {
        format!(
            "Devices on the UDP firmware (v0.10.0 or newer) need the host firewall to accept \
             UDP from fe80::/10 port {DEVICE_PORT} on the interface they hang off; no \
             administrator rights or IP address are needed."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMON: usize = 8;

    #[test]
    fn the_common_arguments_lead_every_invocation() {
        let config = OtaConfig {
            devices: 3,
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
                "--devices",
                "3",
                "--reboot-wait-secs",
                "20",
            ]
        );
    }

    #[test]
    fn only_a_named_interface_is_passed() {
        let config = OtaConfig {
            udp: UdpConfig {
                interface: "  eth0 ".to_string(),
            },
            ..OtaConfig::default()
        };
        assert_eq!(
            config.args("1.0.0", "cpu")[COMMON..],
            ["--interface", "eth0"]
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
        assert_eq!(config.devices, 1);
        assert_eq!(config.udp, UdpConfig::default());
    }

    #[test]
    fn a_config_saved_with_the_removed_udp_timeouts_still_loads() {
        let config: OtaConfig = serde_json::from_str(
            r#"{"reboot_wait_secs":30,"udp":{"heartbeat_us":20000,"reply_timeout_us":1500,"sync_timeout_ms":8000,"response_timeout_ms":300,"enumeration_timeout_ms":20000}}"#,
        )
        .unwrap();
        assert_eq!(config.reboot_wait_secs, 30);
        assert_eq!(config.udp, UdpConfig::default());
    }

    #[test]
    fn a_config_saved_with_the_removed_heartbeat_and_open_timeout_still_loads() {
        let config: OtaConfig = serde_json::from_str(
            r#"{"devices":2,"udp":{"interface":"eth0","heartbeat_us":20000,"open_timeout_ms":20000}}"#,
        )
        .unwrap();
        assert_eq!(config.devices, 2);
        assert_eq!(
            config.udp,
            UdpConfig {
                interface: "eth0".to_string(),
            }
        );
    }
}
