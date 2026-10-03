pub use autd3_cpu_wire::udp::FAILSAFE_TIMEOUT_MS;
pub use autd3_cpu_wire::{
    Cmd, Error, FRAME_BYTES_MAX, FRAME_HEADER_BYTES, Mode, PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX,
    Telemetry, wire_enum,
};

pub const BUFFER_SIZE_MIN: u32 = autd3_cpu_wire::layout::BUFFER_SIZE_MIN as u32;
pub const MOD_BUFFER_SAMPLES: u32 = autd3_cpu_wire::layout::MOD_BUFFER_SAMPLES as u32;
pub const EMISSION_RAM_WORDS: u32 = autd3_cpu_wire::layout::EMISSION_RAM_WORDS as u32;
pub const EMISSION_SLOT_WORDS: u32 = autd3_cpu_wire::layout::EMISSION_SLOT_WORDS as u32;
pub const MAX_FOCI_TOTAL: u32 = autd3_cpu_wire::layout::MAX_FOCI_TOTAL as u32;

pub const OUTPUT_MASK_WORDS: usize = autd3_cpu_wire::layout::OUTPUT_MASK_WORDS;

const _: () = assert!(crate::params::NUM_TRANSDUCERS <= EMISSION_SLOT_WORDS as usize);

#[derive(Clone, Copy)]
pub struct RxFrame {
    pub seq: u8,
    pub cmd: u8,
    pub msg_id: u16,
    pub len: u16,
    pub payload: [u8; PAYLOAD_BYTES],
}

impl RxFrame {
    pub const ZERO: Self = Self {
        seq: 0,
        cmd: 0,
        msg_id: 0,
        len: 0,
        payload: [0; PAYLOAD_BYTES],
    };

    #[must_use]
    pub fn from_frame(frame: &[u8], msg_id: u16) -> Self {
        let mut rx = Self::ZERO;
        rx.seq = frame.first().copied().unwrap_or(0);
        rx.cmd = frame.get(1).copied().unwrap_or(0);
        rx.msg_id = msg_id;
        let body = frame.get(FRAME_HEADER_BYTES..).unwrap_or(&[]);
        let len = body.len().min(PAYLOAD_BYTES);
        rx.payload[..len].copy_from_slice(&body[..len]);
        rx.len = len as u16;
        rx
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload[..usize::from(self.len)]
    }
}

impl Default for RxFrame {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Reply {
    pub ack: u8,
    pub status: u8,
    len: u8,
    data: [u8; REPLY_DATA_BYTES_MAX],
}

impl Reply {
    #[must_use]
    pub fn new(ack: u8, status: u8, data: &[u8]) -> Self {
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let mut buf = [0; REPLY_DATA_BYTES_MAX];
        buf[..len].copy_from_slice(&data[..len]);
        Self {
            ack,
            status,
            len: len as u8,
            data: buf,
        }
    }

    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data[..usize::from(self.len)]
    }
}

impl Default for Reply {
    fn default() -> Self {
        Self::new(0, 0, &[])
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Disposition {
    Reply,
    Deferred,
    Dropped,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drained {
    Empty,
    Completed { msg_id: u16 },
    Flushed,
}
