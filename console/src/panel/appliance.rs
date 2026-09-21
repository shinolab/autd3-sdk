use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use autd3_rs_appliance::release::{self, ServerRelease};
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
const RELEASE_TIMEOUT: Duration = Duration::from_secs(20);
const RESTART_GRACE: Duration = Duration::from_secs(60);

const BINARY_ONLY_NOTE: &str = "Only the server binary is replaced. The systemd unit, the admin \
     helper and the Wi-Fi scripts stay as the image shipped them; write the image to update those.";

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplianceConfig {
    pub addr: String,
    pub version: Option<String>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
enum Listing {
    #[default]
    Idle,
    Due,
    Done,
}

enum Outcome {
    Status(Box<ApplianceStatus>),
    Message(String),
    Releases(Result<Vec<ServerRelease>, String>),
    Failed(String),
}

#[derive(Default)]
pub struct AppliancePanel {
    pub config: ApplianceConfig,
    scan: Option<ManagedProcess>,
    inflight: Option<Receiver<Outcome>>,
    release_job: Option<Receiver<Outcome>>,
    acting: bool,
    polled: Option<Instant>,
    status: Option<ApplianceStatus>,
    message: Option<String>,
    error: Option<String>,
    releases: Vec<ServerRelease>,
    release_error: Option<String>,
    listing: Listing,
    updating: bool,
    awaiting: Option<String>,
    confirm: Option<ServerRelease>,
    quiet_until: Option<Instant>,
}

impl AppliancePanel {
    pub fn pump(&mut self, visible: bool) -> Option<Duration> {
        self.pump_scan();
        self.pump_inflight();
        self.pump_release_job();
        if !visible || self.config.addr.trim().is_empty() || self.inflight.is_some() {
            return None;
        }
        if self.listing == Listing::Due {
            self.refresh_releases();
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
        self.scan.as_ref().is_some_and(ManagedProcess::is_running)
            || self.inflight.is_some()
            || self.release_job.is_some()
    }

    fn busy(&self) -> bool {
        self.scan.as_ref().is_some_and(ManagedProcess::is_running) || self.acting
    }

    fn running_version(&self) -> Option<&str> {
        self.status
            .as_ref()
            .map(|status| status.sdk_version.as_str())
    }

    fn picked(&self) -> Option<&ServerRelease> {
        let version = self.config.version.as_deref()?;
        self.releases
            .iter()
            .find(|release| release.version == version)
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
                let installed = self
                    .awaiting
                    .as_ref()
                    .is_none_or(|version| status.sdk_version == *version);
                if installed {
                    self.awaiting = None;
                    self.quiet_until = None;
                }
                self.status = Some(*status);
                self.error = None;
                if self.listing == Listing::Idle {
                    self.listing = Listing::Due;
                }
            }
            Some(Outcome::Message(message)) => {
                self.message = Some(message);
                if self.updating {
                    self.updating = false;
                    self.quiet_until = Some(Instant::now() + RESTART_GRACE);
                } else {
                    self.polled = None;
                }
            }
            Some(Outcome::Releases(_)) => {
                self.error = Some("a release listing answered an appliance request".to_owned());
            }
            Some(Outcome::Failed(error)) => {
                self.updating = false;
                if self.restarting() {
                    self.message = Some("the server is restarting...".to_owned());
                } else {
                    self.awaiting = None;
                    self.error = Some(error);
                }
            }
            None => {
                self.updating = false;
                self.error = Some("the request stopped without an answer".to_owned());
            }
        }
    }

    fn pump_release_job(&mut self) {
        let Some(rx) = &self.release_job else {
            return;
        };
        let outcome = match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => None,
        };
        self.release_job = None;
        match outcome {
            Some(Outcome::Releases(Ok(releases))) => {
                self.release_error = None;
                self.releases = releases;
                if self.picked().is_none() {
                    self.config.version =
                        self.releases.first().map(|release| release.version.clone());
                }
            }
            Some(Outcome::Releases(Err(error))) => self.release_error = Some(error),
            _ => {
                self.release_error =
                    Some("the release listing stopped without an answer".to_owned());
            }
        }
    }

    fn restarting(&self) -> bool {
        self.quiet_until.is_some_and(|until| Instant::now() < until)
    }

    fn request<F>(&mut self, announce: bool, call: F)
    where
        F: FnOnce(&ApplianceClient) -> Result<Outcome, autd3_rs_appliance::ClientError>
            + Send
            + 'static,
    {
        let base = base_url(&self.config.addr);
        self.spawn(announce, move || {
            let client = ApplianceClient::with_base_and_timeout(base, REQUEST_TIMEOUT);
            call(&client).unwrap_or_else(|e| Outcome::Failed(e.to_string()))
        });
    }

    fn spawn<F>(&mut self, announce: bool, job: F)
    where
        F: FnOnce() -> Outcome + Send + 'static,
    {
        self.acting = announce;
        if announce {
            self.message = None;
        }
        self.inflight = Some(detach(job));
    }

    fn refresh_releases(&mut self) {
        self.release_error = None;
        self.listing = Listing::Done;
        self.release_job = Some(detach(|| {
            Outcome::Releases(release::list(RELEASE_TIMEOUT).map_err(|e| detail(&e)))
        }));
    }

    fn start_update(&mut self, release: ServerRelease) {
        let base = base_url(&self.config.addr);
        self.error = None;
        self.updating = true;
        self.awaiting = Some(release.version.clone());
        self.spawn(true, move || {
            let binary = match release::download(&release, release::DEFAULT_TIMEOUT) {
                Ok(binary) => binary,
                Err(e) => return Outcome::Failed(detail(&e)),
            };
            let client = ApplianceClient::with_base_and_timeout(base, REQUEST_TIMEOUT);
            match client.update(&binary) {
                Ok(accepted) => Outcome::Message(accepted.message),
                Err(e) => Outcome::Failed(e.to_string()),
            }
        });
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

        let available = self
            .running_version()
            .and_then(|running| newest_update(running, &self.releases))
            .map(|release| release.version.clone());

        match &self.status {
            Some(status) => status_view(ui, status, available.as_deref()),
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

        ui.separator();
        self.update_ui(ui, connected, admin, busy);

        if let Some(message) = &self.message {
            ui.label(message);
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }

        self.confirm_ui(ui);
    }

    fn update_ui(&mut self, ui: &mut egui::Ui, connected: bool, admin: bool, busy: bool) {
        let picked = self.picked().cloned();
        ui.horizontal(|ui| {
            ui.label("Server update");
            let selected = self
                .config
                .version
                .clone()
                .unwrap_or_else(|| "(none)".to_owned());
            ui.add_enabled_ui(!busy && !self.releases.is_empty(), |ui| {
                egui::ComboBox::from_id_salt("appliance-server-version")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for release in &self.releases {
                            ui.selectable_value(
                                &mut self.config.version,
                                Some(release.version.clone()),
                                &release.version,
                            );
                        }
                    });
            });
            if ui
                .add_enabled(
                    !busy && self.inflight.is_none(),
                    egui::Button::new("Refresh"),
                )
                .clicked()
            {
                self.refresh_releases();
            }
            if ui
                .add_enabled(
                    connected && admin && !busy && picked.is_some(),
                    egui::Button::new("Update"),
                )
                .clicked()
            {
                self.confirm = picked;
            }
            if self.updating {
                ui.weak("downloading and uploading...");
            }
        });

        if let Some(error) = &self.release_error {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("cannot read the published releases: {error}"),
            );
        } else if self.release_job.is_some() {
            ui.weak("reading the published releases...");
        } else if self.listing != Listing::Done {
            ui.weak("the published releases are read once the appliance answers");
        } else if self.releases.is_empty() {
            ui.weak("no release publishes a server binary for the appliance yet");
        }
        ui.weak(BINARY_ONLY_NOTE);
    }

    fn confirm_ui(&mut self, ui: &egui::Ui) {
        let Some(release) = self.confirm.clone() else {
            return;
        };
        let running = self.running_version().unwrap_or("?").to_owned();
        let mut start = false;
        let mut close = false;
        let modal =
            egui::Modal::new(egui::Id::new("appliance-update-confirm")).show(ui.ctx(), |ui| {
                ui.set_width(460.0);
                ui.heading(format!(
                    "Update the appliance server to {}?",
                    release.version
                ));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.monospace(&running);
                    ui.monospace(format!("-> {}", release.version));
                    ui.weak(format!("({} bytes)", release.size));
                });
                ui.add_space(4.0);
                ui.colored_label(
                    egui::Color32::LIGHT_YELLOW,
                    "The server installs the binary and restarts, so the bus closes and a \
                     connected client loses the appliance.",
                );
                ui.add_space(4.0);
                ui.weak(BINARY_ONLY_NOTE);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Update").clicked() {
                        start = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if start {
            self.confirm = None;
            self.start_update(release);
        } else if close || modal.should_close() {
            self.confirm = None;
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

fn detach<F>(job: F) -> Receiver<Outcome>
where
    F: FnOnce() -> Outcome + Send + 'static,
{
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(job());
    });
    rx
}

fn detail(error: &dyn std::error::Error) -> String {
    use std::fmt::Write;

    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let _ = write!(text, ": {cause}");
        source = cause.source();
    }
    text
}

fn base_url(addr: &str) -> String {
    let addr = addr.trim();
    let (scheme, host) = match addr.split_once("://") {
        Some((scheme, rest)) => (scheme, rest),
        None => ("http", addr),
    };
    let host = host
        .parse::<std::net::SocketAddr>()
        .map_or_else(|_| host.to_owned(), autd3_rs_appliance::host_of);
    format!("{scheme}://{host}")
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
    let port = first
        .get("control_port")?
        .as_u64()
        .unwrap_or(u64::from(autd3_rs_appliance::DEFAULT_CONTROL_PORT));

    if let Some(host) = first.get("host").and_then(|host| host.as_str()) {
        let host = host.trim_end_matches('.');
        if !host.is_empty() {
            return Some(format!("{host}:{port}"));
        }
    }

    let addr = first.get("addr")?.as_str()?;
    let host = addr.rsplit_once(':')?.0;
    Some(format!("{host}:{port}"))
}

fn newest_update<'a>(running: &str, releases: &'a [ServerRelease]) -> Option<&'a ServerRelease> {
    releases
        .iter()
        .filter(|release| release::is_newer(&release.version, running))
        .max_by_key(|release| release::version_key(&release.version))
}

fn status_view(ui: &mut egui::Ui, status: &ApplianceStatus, available: Option<&str>) {
    egui::Grid::new("appliance-status")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            identity_rows(ui, status, available);
            bus_rows(ui, status);
        });
}

fn identity_rows(ui: &mut egui::Ui, status: &ApplianceStatus, available: Option<&str>) {
    ui.label("Instance");
    ui.label(&status.instance);
    ui.end_row();

    ui.label("Versions");
    ui.horizontal(|ui| {
        ui.label(format!(
            "autd3-sdk {} / wire {}",
            status.sdk_version, status.wire_version,
        ));
        match available {
            Some(version) => ui.colored_label(
                egui::Color32::LIGHT_YELLOW,
                format!("({version} available)"),
            ),
            None => ui.weak("(up to date)"),
        };
    });
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
}

fn bus_rows(ui: &mut egui::Ui, status: &ApplianceStatus) {
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
                ..ApplianceConfig::default()
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
    fn the_mdns_name_is_preferred_over_a_link_local_address() {
        let output = "[{\"addr\": \"[fe80::1%3]:8080\", \"host\": \"autd3-0a1b2c3d.local.\", \
             \"control_port\": 8081}]";
        assert_eq!(
            first_control_endpoint(output).as_deref(),
            Some("autd3-0a1b2c3d.local:8081"),
            "a zone index cannot be dialled from a browser; the name can",
        );
    }

    #[test]
    fn a_zone_index_reaches_the_browser_percent_encoded() {
        assert_eq!(
            base_url("[fe80::1%3]:8081"),
            "http://[fe80::1%253]:8081",
            "a bare `%3]` is not a legal percent-escape, so the URI never parses",
        );
        assert_eq!(base_url("[2001:db8::1]:8081"), "http://[2001:db8::1]:8081");
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

    fn release(version: &str) -> ServerRelease {
        ServerRelease {
            version: version.to_owned(),
            tag: format!("appliance-v{version}"),
            size: 6_749_168,
            ..ServerRelease::default()
        }
    }

    fn status(version: &str) -> ApplianceStatus {
        ApplianceStatus {
            instance: "autd3".to_owned(),
            sdk_version: version.to_owned(),
            wire_version: 3,
            uptime_secs: 1,
            allow_admin: true,
            bus: autd3_rs_appliance::BusStatus::default(),
            binary: None,
            interface: autd3_rs_appliance::InterfaceStatus::default(),
            uplinks: Vec::new(),
            storage: None,
            client: None,
            image: None,
        }
    }

    fn deliver(panel: &mut AppliancePanel, outcome: Outcome) {
        let (tx, rx) = channel();
        tx.send(outcome).unwrap();
        panel.inflight = Some(rx);
        panel.pump_inflight();
    }

    fn deliver_releases(panel: &mut AppliancePanel, releases: Result<Vec<ServerRelease>, String>) {
        let (tx, rx) = channel();
        tx.send(Outcome::Releases(releases)).unwrap();
        panel.release_job = Some(rx);
        panel.pump_release_job();
    }

    #[test]
    fn a_config_saved_before_the_update_control_existed_still_loads() {
        let config: ApplianceConfig = serde_json::from_str(r#"{"addr":"autd3.local:8081"}"#)
            .expect("the panel must keep reading the settings it wrote before");
        assert_eq!(config.addr, "autd3.local:8081");
        assert_eq!(config.version, None);
    }

    #[test]
    fn only_a_release_newer_than_the_running_server_is_announced() {
        let releases = [release("0.11.0"), release("0.10.0"), release("0.9.0")];
        assert_eq!(
            newest_update("0.9.0", &releases).map(|r| r.version.as_str()),
            Some("0.11.0"),
        );
        assert_eq!(
            newest_update("0.10.0", &releases).map(|r| r.version.as_str()),
            Some("0.11.0"),
        );
        assert!(newest_update("0.11.0", &releases).is_none());
        assert!(newest_update("1.0.0", &releases).is_none());
        assert!(newest_update("0.9.0", &[]).is_none());
    }

    #[test]
    fn the_listing_picks_the_newest_version_and_keeps_a_version_the_user_chose() {
        let mut panel = panel("127.0.0.1:1");
        deliver_releases(&mut panel, Ok(vec![release("0.11.0"), release("0.10.0")]));
        assert_eq!(panel.config.version.as_deref(), Some("0.11.0"));
        assert_eq!(panel.picked().map(|r| r.version.as_str()), Some("0.11.0"));

        panel.config.version = Some("0.10.0".to_owned());
        deliver_releases(&mut panel, Ok(vec![release("0.11.0"), release("0.10.0")]));
        assert_eq!(panel.config.version.as_deref(), Some("0.10.0"));
    }

    #[test]
    fn a_saved_version_that_is_no_longer_published_is_dropped() {
        let mut panel = panel("127.0.0.1:1");
        panel.config.version = Some("0.10.0".to_owned());
        deliver_releases(&mut panel, Ok(vec![release("0.11.0"), release("0.10.0")]));
        assert_eq!(panel.config.version.as_deref(), Some("0.10.0"));

        deliver_releases(&mut panel, Ok(vec![release("0.11.0")]));
        assert_eq!(
            panel.config.version.as_deref(),
            Some("0.11.0"),
            "the choice must be judged against the list that just arrived",
        );
        assert!(panel.picked().is_some(), "Update would stay greyed out");
    }

    #[test]
    fn the_saved_version_survives_the_first_listing_of_a_session() {
        let mut panel = panel("127.0.0.1:1");
        panel.config.version = Some("0.10.0".to_owned());
        assert!(panel.releases.is_empty());
        deliver_releases(&mut panel, Ok(vec![release("0.11.0"), release("0.10.0")]));
        assert_eq!(
            panel.config.version.as_deref(),
            Some("0.10.0"),
            "the panel must not overwrite what the user chose last session",
        );
    }

    #[test]
    fn a_github_failure_never_takes_over_the_appliance_error_line() {
        let mut panel = panel("127.0.0.1:1");
        deliver_releases(&mut panel, Err("no network".to_owned()));
        assert_eq!(panel.release_error.as_deref(), Some("no network"));
        assert!(panel.error.is_none());
        assert!(panel.picked().is_none());
    }

    #[test]
    fn a_release_query_does_not_hold_up_the_status_polling() {
        let mut panel = panel("127.0.0.1:1");
        let (_tx, rx) = channel::<Outcome>();
        panel.release_job = Some(rx);
        panel.listing = Listing::Done;
        panel.polled = Instant::now().checked_sub(POLL_PERIOD * 2);
        assert_eq!(panel.pump(true), None, "the poll starts anyway");
        assert!(panel.inflight.is_some(), "the bus state keeps being read");
    }

    #[test]
    fn the_poll_that_races_the_restart_is_not_reported_as_a_failure() {
        let mut panel = panel("127.0.0.1:1");
        panel.updating = true;
        panel.awaiting = Some("0.10.0".to_owned());
        deliver(&mut panel, Outcome::Message("installed 0.10.0".to_owned()));
        assert!(!panel.updating);
        assert!(panel.restarting());
        assert!(
            panel.polled.is_some(),
            "an immediate poll would reach the old process and end the grace window",
        );

        deliver(&mut panel, Outcome::Status(Box::new(status("0.9.0"))));
        assert!(
            panel.restarting(),
            "the old binary answers for another half second before it exits",
        );

        deliver(&mut panel, Outcome::Failed("connection refused".to_owned()));
        assert!(panel.error.is_none(), "the server is on its way back up");
        assert_eq!(
            panel.message.as_deref(),
            Some("the server is restarting...")
        );

        deliver(&mut panel, Outcome::Status(Box::new(status("0.10.0"))));
        assert!(!panel.restarting(), "the new binary answered");
        assert!(panel.awaiting.is_none());
    }

    #[test]
    fn a_worker_that_dies_does_not_leave_the_panel_thinking_it_is_updating() {
        let mut panel = panel("127.0.0.1:1");
        panel.updating = true;
        let (tx, rx) = channel::<Outcome>();
        drop(tx);
        panel.inflight = Some(rx);
        panel.pump_inflight();
        assert!(!panel.updating);
        assert!(panel.error.is_some());
    }

    #[test]
    fn an_update_that_fails_before_the_restart_window_is_reported() {
        let mut panel = panel("127.0.0.1:1");
        panel.updating = true;
        deliver(
            &mut panel,
            Outcome::Failed("the download was truncated".to_owned()),
        );
        assert!(!panel.updating);
        assert_eq!(panel.error.as_deref(), Some("the download was truncated"));
    }

    #[test]
    fn the_release_list_is_fetched_once_the_appliance_has_answered() {
        let mut panel = panel("127.0.0.1:1");
        assert_eq!(
            panel.listing,
            Listing::Idle,
            "nothing is known about the running server yet",
        );

        deliver(&mut panel, Outcome::Status(Box::new(status("0.9.0"))));
        assert_eq!(panel.listing, Listing::Due);

        panel.listing = Listing::Done;
        deliver(&mut panel, Outcome::Status(Box::new(status("0.9.0"))));
        assert_eq!(
            panel.listing,
            Listing::Done,
            "every poll would otherwise query GitHub again",
        );
    }

    #[test]
    fn the_address_field_takes_a_bare_host_or_a_url() {
        assert_eq!(base_url(" autd3.local:8081 "), "http://autd3.local:8081");
        assert_eq!(base_url("http://10.0.0.2:8081"), "http://10.0.0.2:8081");
    }
}
