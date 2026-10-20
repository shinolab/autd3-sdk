use std::net::SocketAddr;

use autd3_cpu_wire::udp::{Flags, UnitInfo};
use autd3_rs::{BusStats, DeviceState};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Opening,
    Serving,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counters {
    pub frames: u64,
    pub replies: u64,
    pub dropped: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BusCounters {
    pub heartbeats: u64,
    pub missed_heartbeat_replies: u64,
    pub recoveries: u64,
}

impl BusCounters {
    #[must_use]
    pub fn new(stats: &BusStats) -> Self {
        Self {
            heartbeats: stats.heartbeats(),
            missed_heartbeat_replies: stats.missed_heartbeat_replies(),
            recoveries: stats.recoveries(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitState {
    Ready,
    Syncing,
    Lost,
}

impl UnitState {
    #[must_use]
    pub const fn new(state: DeviceState) -> Self {
        match state {
            DeviceState::Ready => UnitState::Ready,
            DeviceState::Syncing => UnitState::Syncing,
            DeviceState::Lost => UnitState::Lost,
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UnitStatus {
    pub id: usize,
    pub state: Option<UnitState>,
    pub responding: bool,
    pub ptp_locked: bool,
    pub sync_ready: bool,
    pub firmware: Option<String>,
    pub ptp_offset_ns: Option<i32>,
}

impl UnitStatus {
    #[must_use]
    pub fn new(id: usize, state: Option<DeviceState>, device: Option<&UnitInfo>) -> Self {
        Self {
            id,
            state: state.map(UnitState::new),
            responding: device.is_some(),
            ptp_locked: device.is_some_and(|info| info.flags.contains(Flags::PTP_LOCKED)),
            sync_ready: device.is_some_and(|info| info.flags.contains(Flags::SYNC_READY)),
            firmware: device.map(|info| {
                let [major, minor, patch] = info.fw_version;
                format!("{major}.{minor}.{patch}")
            }),
            ptp_offset_ns: device.map(|info| info.ptp_offset_ns.get()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Status {
    pub phase: Phase,
    pub error: Option<String>,
    pub client: Option<SocketAddr>,
    pub relayed: Counters,
    pub bus: BusCounters,
    pub units: Vec<UnitStatus>,
}

impl Status {
    #[must_use]
    pub fn opening(error: Option<String>) -> Self {
        Self {
            phase: Phase::Opening,
            error,
            client: None,
            relayed: Counters::default(),
            bus: BusCounters::default(),
            units: Vec::new(),
        }
    }

    #[must_use]
    pub fn serving(
        client: Option<SocketAddr>,
        relayed: Counters,
        bus: BusCounters,
        states: Option<&[DeviceState]>,
        devices: &[Option<UnitInfo>],
    ) -> Self {
        Self {
            phase: Phase::Serving,
            error: None,
            client,
            relayed,
            bus,
            units: devices
                .iter()
                .enumerate()
                .map(|(id, device)| {
                    UnitStatus::new(
                        id,
                        states.and_then(|states| states.get(id).copied()),
                        device.as_ref(),
                    )
                })
                .collect(),
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use zerocopy::little_endian::{I32, U64};

    use super::*;

    #[test]
    fn an_opening_status_is_one_json_line_naming_the_failure() {
        assert_eq!(
            Status::opening(Some("no devices".into()))
                .to_json()
                .unwrap(),
            concat!(
                r#"{"phase":"opening","error":"no devices","client":null,"#,
                r#""relayed":{"frames":0,"replies":0,"dropped":0},"#,
                r#""bus":{"heartbeats":0,"missed_heartbeat_replies":0,"recoveries":0},"units":[]}"#
            )
        );
    }

    #[test]
    fn a_serving_status_lists_every_unit_in_chain_order() {
        let device = UnitInfo {
            unit_id: 0,
            flags: Flags::ASSIGNED | Flags::PTP_LOCKED,
            upstream_port: 0,
            reserved: 0,
            fw_version: [0, 10, 1],
            reserved2: 0,
            sys_time: U64::new(0),
            ptp_offset_ns: I32::new(-3),
        };
        let client = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 5000));
        let relayed = Counters {
            frames: 4,
            replies: 2,
            dropped: 1,
        };
        let bus = BusCounters {
            heartbeats: 9,
            missed_heartbeat_replies: 1,
            recoveries: 2,
        };
        assert_eq!(
            Status::serving(
                Some(client),
                relayed,
                bus,
                Some(&[DeviceState::Syncing, DeviceState::Lost]),
                &[Some(device), None]
            )
            .to_json()
            .unwrap(),
            concat!(
                r#"{"phase":"serving","error":null,"client":"[::1]:5000","#,
                r#""relayed":{"frames":4,"replies":2,"dropped":1},"#,
                r#""bus":{"heartbeats":9,"missed_heartbeat_replies":1,"recoveries":2},"#,
                r#""units":[{"id":0,"state":"syncing","responding":true,"ptp_locked":true,"sync_ready":false,"firmware":"0.10.1","ptp_offset_ns":-3},"#,
                r#"{"id":1,"state":"lost","responding":false,"ptp_locked":false,"sync_ready":false,"firmware":null,"ptp_offset_ns":null}]}"#
            )
        );
    }

    #[test]
    fn the_state_is_null_while_no_client_drives_the_bus() {
        let status = Status::serving(
            None,
            Counters::default(),
            BusCounters::default(),
            None,
            &[None],
        );
        assert_eq!(status.units[0].state, None);
    }
}
