pub use autd3_cpu_wire::{
    Cmd, EXEC_AT_UNIT_NS, Error, FRAME_BYTES_MAX, FrameHeader, PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX,
    wire_enum_u8,
};
use zerocopy::little_endian::U32;

pub const MOD_BUFFER_SAMPLES: u32 = autd3_cpu_wire::layout::MOD_BUFFER_SAMPLES as u32;
pub const EMISSION_RAM_WORDS: u32 = autd3_cpu_wire::layout::EMISSION_RAM_WORDS as u32;
pub const EMISSION_SLOT_WORDS: u32 = autd3_cpu_wire::layout::EMISSION_SLOT_WORDS as u32;

pub const OUTPUT_MASK_WORDS: usize = autd3_cpu_wire::layout::OUTPUT_MASK_WORDS;

const _: () = assert!(crate::fpga_params::NUM_TRANSDUCERS <= EMISSION_SLOT_WORDS as usize);

#[derive(Clone, Copy)]
#[repr(C, align(4))]
pub struct RxFrame {
    pub msg_id: u16,
    pub len: u16,
    pub payload: [u8; PAYLOAD_BYTES],
    pub header: FrameHeader,
}

const _: () = assert!(core::mem::offset_of!(RxFrame, payload).is_multiple_of(4));

impl RxFrame {
    pub const ZERO: Self = Self {
        msg_id: 0,
        len: 0,
        payload: [0; PAYLOAD_BYTES],
        header: FrameHeader {
            reserved: 0,
            cmd: 0,
            exec_at: U32::ZERO,
        },
    };

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReplyData {
    len: u8,
    bytes: [u8; REPLY_DATA_BYTES_MAX],
}

impl ReplyData {
    pub const EMPTY: Self = Self {
        len: 0,
        bytes: [0; REPLY_DATA_BYTES_MAX],
    };

    #[must_use]
    pub fn from_slice(data: &[u8]) -> Self {
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let mut bytes = [0; REPLY_DATA_BYTES_MAX];
        bytes[..len].copy_from_slice(&data[..len]);
        Self {
            len: len as u8,
            bytes,
        }
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Reply {
    pub status: Error,
    data: ReplyData,
}

impl Reply {
    pub const EMPTY: Self = Self::new(Error::None, ReplyData::EMPTY);

    #[must_use]
    pub const fn new(status: Error, data: ReplyData) -> Self {
        Self { status, data }
    }

    #[must_use]
    pub fn data(&self) -> &[u8] {
        self.data.as_slice()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Disposition {
    Deferred,
    Dropped,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drained {
    Empty,
    Completed { msg_id: u16 },
}
