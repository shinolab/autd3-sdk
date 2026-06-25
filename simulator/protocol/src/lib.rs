use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TransducerInfo {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
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
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Geometry { transducers: Vec<TransducerInfo> },
    State { states: Vec<TransState> },
    DeviceStates { devices: Vec<DeviceState> },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    SetModulationEnabled { enabled: bool },
}
