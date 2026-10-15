use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::process::{LogLines, ManagedProcess};

const BIN: &str = "autd3-server";
const DEFAULT_PORT: u16 = 44338;
const MAX_DEVICES: u32 = 128;
const MAX_RECOVERY_INTERVAL_MS: u32 = 60_000;
const DEFAULT_HEARTBEAT_US: u64 = 10_000;
const DEFAULT_REPLY_TIMEOUT_US: u64 = 1_000;
const DEFAULT_LOST_TIMEOUT_MS: u64 = 100;
const DEFAULT_RESPONSE_TIMEOUT_MS: u64 = 200;
const DEFAULT_ENUMERATION_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_SYNC_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_SEND_BUFFER_BYTES: u64 = 8_192;
const DEFAULT_TIMER_RESOLUTION_MS: u64 = 1;
const DEFAULT_MONITOR_INTERVAL_MS: u64 = 1_000;
const DEFAULT_RETRY_INTERVAL_MS: u64 = 1_000;
const MIN_SEND_RATE_LIMIT: f32 = 3.076;
const SUGGESTED_SEND_RATE_LIMIT: f32 = 95.0;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Setting<T> {
    #[default]
    Default,
    Custom(T),
    Off,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub devices: u32,
    pub iface: String,
    pub port: u16,
    pub recovery_interval_ms: u32,
    pub heartbeat_us: Setting<u64>,
    pub reply_timeout_us: Option<u64>,
    pub lost_timeout_ms: Option<u64>,
    pub send_rate_limit: Option<f32>,
    pub response_timeout_ms: Option<u64>,
    pub enumeration_timeout_ms: Option<u64>,
    pub sync_timeout_ms: Option<u64>,
    pub send_buffer_bytes: Setting<u64>,
    pub timer_resolution_ms: Setting<u64>,
    pub monitor_interval_ms: Option<u64>,
    pub retry_interval_ms: Option<u64>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            devices: 0,
            iface: String::new(),
            port: DEFAULT_PORT,
            recovery_interval_ms: 0,
            heartbeat_us: Setting::Default,
            reply_timeout_us: None,
            lost_timeout_ms: None,
            send_rate_limit: None,
            response_timeout_ms: None,
            enumeration_timeout_ms: None,
            sync_timeout_ms: None,
            send_buffer_bytes: Setting::Default,
            timer_resolution_ms: Setting::Default,
            monitor_interval_ms: None,
            retry_interval_ms: None,
        }
    }
}

fn push(args: &mut Vec<String>, flag: &str, value: String) {
    args.push(flag.to_string());
    args.push(value);
}

impl ServerConfig {
    fn args(&self) -> Vec<String> {
        let mut args = vec!["--port".to_string(), self.port.to_string()];
        if self.devices > 0 {
            push(&mut args, "--devices", self.devices.to_string());
        }
        let iface = self.iface.trim();
        if !iface.is_empty() {
            push(&mut args, "--iface", iface.to_string());
        }
        if self.recovery_interval_ms > 0 {
            push(
                &mut args,
                "--recovery-interval",
                format!("{}ms", self.recovery_interval_ms),
            );
        }
        match self.heartbeat_us {
            Setting::Default => {}
            Setting::Custom(us) => push(&mut args, "--heartbeat", format!("{us}us")),
            Setting::Off => args.push("--no-heartbeat".to_string()),
        }
        if let Some(us) = self.reply_timeout_us {
            push(&mut args, "--reply-timeout", format!("{us}us"));
        }
        for (flag, ms) in [
            ("--lost-timeout", self.lost_timeout_ms),
            ("--response-timeout", self.response_timeout_ms),
            ("--enumeration-timeout", self.enumeration_timeout_ms),
            ("--sync-timeout", self.sync_timeout_ms),
        ] {
            if let Some(ms) = ms {
                push(&mut args, flag, format!("{ms}ms"));
            }
        }
        if let Some(percent) = self.send_rate_limit {
            push(&mut args, "--send-rate-limit", percent.to_string());
        }
        match self.send_buffer_bytes {
            Setting::Default => {}
            Setting::Custom(bytes) => push(&mut args, "--send-buffer", bytes.to_string()),
            Setting::Off => push(&mut args, "--send-buffer", "0".to_string()),
        }
        match self.timer_resolution_ms {
            Setting::Default => {}
            Setting::Custom(ms) => push(&mut args, "--timer-resolution", format!("{ms}ms")),
            Setting::Off => args.push("--no-timer-resolution".to_string()),
        }
        for (flag, ms) in [
            ("--monitor-interval", self.monitor_interval_ms),
            ("--retry-interval", self.retry_interval_ms),
        ] {
            if let Some(ms) = ms {
                push(&mut args, flag, format!("{}ms", ms.max(1)));
            }
        }
        args
    }

    fn client_link(&self) -> String {
        format!("ServerLink {{ port: {}, .. }}", self.port)
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("server-config")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| self.basic_ui(ui));
        egui::CollapsingHeader::new("Advanced options")
            .id_salt("server-advanced")
            .show(ui, |ui| {
                egui::Grid::new("server-advanced-grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| self.advanced_ui(ui));
            });
    }

    fn basic_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Devices (0 = all found)");
        ui.add(egui::DragValue::new(&mut self.devices).range(0..=MAX_DEVICES));
        ui.end_row();

        ui.label("Interface (empty = auto)");
        ui.add(egui::TextEdit::singleline(&mut self.iface));
        ui.end_row();

        ui.label("Port");
        ui.add(egui::DragValue::new(&mut self.port).range(1..=65535));
        ui.end_row();

        ui.label("Recovery interval [ms] (0 = off)");
        ui.add(
            egui::DragValue::new(&mut self.recovery_interval_ms)
                .range(0..=MAX_RECOVERY_INTERVAL_MS),
        );
        ui.end_row();

        setting_row(
            ui,
            "Heartbeat",
            &mut self.heartbeat_us,
            DEFAULT_HEARTBEAT_US,
            1,
            " µs",
            "off",
        );
        optional_row(
            ui,
            "Reply timeout",
            &mut self.reply_timeout_us,
            DEFAULT_REPLY_TIMEOUT_US,
            1,
            " µs",
        );
        optional_row(
            ui,
            "Lost timeout",
            &mut self.lost_timeout_ms,
            DEFAULT_LOST_TIMEOUT_MS,
            1,
            " ms",
        );

        ui.label("Send rate limit");
        ui.horizontal(|ui| {
            let mut custom = self.send_rate_limit.is_some();
            if ui.checkbox(&mut custom, "").changed() {
                self.send_rate_limit = custom.then_some(SUGGESTED_SEND_RATE_LIMIT);
            }
            match &mut self.send_rate_limit {
                Some(percent) => {
                    ui.add(
                        egui::DragValue::new(percent)
                            .range(MIN_SEND_RATE_LIMIT..=100.0)
                            .speed(0.1)
                            .suffix(" %"),
                    );
                }
                None => {
                    ui.weak("default (no cap)");
                }
            }
        });
        ui.end_row();
    }

    fn advanced_ui(&mut self, ui: &mut egui::Ui) {
        optional_row(
            ui,
            "Response timeout",
            &mut self.response_timeout_ms,
            DEFAULT_RESPONSE_TIMEOUT_MS,
            1,
            " ms",
        );
        optional_row(
            ui,
            "Enumeration timeout",
            &mut self.enumeration_timeout_ms,
            DEFAULT_ENUMERATION_TIMEOUT_MS,
            0,
            " ms",
        );
        optional_row(
            ui,
            "Sync timeout",
            &mut self.sync_timeout_ms,
            DEFAULT_SYNC_TIMEOUT_MS,
            0,
            " ms",
        );
        setting_row(
            ui,
            "Send buffer",
            &mut self.send_buffer_bytes,
            DEFAULT_SEND_BUFFER_BYTES,
            1,
            " B",
            "OS default",
        );
        setting_row(
            ui,
            "Timer resolution (Windows)",
            &mut self.timer_resolution_ms,
            DEFAULT_TIMER_RESOLUTION_MS,
            1,
            " ms",
            "OS default",
        );
        optional_row(
            ui,
            "Monitor interval",
            &mut self.monitor_interval_ms,
            DEFAULT_MONITOR_INTERVAL_MS,
            1,
            " ms",
        );
        optional_row(
            ui,
            "Retry interval",
            &mut self.retry_interval_ms,
            DEFAULT_RETRY_INTERVAL_MS,
            1,
            " ms",
        );
    }
}

fn optional_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<u64>,
    default: u64,
    min: u64,
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
                ui.add(egui::DragValue::new(v).range(min..=u64::MAX).suffix(suffix));
            }
            None => {
                ui.weak(format!("default ({default}{suffix})"));
            }
        }
    });
    ui.end_row();
}

fn setting_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Setting<u64>,
    default: u64,
    min: u64,
    suffix: &str,
    off: &str,
) {
    ui.label(label);
    ui.horizontal(|ui| {
        let mut choice = match value {
            Setting::Default => Setting::Default,
            Setting::Custom(_) => Setting::Custom(()),
            Setting::Off => Setting::Off,
        };
        let text = |choice: Setting<()>| match choice {
            Setting::Default => format!("default ({default}{suffix})"),
            Setting::Custom(()) => "custom".to_string(),
            Setting::Off => off.to_string(),
        };
        egui::ComboBox::from_id_salt(label)
            .selected_text(text(choice))
            .show_ui(ui, |ui| {
                for option in [Setting::Default, Setting::Custom(()), Setting::Off] {
                    ui.selectable_value(&mut choice, option, text(option));
                }
            });
        *value = match (choice, *value) {
            (Setting::Default, _) => Setting::Default,
            (Setting::Custom(()), Setting::Custom(v)) => Setting::Custom(v),
            (Setting::Custom(()), _) => Setting::Custom(default),
            (Setting::Off, _) => Setting::Off,
        };
        if let Setting::Custom(v) = value {
            ui.add(egui::DragValue::new(v).range(min..=u64::MAX).suffix(suffix));
        }
    });
    ui.end_row();
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
struct Relayed {
    frames: u64,
    replies: u64,
    dropped: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
struct Bus {
    heartbeats: u64,
    missed_heartbeat_replies: u64,
    recoveries: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct Unit {
    id: usize,
    state: Option<String>,
    responding: bool,
    ptp_locked: bool,
    sync_ready: bool,
    firmware: Option<String>,
    ptp_offset_ns: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct Status {
    phase: String,
    error: Option<String>,
    client: Option<String>,
    relayed: Relayed,
    bus: Bus,
    units: Vec<Unit>,
}

fn parse_status(line: &str) -> Option<Status> {
    if !line.starts_with('{') {
        return None;
    }
    serde_json::from_str(line).ok()
}

#[derive(Default)]
struct Output {
    status: Option<Status>,
    logs: LogLines,
}

impl Output {
    fn take(&mut self, line: &str) {
        match parse_status(line) {
            Some(status) => self.status = Some(status),
            None => self.logs.push(line.to_string()),
        }
    }
}

#[derive(Default)]
pub struct ServerPanel {
    pub config: ServerConfig,
    proc: Option<ManagedProcess>,
    output: Output,
    error: Option<String>,
}

fn mark(ui: &mut egui::Ui, good: bool) {
    if good {
        ui.label("yes");
    } else {
        ui.colored_label(egui::Color32::LIGHT_RED, "no");
    }
}

impl ServerPanel {
    pub fn pump(&mut self) {
        if let Some(proc) = &mut self.proc {
            proc.pump_with(|line| {
                self.output.take(line);
                true
            });
        }
    }

    pub fn is_running(&self) -> bool {
        self.proc.as_ref().is_some_and(ManagedProcess::is_running)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let running = self.is_running();

        ui.add_enabled_ui(!running, |ui| self.config.ui(ui));

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
                self.output.status = None;
            }
            ui.label(if running { "running" } else { "stopped" });
            ui.weak(format!(
                "clients connect with {}",
                self.config.client_link()
            ));
        });

        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }

        if running && let Some(status) = &self.output.status {
            ui.separator();
            Self::status_view(ui, status);
        }

        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let logs = self.output.logs.as_slice();
                if logs.is_empty() {
                    ui.weak("no output yet");
                }
                for line in logs {
                    ui.monospace(line);
                }
            });
    }

    fn status_view(ui: &mut egui::Ui, status: &Status) {
        ui.horizontal(|ui| {
            ui.label(format!("phase: {}", status.phase));
            ui.label(format!(
                "client: {}",
                status.client.as_deref().unwrap_or("none")
            ));
            ui.label(format!(
                "frames {} / replies {} / dropped {}",
                status.relayed.frames, status.relayed.replies, status.relayed.dropped
            ));
            ui.label(format!(
                "heartbeats {} / missed replies {} / recoveries {}",
                status.bus.heartbeats, status.bus.missed_heartbeat_replies, status.bus.recoveries
            ));
        });
        if let Some(error) = &status.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        if status.units.is_empty() {
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("server-units")
            .max_height(160.0)
            .show(ui, |ui| {
                egui::Grid::new("server-units")
                    .num_columns(7)
                    .striped(true)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for title in [
                            "ID",
                            "State",
                            "Responding",
                            "PTP locked",
                            "Sync ready",
                            "Firmware",
                            "PTP offset [ns]",
                        ] {
                            ui.strong(title);
                        }
                        ui.end_row();
                        for unit in &status.units {
                            ui.label(unit.id.to_string());
                            match unit.state.as_deref() {
                                Some("lost") => {
                                    ui.colored_label(egui::Color32::LIGHT_RED, "lost");
                                }
                                Some(state) => {
                                    ui.label(state);
                                }
                                None => {
                                    ui.weak("-");
                                }
                            }
                            mark(ui, unit.responding);
                            mark(ui, unit.ptp_locked);
                            mark(ui, unit.sync_ready);
                            ui.label(unit.firmware.as_deref().unwrap_or("-"));
                            ui.label(
                                unit.ptp_offset_ns
                                    .map_or_else(|| "-".to_string(), |offset| offset.to_string()),
                            );
                            ui.end_row();
                        }
                    });
            });
    }

    fn start(&mut self) {
        self.error = None;
        self.output = Output::default();
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
    fn the_default_config_only_names_the_port() {
        assert_eq!(ServerConfig::default().args(), ["--port", "44338"]);
        assert_eq!(
            ServerConfig::default().client_link(),
            "ServerLink { port: 44338, .. }"
        );
    }

    #[test]
    fn every_field_reaches_the_arguments() {
        let config = ServerConfig {
            devices: 4,
            iface: " eth0 ".to_string(),
            port: 5000,
            recovery_interval_ms: 100,
            heartbeat_us: Setting::Custom(20_000),
            reply_timeout_us: Some(1_500),
            lost_timeout_ms: Some(150),
            send_rate_limit: Some(95.5),
            response_timeout_ms: Some(300),
            enumeration_timeout_ms: Some(20_000),
            sync_timeout_ms: Some(60_000),
            send_buffer_bytes: Setting::Custom(4_096),
            timer_resolution_ms: Setting::Custom(2),
            monitor_interval_ms: Some(250),
            retry_interval_ms: Some(3_000),
        };
        assert_eq!(
            config.args(),
            [
                "--port",
                "5000",
                "--devices",
                "4",
                "--iface",
                "eth0",
                "--recovery-interval",
                "100ms",
                "--heartbeat",
                "20000us",
                "--reply-timeout",
                "1500us",
                "--lost-timeout",
                "150ms",
                "--response-timeout",
                "300ms",
                "--enumeration-timeout",
                "20000ms",
                "--sync-timeout",
                "60000ms",
                "--send-rate-limit",
                "95.5",
                "--send-buffer",
                "4096",
                "--timer-resolution",
                "2ms",
                "--monitor-interval",
                "250ms",
                "--retry-interval",
                "3000ms",
            ]
        );
        assert_eq!(config.client_link(), "ServerLink { port: 5000, .. }");
    }

    #[test]
    fn the_settings_turned_off_map_to_their_off_arguments() {
        let config = ServerConfig {
            heartbeat_us: Setting::Off,
            send_buffer_bytes: Setting::Off,
            timer_resolution_ms: Setting::Off,
            ..ServerConfig::default()
        };
        assert_eq!(
            config.args(),
            [
                "--port",
                "44338",
                "--no-heartbeat",
                "--send-buffer",
                "0",
                "--no-timer-resolution"
            ]
        );
    }

    #[test]
    fn a_config_saved_before_the_advanced_options_still_loads() {
        let config: ServerConfig = serde_json::from_str(
            r#"{"devices":4,"iface":"eth0","port":5000,"recovery_interval_ms":100}"#,
        )
        .unwrap();
        assert_eq!(
            config.args(),
            [
                "--port",
                "5000",
                "--devices",
                "4",
                "--iface",
                "eth0",
                "--recovery-interval",
                "100ms"
            ]
        );
    }

    #[test]
    fn a_long_running_server_keeps_status_lines_out_of_a_bounded_log() {
        let capacity = std::num::NonZeroUsize::new(5).unwrap();
        let mut output = Output {
            status: None,
            logs: LogLines::new(capacity),
        };
        for i in 0..1_000 {
            output.take(&format!(
                concat!(
                    r#"{{"phase":"serving","error":null,"client":null,"#,
                    r#""relayed":{{"frames":{},"replies":0,"dropped":0}},"#,
                    r#""bus":{{"heartbeats":0,"missed_heartbeat_replies":0,"recoveries":0}},"#,
                    r#""units":[]}}"#
                ),
                i
            ));
            if i % 10 == 0 {
                output.take(&format!("INFO line {i}"));
            }
            assert!(output.logs.as_slice().len() <= capacity.get());
        }
        assert_eq!(
            output.logs.as_slice(),
            [
                "INFO line 950",
                "INFO line 960",
                "INFO line 970",
                "INFO line 980",
                "INFO line 990"
            ]
        );
        assert_eq!(output.status.unwrap().relayed.frames, 999);
    }

    #[test]
    fn a_zero_interval_saved_before_the_floor_is_raised_to_one_millisecond() {
        let config = ServerConfig {
            monitor_interval_ms: Some(0),
            retry_interval_ms: Some(0),
            ..ServerConfig::default()
        };
        assert_eq!(
            config.args(),
            [
                "--port",
                "44338",
                "--monitor-interval",
                "1ms",
                "--retry-interval",
                "1ms"
            ]
        );
    }

    #[test]
    fn status_lines_feed_the_table_and_the_rest_feeds_the_log() {
        let lines = [
            "INFO enumerating the devices".to_string(),
            concat!(
                r#"{"phase":"serving","error":null,"client":"[::1]:5000","#,
                r#""relayed":{"frames":4,"replies":2,"dropped":1},"#,
                r#""bus":{"heartbeats":9,"missed_heartbeat_replies":1,"recoveries":2},"#,
                r#""units":[{"id":0,"state":"syncing","responding":true,"ptp_locked":true,"sync_ready":false,"firmware":"0.10.1","ptp_offset_ns":-3},"#,
                r#"{"id":1,"state":null,"responding":false,"ptp_locked":false,"sync_ready":false,"firmware":null,"ptp_offset_ns":null}]}"#
            )
            .to_string(),
            "{not a status".to_string(),
        ];
        let mut output = Output::default();
        for line in &lines {
            output.take(line);
        }

        assert_eq!(
            output.logs.as_slice(),
            ["INFO enumerating the devices", "{not a status"]
        );
        let status = output.status.unwrap();
        assert_eq!(status.phase, "serving");
        assert_eq!(status.client.as_deref(), Some("[::1]:5000"));
        assert_eq!(
            status.relayed,
            Relayed {
                frames: 4,
                replies: 2,
                dropped: 1
            }
        );
        assert_eq!(
            status.bus,
            Bus {
                heartbeats: 9,
                missed_heartbeat_replies: 1,
                recoveries: 2
            }
        );
        assert_eq!(status.units[0].state.as_deref(), Some("syncing"));
        assert_eq!(status.units[1].state, None);
        assert_eq!(status.units.len(), 2);
        assert_eq!(status.units[0].firmware.as_deref(), Some("0.10.1"));
        assert_eq!(status.units[0].ptp_offset_ns, Some(-3));
        assert!(!status.units[1].responding);
    }
}
