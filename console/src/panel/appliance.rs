use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use autd3_rs_appliance::{
    ApplianceClient, ApplianceStatus, BusActual, UNKNOWN_STATE_HINT, UplinkStatus,
};
use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::launch::tool_bin;
use crate::process::ManagedProcess;

const SUBDIR: &str = "appliance";
const BIN: &str = "autd3-appliance";
const POLL_PERIOD: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct ApplianceConfig {
    pub addr: String,
}

enum Outcome {
    Status(Box<ApplianceStatus>),
    Message(String),
    Failed(String),
}

#[derive(Default)]
pub struct AppliancePanel {
    pub config: ApplianceConfig,
    scan: Option<ManagedProcess>,
    inflight: Option<Receiver<Outcome>>,
    acting: bool,
    polled: Option<Instant>,
    status: Option<ApplianceStatus>,
    message: Option<String>,
    error: Option<String>,
}

impl AppliancePanel {
    pub fn pump(&mut self, visible: bool) -> Option<Duration> {
        self.pump_scan();
        self.pump_inflight();
        if !visible || self.config.addr.trim().is_empty() || self.inflight.is_some() {
            return None;
        }
        if self.polled.is_none_or(|at| at.elapsed() >= POLL_PERIOD) {
            self.request(false, |client| {
                client
                    .status()
                    .map(|status| Outcome::Status(Box::new(status)))
            });
            return None;
        }
        self.polled
            .map(|at| POLL_PERIOD.saturating_sub(at.elapsed()))
    }

    pub fn is_running(&self) -> bool {
        self.scan.as_ref().is_some_and(ManagedProcess::is_running) || self.inflight.is_some()
    }

    fn busy(&self) -> bool {
        self.scan.as_ref().is_some_and(ManagedProcess::is_running) || self.acting
    }

    fn pump_scan(&mut self) {
        let Some(scan) = &mut self.scan else {
            return;
        };
        scan.pump();
        if scan.is_running() {
            return;
        }
        let output = scan.logs().join("\n");
        self.scan = None;
        match first_control_endpoint(&output) {
            Some(addr) => {
                self.config.addr = addr;
                self.message = Some(format!("found {}", self.config.addr));
                self.polled = None;
            }
            None => self.error = Some("no appliance answered".to_owned()),
        }
    }

    fn pump_inflight(&mut self) {
        let Some(rx) = &self.inflight else {
            return;
        };
        let outcome = match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => None,
        };
        self.inflight = None;
        self.acting = false;
        self.polled = Some(Instant::now());
        match outcome {
            Some(Outcome::Status(status)) => {
                self.status = Some(*status);
                self.error = None;
            }
            Some(Outcome::Message(message)) => {
                self.message = Some(message);
                self.polled = None;
            }
            Some(Outcome::Failed(error)) => self.error = Some(error),
            None => {}
        }
    }

    fn request<F>(&mut self, announce: bool, call: F)
    where
        F: FnOnce(&ApplianceClient) -> Result<Outcome, autd3_rs_appliance::ClientError>
            + Send
            + 'static,
    {
        let base = base_url(&self.config.addr);
        self.acting = announce;
        if announce {
            self.message = None;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let client = ApplianceClient::with_base_and_timeout(base, REQUEST_TIMEOUT);
            let outcome = call(&client).unwrap_or_else(|e| Outcome::Failed(e.to_string()));
            let _ = tx.send(outcome);
        });
        self.inflight = Some(rx);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let busy = self.busy();

        ui.horizontal(|ui| {
            ui.label("Address");
            ui.add(
                egui::TextEdit::singleline(&mut self.config.addr)
                    .hint_text("autd3-xxxxxxxx.local:8081")
                    .desired_width(240.0),
            );
            if ui
                .add_enabled(self.scan.is_none(), egui::Button::new("Scan"))
                .clicked()
            {
                self.scan();
            }
            if !self.config.addr.trim().is_empty() {
                ui.hyperlink_to("Open in browser", base_url(&self.config.addr));
            }
        });

        ui.separator();

        match &self.status {
            Some(status) => status_view(ui, status),
            None if self.scan.is_some() => {
                ui.weak("scanning...");
            }
            None => {
                ui.weak("no appliance connected");
            }
        }

        ui.separator();
        let connected = self.status.is_some();
        let admin = self.status.as_ref().is_some_and(|s| s.allow_admin);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(connected && !busy, egui::Button::new("Open"))
                .clicked()
            {
                self.act(|client| client.bus_open().map(|a| Outcome::Message(a.message)));
            }
            if ui
                .add_enabled(connected && !busy, egui::Button::new("Close"))
                .clicked()
            {
                self.act(|client| client.bus_close().map(|a| Outcome::Message(a.message)));
            }
            if ui
                .add_enabled(connected && !busy, egui::Button::new("Probe"))
                .clicked()
            {
                self.act(|client| {
                    client.bus_probe().map(|r| {
                        Outcome::Message(format!("{} device(s) on the bus", r.num_devices))
                    })
                });
            }
            if ui
                .add_enabled(connected && !busy, egui::Button::new("Restart server"))
                .clicked()
            {
                self.act(|client| client.restart().map(|a| Outcome::Message(a.message)));
            }
            if ui
                .add_enabled(connected && admin && !busy, egui::Button::new("Reboot"))
                .clicked()
            {
                self.act(|client| client.reboot().map(|a| Outcome::Message(a.message)));
            }
            if ui
                .add_enabled(connected && admin && !busy, egui::Button::new("Shut down"))
                .clicked()
            {
                self.act(|client| client.shutdown().map(|a| Outcome::Message(a.message)));
            }
        });

        if let Some(message) = &self.message {
            ui.label(message);
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
    }

    fn act<F>(&mut self, call: F)
    where
        F: FnOnce(&ApplianceClient) -> Result<Outcome, autd3_rs_appliance::ClientError>
            + Send
            + 'static,
    {
        self.request(true, call);
    }

    fn scan(&mut self) {
        self.error = None;
        self.message = None;
        let bin = match tool_bin(SUBDIR, BIN) {
            Ok(bin) => bin,
            Err(e) => {
                self.error = Some(format!("cannot resolve {BIN}: {e}"));
                return;
            }
        };
        match ManagedProcess::spawn(&bin, &["scan".to_owned(), "--json".to_owned()]) {
            Ok(proc) => self.scan = Some(proc),
            Err(e) => self.error = Some(super::spawn_error(&bin, &e)),
        }
    }
}

fn base_url(addr: &str) -> String {
    let addr = addr.trim();
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_owned()
    } else {
        format!("http://{addr}")
    }
}

fn first_control_endpoint(output: &str) -> Option<String> {
    let first = output.match_indices('[').find_map(|(start, _)| {
        serde_json::Deserializer::from_str(&output[start..])
            .into_iter::<serde_json::Value>()
            .next()?
            .ok()?
            .as_array()?
            .first()
            .cloned()
    })?;
    let addr = first.get("addr")?.as_str()?;
    let port = first
        .get("control_port")?
        .as_u64()
        .unwrap_or(u64::from(autd3_rs_appliance::DEFAULT_CONTROL_PORT));
    let host = addr.rsplit_once(':')?.0;
    Some(format!("{host}:{port}"))
}

fn status_view(ui: &mut egui::Ui, status: &ApplianceStatus) {
    egui::Grid::new("appliance-status")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Instance");
            ui.label(&status.instance);
            ui.end_row();

            ui.label("Versions");
            ui.label(format!(
                "autd3-sdk {} / wire {}",
                status.sdk_version, status.wire_version,
            ));
            ui.end_row();

            if let Some(image) = &status.image {
                ui.label("Image");
                ui.label(format!(
                    "{} (built {}, autd3-sdk {})",
                    image.version, image.built, image.sdk_version,
                ));
                ui.end_row();
            }

            if let Some(binary) = &status.binary {
                ui.label("Binary");
                ui.label(binary);
                ui.end_row();
            }

            ui.label("Uptime");
            ui.label(human_duration(status.uptime_secs));
            ui.end_row();

            let bus = &status.bus;
            ui.label("Bus");
            let healthy = bus.actual == BusActual::Open && bus.devices.iter().all(|d| d == "OP");
            ui.colored_label(
                if healthy {
                    egui::Color32::LIGHT_GREEN
                } else {
                    egui::Color32::LIGHT_RED
                },
                match (&bus.failure, bus.has_unknown_state()) {
                    (Some(reason), _) => format!("{:?}: {reason}", bus.actual),
                    (None, true) => format!(
                        "{:?} (requested {:?}) [{UNKNOWN_STATE_HINT}]",
                        bus.actual, bus.desired
                    ),
                    (None, false) => format!("{:?} (requested {:?})", bus.actual, bus.desired),
                },
            );
            ui.end_row();

            ui.label("Devices");
            ui.label(format!("{} [{}]", bus.num_devices, bus.devices.join(", ")));
            ui.end_row();

            ui.label("Counters");
            ui.label(format!(
                "recoveries {} / stale {} / lost {} / phase excursions {}",
                bus.recoveries, bus.stale_cycles, bus.lost_cycles, bus.phase_excursions,
            ));
            ui.end_row();

            if bus.exchanges > 0 {
                ui.label("Exchange");
                ui.label(format!(
                    "mean {} us / worst {} us",
                    bus.exchange_mean_ns / 1_000,
                    bus.exchange_worst_ns / 1_000,
                ));
                ui.end_row();
            }

            ui.label("EtherCAT port");
            ui.label(format!(
                "{} {}",
                status.interface.name,
                if status.interface.carrier {
                    "up"
                } else {
                    status.interface.operstate.as_str()
                },
            ));
            ui.end_row();

            for uplink in &status.uplinks {
                ui.label(format!("Uplink {}", uplink.name));
                ui.label(uplink_line(uplink));
                ui.end_row();
            }

            if let Some(storage) = &status.storage {
                ui.label("Storage");
                ui.label(format!(
                    "{} {} MB free of {} MB",
                    storage.path, storage.free_mb, storage.total_mb,
                ));
                ui.end_row();
            }

            ui.label("Client");
            ui.label(status.client.as_ref().map_or_else(
                || "none".to_owned(),
                |client| format!("{} ({} devices)", client.peer, client.devices),
            ));
            ui.end_row();
        });
}

fn uplink_line(uplink: &UplinkStatus) -> String {
    let mut parts = vec![if uplink.carrier {
        "up".to_owned()
    } else {
        uplink.operstate.clone()
    }];
    if let Some(wifi) = &uplink.wifi {
        parts.push(match (&wifi.ssid, wifi.signal_dbm) {
            (Some(ssid), Some(dbm)) => format!("{ssid} ({dbm} dBm)"),
            (Some(ssid), None) => ssid.clone(),
            (None, _) if wifi.blocked => "radio blocked".to_owned(),
            (None, _) => "not associated".to_owned(),
        });
        parts.push(format!(
            "domain {}",
            wifi.regdomain.as_deref().unwrap_or("unset"),
        ));
    }
    parts.push(if uplink.addresses.is_empty() {
        "no address".to_owned()
    } else {
        uplink.addresses.join(", ")
    });
    parts.join(" / ")
}

fn human_duration(secs: u64) -> String {
    let (days, hours, minutes) = (secs / 86_400, (secs % 86_400) / 3_600, (secs % 3_600) / 60);
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m {}s", secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(addr: &str) -> AppliancePanel {
        AppliancePanel {
            config: ApplianceConfig {
                addr: addr.to_owned(),
            },
            ..AppliancePanel::default()
        }
    }

    #[test]
    fn a_hidden_panel_never_reaches_the_appliance() {
        let mut hidden = panel("127.0.0.1:1");
        assert_eq!(hidden.pump(false), None);
        assert!(!hidden.is_running());

        let mut addressless = panel("   ");
        assert_eq!(addressless.pump(true), None);
        assert!(!addressless.is_running());
    }

    #[test]
    fn a_visible_panel_keeps_asking_to_be_woken_without_any_user_input() {
        let mut panel = panel("127.0.0.1:1");
        assert_eq!(panel.pump(true), None, "the first pump starts a poll");
        assert!(panel.is_running(), "the in-flight poll drives the repaint");

        let deadline = Instant::now() + Duration::from_secs(10);
        let next = loop {
            if let Some(next) = panel.pump(true) {
                break next;
            }
            assert!(
                Instant::now() < deadline,
                "a settled panel must schedule its own next poll, not wait for the mouse",
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(next <= POLL_PERIOD, "{next:?}");
    }

    #[test]
    fn a_reply_slower_than_the_poll_period_does_not_start_the_next_poll_at_once() {
        let mut panel = panel("127.0.0.1:1");
        let (tx, rx) = channel();
        panel.inflight = Some(rx);
        panel.polled = Instant::now().checked_sub(POLL_PERIOD * 4);
        assert_eq!(panel.pump(true), None, "the reply is still on its way");

        tx.send(Outcome::Failed("unreachable".to_owned())).unwrap();
        let next = panel.pump(true).expect("the interval runs between polls");
        assert!(next > Duration::ZERO && next <= POLL_PERIOD, "{next:?}");
        assert!(!panel.is_running(), "the appliance must get a breather");
    }

    #[test]
    fn a_background_poll_does_not_grey_out_the_buttons() {
        let mut panel = panel("127.0.0.1:1");
        panel.pump(true);
        assert!(panel.is_running());
        assert!(!panel.busy(), "only a user action may disable the controls");

        panel.act(|client| client.bus_open().map(|a| Outcome::Message(a.message)));
        assert!(panel.busy());
    }

    #[test]
    fn the_scan_output_yields_the_control_endpoint_not_the_relay_one() {
        let output = "using autd3-99c06885\n[\n  {\n    \"addr\": \"169.254.1.5:8080\",\n \
             \"control_port\": 8081,\n    \"instance\": \"autd3-0a1b2c3d\"\n  }\n]";
        assert_eq!(
            first_control_endpoint(output).as_deref(),
            Some("169.254.1.5:8081"),
        );
    }

    #[test]
    fn an_ipv6_endpoint_keeps_its_brackets() {
        let output = "[{\"addr\": \"[fe80::1%3]:8080\", \"control_port\": 8081}]";
        assert_eq!(
            first_control_endpoint(output).as_deref(),
            Some("[fe80::1%3]:8081"),
        );
    }

    #[test]
    fn the_exit_marker_appended_to_the_logs_does_not_hide_the_endpoint() {
        let output = "[\n  {\n    \"addr\": \"169.254.1.5:8080\",\n    \"control_port\": 8081\n  \
             }\n]\n[process finished]";
        assert_eq!(
            first_control_endpoint(output).as_deref(),
            Some("169.254.1.5:8081"),
        );
    }

    #[test]
    fn an_empty_scan_finds_nothing() {
        assert_eq!(first_control_endpoint("[]"), None);
        assert_eq!(first_control_endpoint("no appliance answered"), None);
    }

    #[test]
    fn the_address_field_takes_a_bare_host_or_a_url() {
        assert_eq!(base_url(" autd3.local:8081 "), "http://autd3.local:8081");
        assert_eq!(base_url("http://10.0.0.2:8081"), "http://10.0.0.2:8081");
    }
}
