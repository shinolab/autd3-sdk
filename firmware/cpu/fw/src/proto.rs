pub use autd3_cpu_wire::{
    Cmd, DEVICE_TO_HOST_BYTES, Error, HOST_TO_DEVICE_BYTES, Mode, PAYLOAD_BYTES, Telemetry,
    wire_enum,
};

pub const WIRE_RX_FRAME_BYTES: usize = HOST_TO_DEVICE_BYTES + 2;
pub const WIRE_RX_GAP_START: usize = 498;
pub const WIRE_RX_GAP_END: usize = 500;

pub const BUFFER_SIZE_MIN: u32 = autd3_cpu_wire::layout::BUFFER_SIZE_MIN as u32;
pub const MOD_BUFFER_SAMPLES: u32 = autd3_cpu_wire::layout::MOD_BUFFER_SAMPLES as u32;
pub const EMISSION_RAM_WORDS: u32 = autd3_cpu_wire::layout::EMISSION_RAM_WORDS as u32;
pub const EMISSION_SLOT_WORDS: u32 = autd3_cpu_wire::layout::EMISSION_SLOT_WORDS as u32;
pub const MAX_FOCI_TOTAL: u32 = autd3_cpu_wire::layout::MAX_FOCI_TOTAL as u32;

pub const OUTPUT_MASK_WORDS: usize = autd3_cpu_wire::layout::OUTPUT_MASK_WORDS;

pub const AL_STATUS_CODE_SYNC_ERROR: u16 = 0x001A;
pub const AL_STATUS_CODE_SM_WATCHDOG: u16 = 0x001B;
pub const FAILSAFE_TICKS: u16 = 500;

const PAYLOAD_HEAD_BYTES: usize = WIRE_RX_GAP_START - 2;

const _: () = assert!(crate::params::NUM_TRANSDUCERS <= EMISSION_SLOT_WORDS as usize);

#[derive(Clone, Copy)]
pub struct RxFrame {
    pub seq: u8,
    pub cmd: u8,
    pub payload: [u8; PAYLOAD_BYTES],
}

impl RxFrame {
    pub const ZERO: Self = Self {
        seq: 0,
        cmd: 0,
        payload: [0; PAYLOAD_BYTES],
    };

    #[must_use]
    pub fn from_wire(frame: &[u8; WIRE_RX_FRAME_BYTES]) -> Self {
        let mut rx = Self::ZERO;
        rx.seq = frame[0];
        rx.cmd = frame[1];
        rx.payload[..PAYLOAD_HEAD_BYTES].copy_from_slice(&frame[2..WIRE_RX_GAP_START]);
        rx.payload[PAYLOAD_HEAD_BYTES..].copy_from_slice(&frame[WIRE_RX_GAP_END..]);
        rx
    }
}

impl Default for RxFrame {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct TxFrame {
    pub ack: u8,
    pub data: u8,
}
