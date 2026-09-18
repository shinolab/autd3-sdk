use std::net::{IpAddr, SocketAddr};

use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum LinkKind {
    #[default]
    Echocat,
    TwinCat,
    Remote,
}

impl LinkKind {
    const ALL: [Self; 3] = [Self::Echocat, Self::TwinCat, Self::Remote];

    fn arg(self) -> &'static str {
        match self {
            Self::Echocat => "echocat",
            Self::TwinCat => "twincat",
            Self::Remote => "remote",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Echocat => "echocat",
            Self::TwinCat => "TwinCAT",
            Self::Remote => "Remote",
        }
    }
}

#[derive(Default, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EchocatConfig {
    pub interface: String,
    pub cycle_us: Option<u64>,
    pub frame_phase_us: Option<u64>,
    pub pdu_timeout_ms: Option<u64>,
    pub state_transition_timeout_ms: Option<u64>,
    pub dc_static_sync_iterations: Option<u64>,
    pub dc_start_delay_ms: Option<u64>,
    pub sync_tolerance_us: Option<u64>,
    pub sync_timeout_ms: Option<u64>,
    pub process_data_watchdog_ms: Option<u64>,
    pub spin_margin_us: Option<u64>,
}

#[derive(Default, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TwinCatConfig {
    pub remote: bool,
    pub addr: String,
    pub ams_net_id: String,
    pub connect_timeout_ms: Option<u64>,
    pub read_timeout_ms: Option<u64>,
    pub write_timeout_ms: Option<u64>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteConfig {
    pub discover: bool,
    pub instance: String,
    pub discovery_timeout_ms: Option<u64>,
    pub addr: String,
    pub timeout_ms: Option<u64>,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            discover: true,
            instance: String::new(),
            discovery_timeout_ms: None,
            addr: String::new(),
            timeout_ms: None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OtaConfig {
    pub link: LinkKind,
    pub reboot_wait_secs: u64,
    pub echocat: EchocatConfig,
    pub twincat: TwinCatConfig,
    pub remote: RemoteConfig,
}

impl Default for OtaConfig {
    fn default() -> Self {
        Self {
            link: LinkKind::default(),
            reboot_wait_secs: 10,
            echocat: EchocatConfig::default(),
            twincat: TwinCatConfig::default(),
            remote: RemoteConfig::default(),
        }
    }
}

const DEFAULT_CYCLE_US: u64 = if cfg!(target_os = "windows") {
    2000
} else {
    1000
};

pub const MISSING_DLL_EXIT: &str = "[process exited with code -1073741515]";

fn push_opt(args: &mut Vec<String>, flag: &str, value: Option<u64>) {
    if let Some(value) = value {
        args.push(flag.to_string());
        args.push(value.to_string());
    }
}

fn is_ams_net_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 6 && parts.iter().all(|p| p.parse::<u8>().is_ok())
}

impl OtaConfig {
    pub fn args(&self, version: &str, target: &str) -> Result<Vec<String>, String> {
        let mut args = vec![
            "--version".to_string(),
            version.to_string(),
            "--target".to_string(),
            target.to_string(),
            "--reboot-wait-secs".to_string(),
            self.reboot_wait_secs.to_string(),
            "--link".to_string(),
            self.link.arg().to_string(),
        ];
        match self.link {
            LinkKind::Echocat => self.echocat_args(&mut args),
            LinkKind::TwinCat => self.twincat_args(&mut args)?,
            LinkKind::Remote => self.remote_args(&mut args)?,
        }
        Ok(args)
    }

    fn echocat_args(&self, args: &mut Vec<String>) {
        let e = &self.echocat;
        let interface = e.interface.trim();
        if !interface.is_empty() {
            args.push("--interface".to_string());
            args.push(interface.to_string());
        }
        push_opt(args, "--cycle-us", e.cycle_us);
        push_opt(args, "--frame-phase-us", e.frame_phase_us);
        push_opt(args, "--pdu-timeout-ms", e.pdu_timeout_ms);
        push_opt(
            args,
            "--state-transition-timeout-ms",
            e.state_transition_timeout_ms,
        );
        push_opt(
            args,
            "--dc-static-sync-iterations",
            e.dc_static_sync_iterations,
        );
        push_opt(args, "--dc-start-delay-ms", e.dc_start_delay_ms);
        push_opt(args, "--sync-tolerance-us", e.sync_tolerance_us);
        push_opt(args, "--sync-timeout-ms", e.sync_timeout_ms);
        push_opt(
            args,
            "--process-data-watchdog-ms",
            e.process_data_watchdog_ms,
        );
        push_opt(args, "--spin-margin-us", e.spin_margin_us);
    }

    fn twincat_args(&self, args: &mut Vec<String>) -> Result<(), String> {
        let t = &self.twincat;
        if t.remote {
            let addr = t.addr.trim();
            addr.parse::<IpAddr>()
                .map_err(|_| format!("TwinCAT server address {addr:?} is not an IP address"))?;
            let ams_net_id = t.ams_net_id.trim();
            if !is_ams_net_id(ams_net_id) {
                return Err(format!(
                    "AMS Net ID {ams_net_id:?} must look like 192.168.0.1.1.1"
                ));
            }
            args.extend([
                "--twincat-remote".to_string(),
                addr.to_string(),
                "--ams-net-id".to_string(),
                ams_net_id.to_string(),
            ]);
        }
        push_opt(args, "--twincat-connect-timeout-ms", t.connect_timeout_ms);
        push_opt(args, "--twincat-read-timeout-ms", t.read_timeout_ms);
        push_opt(args, "--twincat-write-timeout-ms", t.write_timeout_ms);
        Ok(())
    }

    fn remote_args(&self, args: &mut Vec<String>) -> Result<(), String> {
        let r = &self.remote;
        if r.discover {
            let instance = r.instance.trim();
            if !instance.is_empty() {
                args.push("--remote-instance".to_string());
                args.push(instance.to_string());
            }
            push_opt(args, "--discovery-timeout-ms", r.discovery_timeout_ms);
        } else {
            let addr = r.addr.trim();
            addr.parse::<SocketAddr>()
                .map_err(|_| format!("remote address {addr:?} must look like 192.168.0.10:8080"))?;
            args.push("--remote-addr".to_string());
            args.push(addr.to_string());
        }
        push_opt(args, "--remote-timeout-ms", self.remote.timeout_ms);
        Ok(())
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.add_enabled_ui(enabled, |ui| {
            egui::Grid::new("ota-common")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Link");
                    ui.horizontal(|ui| {
                        for kind in LinkKind::ALL {
                            ui.selectable_value(&mut self.link, kind, kind.label());
                        }
                    });
                    ui.end_row();

                    ui.label("Reboot wait");
                    ui.add(
                        egui::DragValue::new(&mut self.reboot_wait_secs)
                            .range(1..=600)
                            .suffix(" s"),
                    );
                    ui.end_row();

                    match self.link {
                        LinkKind::Echocat => self.echocat_ui(ui),
                        LinkKind::TwinCat => self.twincat_ui(ui),
                        LinkKind::Remote => self.remote_ui(ui),
                    }
                });
            if self.link == LinkKind::Echocat {
                egui::CollapsingHeader::new("Advanced echocat options")
                    .id_salt("ota-echocat-advanced")
                    .show(ui, |ui| {
                        egui::Grid::new("ota-echocat-advanced-grid")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| self.echocat_advanced_ui(ui));
                    });
            }
        });
    }

    fn echocat_ui(&mut self, ui: &mut egui::Ui) {
        let e = &mut self.echocat;
        ui.label("Interface");
        ui.add(
            egui::TextEdit::singleline(&mut e.interface)
                .hint_text("auto")
                .desired_width(240.0),
        );
        ui.end_row();

        optional_row(ui, "SYNC0 period", &mut e.cycle_us, DEFAULT_CYCLE_US, " µs");
    }

    fn echocat_advanced_ui(&mut self, ui: &mut egui::Ui) {
        let e = &mut self.echocat;
        optional_row_with(ui, "Frame phase", &mut e.frame_phase_us, "auto", 0, " µs");
        optional_row(ui, "PDU timeout", &mut e.pdu_timeout_ms, 100, " ms");
        optional_row(
            ui,
            "State transition timeout",
            &mut e.state_transition_timeout_ms,
            10_000,
            " ms",
        );
        optional_row(
            ui,
            "DC static sync iterations",
            &mut e.dc_static_sync_iterations,
            10_000,
            "",
        );
        optional_row(ui, "DC start delay", &mut e.dc_start_delay_ms, 100, " ms");
        optional_row(ui, "Sync tolerance", &mut e.sync_tolerance_us, 1, " µs");
        optional_row(ui, "Sync timeout", &mut e.sync_timeout_ms, 10_000, " ms");
        optional_row(
            ui,
            "Process data watchdog",
            &mut e.process_data_watchdog_ms,
            100,
            " ms",
        );
        optional_row_with(
            ui,
            "Spin-sleep margin",
            &mut e.spin_margin_us,
            "sleep",
            500,
            " µs",
        );
    }

    fn twincat_ui(&mut self, ui: &mut egui::Ui) {
        let t = &mut self.twincat;
        ui.label("Server");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut t.remote, false, "Local");
            ui.selectable_value(&mut t.remote, true, "Remote");
        });
        ui.end_row();

        if t.remote {
            ui.label("Server address");
            ui.add(
                egui::TextEdit::singleline(&mut t.addr)
                    .hint_text("192.168.0.2")
                    .desired_width(240.0),
            );
            ui.end_row();

            ui.label("AMS Net ID");
            ui.add(
                egui::TextEdit::singleline(&mut t.ams_net_id)
                    .hint_text("192.168.0.2.1.1")
                    .desired_width(240.0),
            );
            ui.end_row();
        }

        optional_row_with(
            ui,
            "Connect timeout",
            &mut t.connect_timeout_ms,
            "none",
            5000,
            " ms",
        );
        optional_row_with(
            ui,
            "Read timeout",
            &mut t.read_timeout_ms,
            "none",
            5000,
            " ms",
        );
        optional_row_with(
            ui,
            "Write timeout",
            &mut t.write_timeout_ms,
            "none",
            5000,
            " ms",
        );
    }

    fn remote_ui(&mut self, ui: &mut egui::Ui) {
        let r = &mut self.remote;
        ui.label("Server");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut r.discover, true, "Discover (mDNS)");
            ui.selectable_value(&mut r.discover, false, "Address");
        });
        ui.end_row();

        if r.discover {
            ui.label("Instance");
            ui.add(
                egui::TextEdit::singleline(&mut r.instance)
                    .hint_text("any (only one appliance may answer)")
                    .desired_width(240.0),
            );
            ui.end_row();

            optional_row_with(
                ui,
                "Discovery timeout",
                &mut r.discovery_timeout_ms,
                "default",
                5000,
                " ms",
            );
        } else {
            ui.label("Address");
            ui.add(
                egui::TextEdit::singleline(&mut r.addr)
                    .hint_text("192.168.0.10:8080")
                    .desired_width(240.0),
            );
            ui.end_row();
        }

        optional_row_with(ui, "Timeout", &mut r.timeout_ms, "none", 5000, " ms");
    }

    pub fn hint(&self, bin: &str) -> String {
        match self.link {
            LinkKind::Echocat if cfg!(target_os = "windows") => {
                "echocat needs Npcap installed (WinPcap API-compatible mode).".to_string()
            }
            LinkKind::Echocat if cfg!(target_os = "linux") => format!(
                "echocat needs raw-socket access: run `sudo setcap \
                 cap_net_raw,cap_net_admin,cap_sys_nice+ep {bin}` once, and again after \
                 every console update (the capability is lost when the binary is replaced)."
            ),
            LinkKind::Echocat => format!(
                "echocat needs read/write access to /dev/bpf*: run `sudo chmod g+rw /dev/bpf*` \
                 (reset at reboot) or start {bin} from a root shell."
            ),
            LinkKind::TwinCat => {
                "The TwinCAT runtime must be in RUN mode with the AUTD3 devices configured."
                    .to_string()
            }
            LinkKind::Remote if self.remote.discover => {
                "The appliance must be running, connected to the devices, and on the same network \
                 (found over mDNS)."
                    .to_string()
            }
            LinkKind::Remote => {
                "The remote server must be running and connected to the devices.".to_string()
            }
        }
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

    fn tail(args: &[String]) -> Vec<&str> {
        let link = args.iter().position(|a| a == "--link").unwrap();
        args[link + 2..].iter().map(String::as_str).collect()
    }

    #[test]
    fn the_common_arguments_lead_every_invocation() {
        let config = OtaConfig {
            reboot_wait_secs: 20,
            ..OtaConfig::default()
        };
        assert_eq!(
            config.args("0.9.0", "both").unwrap(),
            [
                "--version",
                "0.9.0",
                "--target",
                "both",
                "--reboot-wait-secs",
                "20",
                "--link",
                "echocat",
            ]
        );
    }

    #[test]
    fn only_customized_echocat_options_are_passed() {
        let config = OtaConfig {
            echocat: EchocatConfig {
                interface: "  eth0 ".to_string(),
                cycle_us: Some(2000),
                frame_phase_us: Some(250),
                pdu_timeout_ms: Some(50),
                state_transition_timeout_ms: Some(20_000),
                dc_static_sync_iterations: Some(100),
                dc_start_delay_ms: Some(200),
                sync_tolerance_us: Some(3),
                sync_timeout_ms: Some(15_000),
                process_data_watchdog_ms: Some(300),
                spin_margin_us: Some(400),
            },
            ..OtaConfig::default()
        };
        assert_eq!(
            tail(&config.args("1.0.0", "cpu").unwrap()),
            [
                "--interface",
                "eth0",
                "--cycle-us",
                "2000",
                "--frame-phase-us",
                "250",
                "--pdu-timeout-ms",
                "50",
                "--state-transition-timeout-ms",
                "20000",
                "--dc-static-sync-iterations",
                "100",
                "--dc-start-delay-ms",
                "200",
                "--sync-tolerance-us",
                "3",
                "--sync-timeout-ms",
                "15000",
                "--process-data-watchdog-ms",
                "300",
                "--spin-margin-us",
                "400",
            ]
        );
        assert!(tail(&OtaConfig::default().args("1.0.0", "cpu").unwrap()).is_empty());
    }

    #[test]
    fn a_local_twincat_passes_only_its_timeouts() {
        let config = OtaConfig {
            link: LinkKind::TwinCat,
            twincat: TwinCatConfig {
                addr: "not an address".to_string(),
                read_timeout_ms: Some(1500),
                ..TwinCatConfig::default()
            },
            ..OtaConfig::default()
        };
        let args = config.args("1.0.0", "fpga").unwrap();
        assert!(args.windows(2).any(|w| w == ["--link", "twincat"]));
        assert_eq!(tail(&args), ["--twincat-read-timeout-ms", "1500"]);
    }

    #[test]
    fn a_remote_twincat_is_validated_before_it_is_passed() {
        let mut config = OtaConfig {
            link: LinkKind::TwinCat,
            twincat: TwinCatConfig {
                remote: true,
                addr: " 192.168.0.2 ".to_string(),
                ams_net_id: "192.168.0.2.1.1".to_string(),
                ..TwinCatConfig::default()
            },
            ..OtaConfig::default()
        };
        assert_eq!(
            tail(&config.args("1.0.0", "both").unwrap()),
            [
                "--twincat-remote",
                "192.168.0.2",
                "--ams-net-id",
                "192.168.0.2.1.1"
            ]
        );

        config.twincat.ams_net_id = "192.168.0.2.1".to_string();
        assert!(config.args("1.0.0", "both").is_err());
        config.twincat.ams_net_id = "192.168.0.2.1.256".to_string();
        assert!(config.args("1.0.0", "both").is_err());
        config.twincat.ams_net_id = "192.168.0.2.1.1".to_string();
        config.twincat.addr = "twincat-host".to_string();
        assert!(config.args("1.0.0", "both").is_err());
    }

    #[test]
    fn the_remote_link_needs_a_socket_address() {
        let mut config = OtaConfig {
            link: LinkKind::Remote,
            remote: RemoteConfig {
                discover: false,
                addr: "10.0.0.5:9000".to_string(),
                timeout_ms: Some(800),
                ..RemoteConfig::default()
            },
            ..OtaConfig::default()
        };
        assert_eq!(
            tail(&config.args("1.0.0", "both").unwrap()),
            [
                "--remote-addr",
                "10.0.0.5:9000",
                "--remote-timeout-ms",
                "800"
            ]
        );
        config.remote.addr = "10.0.0.5".to_string();
        assert!(config.args("1.0.0", "both").is_err());
    }

    #[test]
    fn the_remote_link_discovers_the_appliance_by_default() {
        let mut config = OtaConfig {
            link: LinkKind::Remote,
            ..OtaConfig::default()
        };
        assert!(tail(&config.args("1.0.0", "both").unwrap()).is_empty());
        config.remote.instance = " autd3-lab ".to_string();
        config.remote.discovery_timeout_ms = Some(5000);
        config.remote.timeout_ms = Some(800);
        config.remote.addr = "not an address".to_string();
        assert_eq!(
            tail(&config.args("1.0.0", "both").unwrap()),
            [
                "--remote-instance",
                "autd3-lab",
                "--discovery-timeout-ms",
                "5000",
                "--remote-timeout-ms",
                "800"
            ]
        );
    }

    #[test]
    fn a_config_saved_before_discovery_switches_to_it_and_keeps_the_address() {
        let config: OtaConfig =
            serde_json::from_str(r#"{"link":"Remote","remote":{"addr":"10.0.0.5:9000"}}"#).unwrap();
        assert!(config.remote.discover);
        assert_eq!(config.remote.addr, "10.0.0.5:9000");
    }

    #[test]
    fn a_saved_config_without_ota_fields_still_loads() {
        let config: OtaConfig = serde_json::from_str(r#"{"link":"Remote"}"#).unwrap();
        assert_eq!(config.link, LinkKind::Remote);
        assert_eq!(config.reboot_wait_secs, 10);
        assert_eq!(config.remote, RemoteConfig::default());
    }
}
