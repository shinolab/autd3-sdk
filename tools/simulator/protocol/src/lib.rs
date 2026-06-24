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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Geometry { transducers: Vec<TransducerInfo> },
    State { states: Vec<TransState> },
}
