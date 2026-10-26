pub use autd3_cpu_wire::{
    Cmd, EXEC_AT_UNIT_NS, Error, FRAME_BYTES_MAX, FrameHeader, PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX,
    ReplyData, wire_enum_u8,
};

pub const MOD_BUFFER_SAMPLES: u32 = autd3_cpu_wire::layout::MOD_BUFFER_SAMPLES as u32;
pub const EMISSION_RAM_WORDS: u32 = autd3_cpu_wire::layout::EMISSION_RAM_WORDS as u32;
pub const EMISSION_SLOT_WORDS: u32 = autd3_cpu_wire::layout::EMISSION_SLOT_WORDS as u32;

pub const OUTPUT_MASK_WORDS: usize = autd3_cpu_wire::layout::OUTPUT_MASK_WORDS;

const _: () = assert!(crate::fpga_params::NUM_TRANSDUCERS <= EMISSION_SLOT_WORDS as usize);

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

#[derive(PartialEq, Eq, Debug)]
pub enum Drained {
    Empty,
    Completed { msg_id: u16 },
}
