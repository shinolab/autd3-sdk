#[cfg(feature = "client")]
mod client;

#[cfg(feature = "discovery")]
pub use autd3_rs_link_remote::{
    Appliance, DiscoveryError, DiscoveryOption, SERVICE_TYPE, discover, discover_all,
};
#[cfg(feature = "client")]
pub use client::{ApplianceClient, ClientError, host_of};

use serde::{Deserialize, Serialize};

pub const DEFAULT_CONTROL_PORT: u16 = 8081;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusDesired {
    #[default]
    Closed,
    Open,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusActual {
    #[default]
    Closed,
    Opening,
    Open,
    Recovering,
    Failed,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BusStatus {
    pub desired: BusDesired,
    pub actual: BusActual,
    pub failure: Option<String>,
    pub num_devices: usize,
    pub devices: Vec<String>,
    pub recoveries: u64,
    pub stale_cycles: u64,
    pub lost_cycles: u64,
    pub phase_excursions: u64,
    pub worst_phase_deviation_ns: u64,
    pub exchanges: u64,
    pub exchange_mean_ns: u64,
    pub exchange_worst_ns: u64,
}

impl BusStatus {
    #[must_use]
    pub fn has_unknown_state(&self) -> bool {
        self.actual == BusActual::Unknown || self.desired == BusDesired::Unknown
    }
}

pub const UNKNOWN_STATE_HINT: &str = "unknown to this client; update it to match the appliance";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InterfaceStatus {
    pub name: String,
    pub operstate: String,
    pub carrier: bool,
    pub speed_mbps: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UplinkKind {
    #[default]
    Ethernet,
    Wifi,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WifiStatus {
    pub blocked: bool,
    pub ssid: Option<String>,
    pub signal_dbm: Option<i32>,
    pub regdomain: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UplinkStatus {
    pub name: String,
    pub kind: UplinkKind,
    pub operstate: String,
    pub carrier: bool,
    pub addresses: Vec<String>,
    pub wifi: Option<WifiStatus>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageStatus {
    pub path: String,
    pub total_mb: u64,
    pub free_mb: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientStatus {
    pub peer: String,
    pub devices: usize,
    pub connected_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRelease {
    pub version: String,
    pub sdk_version: String,
    pub built: String,
    pub board: String,
    pub commit: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplianceStatus {
    pub instance: String,
    pub sdk_version: String,
    pub wire_version: u8,
    pub uptime_secs: u64,
    pub allow_admin: bool,
    pub bus: BusStatus,
    #[serde(default)]
    pub binary: Option<String>,
    pub interface: InterfaceStatus,
    #[serde(default)]
    pub uplinks: Vec<UplinkStatus>,
    #[serde(default)]
    pub storage: Option<StorageStatus>,
    pub client: Option<ClientStatus>,
    #[serde(default)]
    pub image: Option<ImageRelease>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigDocument {
    pub toml: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub num_devices: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLines {
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiCredentials {
    pub ssid: String,
    pub psk: Option<String>,
    pub country: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiForget {
    pub radio_off: bool,
    pub force: bool,
}

pub const DEFAULT_CAPTURE_MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_CAPTURE_MAX_BYTES: u64 = 512 * 1024 * 1024;
pub const DEFAULT_CAPTURE_MAX_SECONDS: u64 = 60;
pub const MAX_CAPTURE_MAX_SECONDS: u64 = 3600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureRequest {
    pub max_bytes: u64,
    pub max_seconds: u64,
}

impl Default for CaptureRequest {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_CAPTURE_MAX_BYTES,
            max_seconds: DEFAULT_CAPTURE_MAX_SECONDS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureState {
    #[default]
    Idle,
    Running,
    Finished,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureStatus {
    pub state: CaptureState,
    pub interface: String,
    pub frames: u64,
    pub bytes: u64,
    pub elapsed_seconds: u64,
    pub stopped_because: Option<String>,
    pub error: Option<String>,
}

pub const FRAME_PHASE_AUTO: u8 = 0;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TuneRequest {
    pub periods_ns: Vec<u64>,
    pub frame_phase_percents: Vec<u8>,
    pub warmup_ns: u64,
    pub dwell_ns: u64,
    pub settle_ns: u64,
    pub poll_ns: u64,
}

impl Default for TuneRequest {
    fn default() -> Self {
        Self {
            periods_ns: vec![1_000_000, 2_000_000],
            frame_phase_percents: vec![FRAME_PHASE_AUTO],
            warmup_ns: 5_000_000_000,
            dwell_ns: 30_000_000_000,
            settle_ns: 20_000_000_000,
            poll_ns: 100_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TuneStatus {
    #[default]
    Ok,
    FailedOpen,
    Aborted,
    Infeasible,
    #[serde(other)]
    Unknown,
}

impl TuneStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::FailedOpen => "failed-open",
            Self::Aborted => "aborted",
            Self::Infeasible => "infeasible",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TuneTarget {
    pub period_ns: u64,
    pub frame_phase_percent: u8,
    pub frame_phase_ns: u64,
}

impl TuneTarget {
    #[must_use]
    pub fn is_auto(&self) -> bool {
        self.frame_phase_percent == FRAME_PHASE_AUTO
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TuneCandidate {
    pub target: TuneTarget,
    pub status: TuneStatus,
    pub note: Option<String>,
    pub num_devices: usize,
    pub samples: u64,
    pub op_samples: u64,
    pub drop_events: u64,
    pub recoveries: u64,
    pub stale_cycles: u64,
    pub lost_cycles: u64,
    pub phase_excursions: u64,
    pub exchanges: u64,
    pub exchange_mean_ns: u64,
    pub exchange_worst_ns: u64,
    pub frames_delivered: u64,
    pub frames_skipped: u64,
    pub telemetry_read: bool,
}

impl TuneCandidate {
    #[must_use]
    pub fn op_ratio(&self) -> f64 {
        if self.samples == 0 {
            0.0
        } else {
            self.op_samples as f64 / self.samples as f64
        }
    }

    #[must_use]
    pub fn per_exchange(&self, count: u64) -> f64 {
        if self.exchanges == 0 {
            0.0
        } else {
            count as f64 / self.exchanges as f64
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TuneCalibration {
    pub num_devices: usize,
    pub period_ns: u64,
    pub exchanges: u64,
    pub exchange_mean_ns: u64,
    pub exchange_worst_ns: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TuneReport {
    pub running: bool,
    pub cancelled: bool,
    pub total: usize,
    pub current: Option<TuneTarget>,
    pub calibration: Option<TuneCalibration>,
    pub candidates: Vec<TuneCandidate>,
    pub best: Option<usize>,
    pub error: Option<String>,
}

impl TuneReport {
    #[must_use]
    pub fn best_candidate(&self) -> Option<&TuneCandidate> {
        self.best.and_then(|index| self.candidates.get(index))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TuneScore {
    pub op_ratio: f64,
    pub drop_events: u64,
    pub recoveries: u64,
    pub lost_per_exchange: f64,
    pub stale_per_exchange: f64,
    pub excursions_per_exchange: f64,
    pub tie_break_ns: u64,
    pub period_ns: u64,
    pub is_auto: bool,
}

pub fn best_tune_score<T>(
    candidates: &[T],
    score: impl Fn(&T) -> Option<TuneScore>,
) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| score(candidate).map(|score| (index, score)))
        .max_by(|(_, a), (_, b)| {
            a.op_ratio
                .total_cmp(&b.op_ratio)
                .then(b.drop_events.cmp(&a.drop_events))
                .then(b.recoveries.cmp(&a.recoveries))
                .then(b.lost_per_exchange.total_cmp(&a.lost_per_exchange))
                .then(b.stale_per_exchange.total_cmp(&a.stale_per_exchange))
                .then(
                    b.excursions_per_exchange
                        .total_cmp(&a.excursions_per_exchange),
                )
                .then(b.tie_break_ns.cmp(&a.tie_break_ns))
                .then(b.period_ns.cmp(&a.period_ns))
                .then(a.is_auto.cmp(&b.is_auto))
        })
        .map(|(index, _)| index)
}

#[must_use]
pub fn best_tune_candidate(candidates: &[TuneCandidate]) -> Option<usize> {
    best_tune_score(candidates, |c| {
        (c.status == TuneStatus::Ok && c.samples > 0).then(|| TuneScore {
            op_ratio: c.op_ratio(),
            drop_events: c.drop_events,
            recoveries: c.recoveries,
            lost_per_exchange: c.per_exchange(c.lost_cycles),
            stale_per_exchange: c.per_exchange(c.stale_cycles),
            excursions_per_exchange: c.per_exchange(c.phase_excursions),
            tie_break_ns: 0,
            period_ns: c.target.period_ns,
            is_auto: c.target.is_auto(),
        })
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accepted {
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bus_state_names_stay_lowercase_on_the_wire() {
        let status = BusStatus {
            desired: BusDesired::Open,
            actual: BusActual::Recovering,
            ..BusStatus::default()
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains(r#""desired":"open""#), "{json}");
        assert!(json.contains(r#""actual":"recovering""#), "{json}");
        assert_eq!(serde_json::from_str::<BusStatus>(&json).unwrap(), status);
    }

    #[test]
    fn a_status_from_a_server_without_an_image_field_still_parses() {
        let json = format!(
            r#"{{
                "instance": "autd3-0a1b2c3d", "sdk_version": "0.4.0", "wire_version": 6,
                "uptime_secs": 12, "allow_admin": true, "client": null,
                "bus": {},
                "interface": {{"name": "ecat0", "operstate": "up", "carrier": true, "speed_mbps": 100}}
            }}"#,
            serde_json::to_string(&BusStatus::default()).unwrap(),
        );
        let status: ApplianceStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status.image, None);
        assert_eq!(status.uplinks, vec![]);
        assert_eq!(status.storage, None);
    }

    #[test]
    fn a_status_struct_an_older_server_sent_short_still_parses() {
        let bus: BusStatus = serde_json::from_str(r#"{"actual":"open"}"#).unwrap();
        assert_eq!(bus.actual, BusActual::Open);
        assert_eq!(bus.exchange_worst_ns, 0);

        let nic: InterfaceStatus = serde_json::from_str(r#"{"name":"eth0"}"#).unwrap();
        assert_eq!(nic.name, "eth0");
        assert!(!nic.carrier);

        let uplink: UplinkStatus = serde_json::from_str(r#"{"name":"wlan0"}"#).unwrap();
        assert_eq!(uplink.kind, UplinkKind::Ethernet);
        assert_eq!(uplink.wifi, None);

        let wifi: WifiStatus = serde_json::from_str(r#"{"ssid":"lab"}"#).unwrap();
        assert_eq!(wifi.ssid.as_deref(), Some("lab"));
        assert_eq!(wifi.signal_dbm, None);

        let storage: StorageStatus = serde_json::from_str(r#"{"path":"/data"}"#).unwrap();
        assert_eq!(storage.total_mb, 0);

        let target: TuneTarget = serde_json::from_str(r#"{"period_ns":2000000}"#).unwrap();
        assert_eq!(target.period_ns, 2_000_000);
        assert!(target.is_auto());

        let candidate: TuneCandidate = serde_json::from_str(r#"{"status":"ok"}"#).unwrap();
        assert_eq!(candidate.status, TuneStatus::Ok);
        assert_eq!(candidate.samples, 0);

        let report: TuneReport = serde_json::from_str(r#"{"running":true}"#).unwrap();
        assert!(report.running);
        assert_eq!(report.candidates, vec![]);
        assert_eq!(report.best, None);
    }

    #[test]
    fn an_uplink_names_its_kind_in_lowercase_on_the_wire() {
        let uplink = UplinkStatus {
            name: "wlan0".to_owned(),
            kind: UplinkKind::Wifi,
            operstate: "up".to_owned(),
            carrier: true,
            addresses: vec!["172.16.99.64".to_owned()],
            wifi: Some(WifiStatus {
                blocked: false,
                ssid: Some("TP-Link_0B1C".to_owned()),
                signal_dbm: Some(-47),
                regdomain: Some("JP".to_owned()),
            }),
        };
        let json = serde_json::to_string(&uplink).unwrap();
        assert!(json.contains(r#""kind":"wifi""#), "{json}");
        assert_eq!(serde_json::from_str::<UplinkStatus>(&json).unwrap(), uplink);
    }

    #[test]
    fn a_bus_state_this_client_does_not_know_does_not_fail_the_whole_status() {
        let json = r#"{
            "desired": "paused", "actual": "quiescing", "failure": null,
            "num_devices": 0, "devices": [], "recoveries": 0, "stale_cycles": 0,
            "lost_cycles": 0, "phase_excursions": 0, "worst_phase_deviation_ns": 0,
            "exchanges": 0, "exchange_mean_ns": 0, "exchange_worst_ns": 0
        }"#;
        let bus: BusStatus = serde_json::from_str(json).unwrap();
        assert_eq!(bus.desired, BusDesired::Unknown);
        assert_eq!(bus.actual, BusActual::Unknown);
        assert!(bus.has_unknown_state());
    }

    #[test]
    fn an_uplink_kind_this_client_does_not_know_does_not_fail_the_whole_status() {
        let json = r#"{
            "name": "usb0", "kind": "cellular", "operstate": "up",
            "carrier": true, "addresses": [], "wifi": null
        }"#;
        let uplink: UplinkStatus = serde_json::from_str(json).unwrap();
        assert_eq!(uplink.kind, UplinkKind::Unknown);
    }

    #[test]
    fn the_best_candidate_is_the_one_that_held_op_longest() {
        let candidate = |period_ns: u64, samples: u64, op: u64| TuneCandidate {
            target: TuneTarget {
                period_ns,
                ..TuneTarget::default()
            },
            samples,
            op_samples: op,
            ..TuneCandidate::default()
        };
        let candidates = vec![
            candidate(1_000_000, 100, 54),
            candidate(2_000_000, 100, 100),
            candidate(3_000_000, 100, 100),
        ];
        assert_eq!(
            best_tune_candidate(&candidates),
            Some(1),
            "a tie on OP retention goes to the shorter period, which costs less latency",
        );
    }

    #[test]
    fn a_tie_on_op_is_broken_by_the_phase_excursions_not_by_exchange_noise() {
        let candidate = |period_ns: u64, excursions: u64, worst_ns: u64| TuneCandidate {
            target: TuneTarget {
                period_ns,
                ..TuneTarget::default()
            },
            samples: 300,
            op_samples: 300,
            exchanges: 27_000,
            phase_excursions: excursions,
            exchange_worst_ns: worst_ns,
            ..TuneCandidate::default()
        };
        let candidates = vec![
            candidate(1_000_000, 25_069, 1_133_484),
            candidate(2_000_000, 12, 1_146_000),
            candidate(2_000_000, 8, 1_132_000),
        ];
        assert_eq!(
            best_tune_candidate(&candidates),
            Some(2),
            "the candidate that stayed on its landing phase wins",
        );

        let noisy = vec![
            candidate(2_000_000, 8, 1_190_000),
            candidate(1_000_000, 25_069, 1_100_000),
        ];
        assert_eq!(
            best_tune_candidate(&noisy),
            Some(0),
            "a smaller worst exchange no longer outranks a candidate that held its phase",
        );
    }

    #[test]
    fn counters_are_compared_per_exchange_so_a_long_period_gets_no_free_pass() {
        let candidate = |period_ns: u64, exchanges: u64, stale: u64| TuneCandidate {
            target: TuneTarget {
                period_ns,
                ..TuneTarget::default()
            },
            samples: 300,
            op_samples: 300,
            exchanges,
            stale_cycles: stale,
            ..TuneCandidate::default()
        };
        let candidates = vec![
            candidate(1_000_000, 30_000, 30),
            candidate(2_000_000, 15_000, 20),
        ];
        assert_eq!(
            best_tune_candidate(&candidates),
            Some(0),
            "0.10% stale beats 0.13% stale even though the raw count is higher",
        );
    }

    #[test]
    fn an_automatic_landing_phase_wins_a_dead_heat() {
        let candidate = |percent: u8| TuneCandidate {
            target: TuneTarget {
                period_ns: 2_000_000,
                frame_phase_percent: percent,
                frame_phase_ns: 2_000_000 * u64::from(percent) / 100,
            },
            samples: 300,
            op_samples: 300,
            exchanges: 15_000,
            ..TuneCandidate::default()
        };
        assert_eq!(
            best_tune_candidate(&[candidate(FRAME_PHASE_AUTO), candidate(25)]),
            Some(0),
        );
        assert_eq!(
            best_tune_candidate(&[candidate(25), candidate(FRAME_PHASE_AUTO)]),
            Some(1),
            "the order the candidates were swept in must not decide it",
        );
    }

    #[test]
    fn a_fixed_landing_phase_still_wins_when_it_actually_measured_better() {
        let auto = TuneCandidate {
            target: TuneTarget {
                period_ns: 2_000_000,
                ..TuneTarget::default()
            },
            samples: 300,
            op_samples: 300,
            exchanges: 15_000,
            phase_excursions: 40,
            ..TuneCandidate::default()
        };
        let fixed = TuneCandidate {
            target: TuneTarget {
                period_ns: 2_000_000,
                frame_phase_percent: 25,
                frame_phase_ns: 500_000,
            },
            phase_excursions: 0,
            ..auto.clone()
        };
        assert_eq!(best_tune_candidate(&[auto, fixed]), Some(1));
    }

    #[test]
    fn a_period_that_cannot_carry_one_exchange_is_never_the_best() {
        let candidates = vec![
            TuneCandidate {
                status: TuneStatus::Infeasible,
                note: Some("20 devices need about 1059 us on the wire".to_owned()),
                samples: 0,
                ..TuneCandidate::default()
            },
            TuneCandidate {
                target: TuneTarget {
                    period_ns: 2_000_000,
                    ..TuneTarget::default()
                },
                samples: 300,
                op_samples: 300,
                exchanges: 15_000,
                ..TuneCandidate::default()
            },
        ];
        assert_eq!(best_tune_candidate(&candidates), Some(1));
    }

    #[test]
    fn an_infeasible_candidate_survives_a_round_trip_through_the_wire() {
        let candidate = TuneCandidate {
            status: TuneStatus::Infeasible,
            ..TuneCandidate::default()
        };
        let json = serde_json::to_string(&candidate).unwrap();
        assert!(json.contains("\"infeasible\""), "{json}");
        let back: TuneCandidate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, TuneStatus::Infeasible);
        assert_eq!(TuneStatus::Infeasible.label(), "infeasible");
    }

    #[test]
    fn a_report_without_a_calibration_still_parses_and_carries_one_when_it_has_it() {
        let report: TuneReport = serde_json::from_str(r#"{"running":false}"#).unwrap();
        assert_eq!(report.calibration, None);

        let measured = TuneReport {
            calibration: Some(TuneCalibration {
                num_devices: 20,
                period_ns: 2_000_000,
                exchanges: 512,
                exchange_mean_ns: 1_103_700,
                exchange_worst_ns: 1_146_119,
            }),
            ..TuneReport::default()
        };
        let json = serde_json::to_string(&measured).unwrap();
        let back: TuneReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.calibration, measured.calibration);
    }

    #[test]
    fn a_candidate_that_never_opened_is_never_the_best() {
        let candidates = vec![TuneCandidate {
            status: TuneStatus::FailedOpen,
            samples: 0,
            ..TuneCandidate::default()
        }];
        assert_eq!(best_tune_candidate(&candidates), None);
    }

    #[test]
    fn a_tune_status_this_client_does_not_know_does_not_fail_the_whole_report() {
        let json = r#"{"target":{"period_ns":1,"frame_phase_percent":0,"frame_phase_ns":0},
            "status":"melted","note":null,"num_devices":0,"samples":0,"op_samples":0,
            "drop_events":0,"recoveries":0,"stale_cycles":0,"lost_cycles":0,
            "phase_excursions":0,"exchanges":0,"exchange_mean_ns":0,"exchange_worst_ns":0}"#;
        let candidate: TuneCandidate = serde_json::from_str(json).unwrap();
        assert_eq!(candidate.status, TuneStatus::Unknown);
    }

    #[test]
    fn a_wifi_request_may_omit_the_passphrase_for_an_open_network() {
        let open: WifiCredentials = serde_json::from_str(r#"{"ssid":"lab"}"#).unwrap();
        assert_eq!(open.ssid, "lab");
        assert_eq!(open.psk, None);
        assert_eq!(open.country, None);
    }
}
