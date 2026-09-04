use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct TransducerInfo {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct TransState {
    pub amp: f32,
    pub phase: f32,
    pub enable: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DeviceState {
    pub num_transducers: u16,
    pub silencer_fixed_update_rate: bool,
    pub silencer_intensity: u16,
    pub silencer_phase: u16,
    pub mod_freq_div: u16,
    pub mod_cycle: u32,
    pub mod_idx: u32,
    pub mod_buffer: Vec<u8>,
    pub stm_freq_div: u16,
    pub stm_cycle: u32,
    pub stm_idx: u32,
    pub gpio_types: [u8; 4],
    pub gpio_out: [Vec<u8>; 4],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Geometry { transducers: Vec<TransducerInfo> },
    State { states: Vec<TransState> },
    DeviceStates { devices: Vec<DeviceState> },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    SetModulationEnabled { enabled: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip_server(msg: &ServerMsg) -> ServerMsg {
        let json = serde_json::to_string(msg).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    fn sample_device_state() -> DeviceState {
        DeviceState {
            num_transducers: 249,
            silencer_fixed_update_rate: true,
            silencer_intensity: 256,
            silencer_phase: 128,
            mod_freq_div: 10,
            mod_cycle: 80,
            mod_idx: 3,
            mod_buffer: vec![0, 64, 128, 255],
            stm_freq_div: 20,
            stm_cycle: 5,
            stm_idx: 4,
            gpio_types: [0x01, 0x20, 0xE0, 0xF0],
            gpio_out: [vec![0, 1], vec![1, 0], vec![1, 1], vec![0, 0]],
        }
    }

    #[test]
    fn geometry_round_trips() {
        let msg = ServerMsg::Geometry {
            transducers: vec![
                TransducerInfo {
                    pos: [0.0, 10.16, -20.5],
                    dir: [0.0, 0.0, 1.0],
                },
                TransducerInfo {
                    pos: [192.0, 151.4, 0.25],
                    dir: [-1.0, 0.0, 0.0],
                },
            ],
        };
        assert_eq!(msg, round_trip_server(&msg));
    }

    #[test]
    fn state_round_trips() {
        let msg = ServerMsg::State {
            states: vec![
                TransState {
                    amp: 0.5,
                    phase: 1.25,
                    enable: true,
                },
                TransState {
                    amp: 0.0,
                    phase: -2.75,
                    enable: false,
                },
            ],
        };
        assert_eq!(msg, round_trip_server(&msg));
    }

    #[test]
    fn device_states_round_trip() {
        let msg = ServerMsg::DeviceStates {
            devices: vec![sample_device_state()],
        };
        assert_eq!(msg, round_trip_server(&msg));
    }

    #[test]
    fn empty_collections_round_trip() {
        for msg in [
            ServerMsg::Geometry {
                transducers: Vec::new(),
            },
            ServerMsg::State { states: Vec::new() },
            ServerMsg::DeviceStates {
                devices: Vec::new(),
            },
        ] {
            assert_eq!(msg, round_trip_server(&msg));
        }
    }

    #[test]
    fn client_msg_round_trips() {
        for enabled in [true, false] {
            let msg = ClientMsg::SetModulationEnabled { enabled };
            let json = serde_json::to_string(&msg).unwrap();
            assert_eq!(msg, serde_json::from_str(&json).unwrap());
        }
    }

    #[test]
    fn server_msg_variants_are_tagged_by_type() {
        let json = serde_json::to_string(&ServerMsg::DeviceStates {
            devices: Vec::new(),
        })
        .unwrap();
        assert!(json.contains(r#""type":"device_states""#));

        let json =
            serde_json::to_string(&ClientMsg::SetModulationEnabled { enabled: true }).unwrap();
        assert!(json.contains(r#""type":"set_modulation_enabled""#));
    }

    #[test]
    fn unknown_client_msg_type_is_rejected() {
        assert!(serde_json::from_str::<ClientMsg>(r#"{"type":"unknown"}"#).is_err());
    }
}
