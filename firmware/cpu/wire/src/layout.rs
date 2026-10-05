use crate::fpga_params::{EMISSION_MAX_INDICES, NUM_TRANSDUCERS};
use crate::frame::PAYLOAD_BYTES;
use crate::payload::{UpdateChunkPayload, WritePatternRawPayload};

pub use crate::fpga_params::{
    EMISSION_SLOT_WORDS, FOCUS_WORDS, MOD_BUFFER_SAMPLES, PWE_TABLE_SIZE,
};

pub const BUFFER_SIZE_MIN: usize = 2;

pub const EMISSION_RAM_WORDS: usize = EMISSION_SLOT_WORDS * EMISSION_MAX_INDICES as usize;
pub const MAX_FOCI_TOTAL: usize = EMISSION_RAM_WORDS / FOCUS_WORDS;

pub const OUTPUT_MASK_WORDS: usize = NUM_TRANSDUCERS.div_ceil(16);
pub const GPIO_OUT_NUM: usize = 4;

pub const PATTERN_RAW_DATA_LEN: usize = 2 * NUM_TRANSDUCERS;
pub const PATTERN_RAW_MAX_COUNT: usize =
    (PAYLOAD_BYTES - core::mem::size_of::<WritePatternRawPayload>()) / PATTERN_RAW_DATA_LEN;
const UPDATE_CHUNK_ALIGN: usize = 32;
pub const UPDATE_CHUNK_MAX_DATA_LEN: usize =
    (PAYLOAD_BYTES - core::mem::size_of::<UpdateChunkPayload>()) / UPDATE_CHUNK_ALIGN
        * UPDATE_CHUNK_ALIGN;
