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
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiStatus {
    pub blocked: bool,
    pub ssid: Option<String>,
    pub signal_dbm: Option<i32>,
    pub regdomain: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UplinkStatus {
    pub name: String,
    pub kind: UplinkKind,
    pub operstate: String,
    pub carrier: bool,
    pub addresses: Vec<String>,
    pub wifi: Option<WifiStatus>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    fn a_wifi_request_may_omit_the_passphrase_for_an_open_network() {
        let open: WifiCredentials = serde_json::from_str(r#"{"ssid":"lab"}"#).unwrap();
        assert_eq!(open.ssid, "lab");
        assert_eq!(open.psk, None);
        assert_eq!(open.country, None);
    }
}
