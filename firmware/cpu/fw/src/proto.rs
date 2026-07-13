use core::cell::Cell;
use core::mem::{offset_of, size_of};
use core::sync::atomic::AtomicU8;

use zerocopy::little_endian::{U16, U32, U64};
use zerocopy::{FromBytes, Immutable, KnownLayout, Unaligned};

pub const RX_FRAME_BYTES: usize = 626;
pub const PAYLOAD_BYTES: usize = 624;
pub const TX_FRAME_BYTES: usize = 2;

pub const WIRE_RX_FRAME_BYTES: usize = RX_FRAME_BYTES + 2;
pub const WIRE_RX_GAP_START: usize = 498;
pub const WIRE_RX_GAP_END: usize = 500;

pub const CMD_RESET: u8 = 0x00;
pub const CMD_SYNCHRONIZE: u8 = 0x01;
pub const CMD_SET_MODE: u8 = 0x02;
pub const CMD_CLEAR: u8 = 0x03;
pub const CMD_NOP: u8 = 0x04;
pub const CMD_WRITE_PATTERN_BUFFER: u8 = 0x10;
pub const CMD_CONFIG_PATTERN: u8 = 0x11;
pub const CMD_CHANGE_PATTERN_BANK: u8 = 0x12;
pub const CMD_WRITE_PATTERN_COMPRESSED: u8 = 0x13;
pub const CMD_WRITE_MOD_BUFFER: u8 = 0x20;
pub const CMD_CONFIG_MOD: u8 = 0x21;
pub const CMD_CHANGE_MOD_BANK: u8 = 0x22;
pub const CMD_SET_SILENCER: u8 = 0x30;
pub const CMD_SET_PHASE_CORR: u8 = 0x40;
pub const CMD_SET_OUTPUT_MASK: u8 = 0x41;
pub const CMD_SET_PWE: u8 = 0x42;
pub const CMD_EMULATE_GPIO_IN: u8 = 0x50;
pub const CMD_SET_GPIO_OUT: u8 = 0x52;
pub const CMD_FORCE_FAN: u8 = 0x60;
pub const CMD_READ_ERROR_DETAIL: u8 = 0xE0;
pub const CMD_READ_CPU_FW_VERSION_MAJOR: u8 = 0xE1;
pub const CMD_READ_CPU_FW_VERSION_MINOR: u8 = 0xE2;
pub const CMD_READ_CPU_FW_VERSION_PATCH: u8 = 0xE3;
pub const CMD_READ_FPGA_FW_VERSION_MAJOR: u8 = 0xE4;
pub const CMD_READ_FPGA_FW_VERSION_MINOR: u8 = 0xE5;
pub const CMD_READ_FPGA_FW_VERSION_PATCH: u8 = 0xE6;
pub const CMD_READ_FPGA_STATE: u8 = 0xE7;
pub const CMD_XOR_HASH: u8 = 0xF0;

pub const MOD_BUFFER_SAMPLES: u32 = 65536;
pub const EMISSION_RAM_WORDS: u32 = 262_144;
pub const EMISSION_SLOT_WORDS: u32 = 256;
pub const FOCUS_WORDS: u32 = 4;
pub const MAX_FOCI_TOTAL: u32 = 65536;

pub const EM_WRITE_OFFSET_BANK: usize = 0;
pub const EM_WRITE_OFFSET_OFFSET: usize = 2;
pub const EM_WRITE_OFFSET_DATA_LEN: usize = 6;
pub const EM_WRITE_OFFSET_DATA: usize = 8;
pub const EM_WRITE_MAX_DATA_LEN: usize = PAYLOAD_BYTES - EM_WRITE_OFFSET_DATA;

pub const EM_COMPRESSED_OFFSET_BANK: usize = 0;
pub const EM_COMPRESSED_OFFSET_FORMAT: usize = 1;
pub const EM_COMPRESSED_OFFSET_COUNT: usize = 2;
pub const EM_COMPRESSED_OFFSET_OFFSET: usize = 4;
pub const EM_COMPRESSED_OFFSET_DATA: usize = 8;

pub const WRITE_PATTERN_FORMAT_PHASE_FULL: u8 = 1;
pub const WRITE_PATTERN_FORMAT_PHASE_HALF: u8 = 2;

pub const MOD_WRITE_OFFSET_BANK: usize = 0;
pub const MOD_WRITE_OFFSET_OFFSET: usize = 2;
pub const MOD_WRITE_OFFSET_DATA_LEN: usize = 6;
pub const MOD_WRITE_OFFSET_DATA: usize = 8;
pub const MOD_WRITE_MAX_DATA_LEN: usize = PAYLOAD_BYTES - MOD_WRITE_OFFSET_DATA;

pub const MOD_CONFIG_OFFSET_BANK: usize = 0;
pub const MOD_CONFIG_OFFSET_DIVIDER: usize = 2;
pub const MOD_CONFIG_OFFSET_SIZE: usize = 4;
pub const MOD_CONFIG_OFFSET_REP: usize = 8;

pub const EM_CONFIG_OFFSET_BANK: usize = 0;
pub const EM_CONFIG_OFFSET_TYPE: usize = 1;
pub const EM_CONFIG_OFFSET_DIVIDER: usize = 2;
pub const EM_CONFIG_OFFSET_SIZE: usize = 4;
pub const EM_CONFIG_OFFSET_NUM_FOCI: usize = 8;
pub const EM_CONFIG_OFFSET_SOUND_SPEED: usize = 10;
pub const EM_CONFIG_OFFSET_REP: usize = 12;

pub const CHANGE_BANK_OFFSET_BANK: usize = 0;
pub const CHANGE_BANK_OFFSET_TRANSITION_MODE: usize = 1;
pub const CHANGE_BANK_OFFSET_TRANSITION_VALUE: usize = 2;

pub const SILENCER_OFFSET_FLAG: usize = 0;
pub const SILENCER_OFFSET_UPDATE_RATE_INTENSITY: usize = 2;
pub const SILENCER_OFFSET_UPDATE_RATE_PHASE: usize = 4;
pub const SILENCER_OFFSET_COMPLETION_STEPS_INTENSITY: usize = 6;
pub const SILENCER_OFFSET_COMPLETION_STEPS_PHASE: usize = 8;

pub const SILENCER_FLAG_BIT_STRICT_MODE: u8 = 1;
pub const SILENCER_FLAG_STRICT_MODE: u8 = 1 << SILENCER_FLAG_BIT_STRICT_MODE;

pub const FORCE_FAN_OFFSET_VALUE: usize = 0;

pub const GPIO_IN_OFFSET_FLAG: usize = 0;
pub const GPIO_IN_FLAG_MASK: u8 = 0x0F;

pub const PHASE_CORR_OFFSET_DATA: usize = 0;

pub const OUTPUT_MASK_OFFSET_DATA: usize = 0;
pub const OUTPUT_MASK_WORDS: usize = crate::params::NUM_TRANSDUCERS.div_ceil(16);

pub const PWE_OFFSET_DATA: usize = 0;

pub const GPIO_OUT_OFFSET_DATA: usize = 0;
pub const GPIO_OUT_NUM: usize = 4;

pub const ERR_NONE: u8 = 0x00;
pub const ERR_UNKNOWN_CMD: u8 = 0x01;
pub const ERR_INVALID_PAYLOAD: u8 = 0x02;
pub const ERR_INVALID_DATA: u8 = 0x03;
pub const ERR_INVALID_SILENCER_SETTING: u8 = 0x04;
pub const ERR_INVALID_TRANSITION_MODE: u8 = 0x05;
pub const ERR_MISS_TRANSITION_TIME: u8 = 0x06;
pub const ERR_FPGA_TIMEOUT: u8 = 0x07;
pub const ERR_SYNC_NOT_READY: u8 = 0x08;
pub const ERR_INVALID_SYNC0_CYCLE: u8 = 0x09;

pub const MODE_FIFO: u8 = 0x00;
pub const MODE_LOW_LATENCY: u8 = 0x01;

pub const SET_MODE_OFFSET_MODE: usize = 0;

pub const XOR_HASH_OFFSET_SLEEP_MS: usize = 0;
pub const XOR_HASH_OFFSET_DATA_LEN: usize = 2;
pub const XOR_HASH_OFFSET_DATA: usize = 4;
pub const XOR_HASH_MAX_DATA_LEN: usize = PAYLOAD_BYTES - XOR_HASH_OFFSET_DATA;

const PAYLOAD_HEAD_BYTES: usize = WIRE_RX_GAP_START - 2;

const _: () = assert!(crate::params::NUM_TRANSDUCERS <= EMISSION_SLOT_WORDS as usize);
const _: () = assert!(crate::params::NUM_TRANSDUCERS * 2 <= EM_WRITE_MAX_DATA_LEN);

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

pub(crate) struct ProtoState {
    pub expected_seq: AtomicU8,
    pub fw_version_major: Cell<u8>,
    pub fw_version_minor: Cell<u8>,
    pub fw_version_patch: Cell<u8>,
    pub error_detail: Cell<u8>,
}

impl ProtoState {
    pub(crate) const fn new() -> Self {
        Self {
            expected_seq: AtomicU8::new(0),
            fw_version_major: Cell::new(crate::version::FW_VERSION_MAJOR),
            fw_version_minor: Cell::new(crate::version::FW_VERSION_MINOR),
            fw_version_patch: Cell::new(crate::version::FW_VERSION_PATCH),
            error_detail: Cell::new(ERR_NONE),
        }
    }

    pub(crate) fn init(&self) {
        self.expected_seq
            .store(0, core::sync::atomic::Ordering::Relaxed);
        self.fw_version_major.set(crate::version::FW_VERSION_MAJOR);
        self.fw_version_minor.set(crate::version::FW_VERSION_MINOR);
        self.fw_version_patch.set(crate::version::FW_VERSION_PATCH);
        self.error_detail.set(ERR_NONE);
    }
}

#[derive(FromBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternPayload {
    pub bank: u8,
    _reserved: u8,
    pub offset: U32,
    pub data_len: U16,
    pub data: [u8; EM_WRITE_MAX_DATA_LEN],
}

const _: () = assert!(size_of::<WritePatternPayload>() == PAYLOAD_BYTES);
const _: () = assert!(offset_of!(WritePatternPayload, bank) == EM_WRITE_OFFSET_BANK);
const _: () = assert!(offset_of!(WritePatternPayload, offset) == EM_WRITE_OFFSET_OFFSET);
const _: () = assert!(offset_of!(WritePatternPayload, data_len) == EM_WRITE_OFFSET_DATA_LEN);
const _: () = assert!(offset_of!(WritePatternPayload, data) == EM_WRITE_OFFSET_DATA);

#[derive(FromBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternCompressedPayload {
    pub bank: u8,
    pub format: u8,
    pub count: u8,
    _reserved: u8,
    pub offset: U32,
    pub data: [u8; crate::params::NUM_TRANSDUCERS * 2],
}

const _: () = assert!(offset_of!(WritePatternCompressedPayload, bank) == EM_COMPRESSED_OFFSET_BANK);
const _: () =
    assert!(offset_of!(WritePatternCompressedPayload, format) == EM_COMPRESSED_OFFSET_FORMAT);
const _: () =
    assert!(offset_of!(WritePatternCompressedPayload, count) == EM_COMPRESSED_OFFSET_COUNT);
const _: () =
    assert!(offset_of!(WritePatternCompressedPayload, offset) == EM_COMPRESSED_OFFSET_OFFSET);
const _: () = assert!(offset_of!(WritePatternCompressedPayload, data) == EM_COMPRESSED_OFFSET_DATA);

#[derive(FromBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WriteModPayload {
    pub bank: u8,
    _reserved: u8,
    pub offset: U32,
    pub data_len: U16,
    pub data: [u8; MOD_WRITE_MAX_DATA_LEN],
}

const _: () = assert!(size_of::<WriteModPayload>() == PAYLOAD_BYTES);
const _: () = assert!(offset_of!(WriteModPayload, bank) == MOD_WRITE_OFFSET_BANK);
const _: () = assert!(offset_of!(WriteModPayload, offset) == MOD_WRITE_OFFSET_OFFSET);
const _: () = assert!(offset_of!(WriteModPayload, data_len) == MOD_WRITE_OFFSET_DATA_LEN);
const _: () = assert!(offset_of!(WriteModPayload, data) == MOD_WRITE_OFFSET_DATA);

#[derive(FromBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigModPayload {
    pub bank: u8,
    _reserved: u8,
    pub divider: U16,
    pub size: U32,
    pub rep: U16,
}

const _: () = assert!(offset_of!(ConfigModPayload, bank) == MOD_CONFIG_OFFSET_BANK);
const _: () = assert!(offset_of!(ConfigModPayload, divider) == MOD_CONFIG_OFFSET_DIVIDER);
const _: () = assert!(offset_of!(ConfigModPayload, size) == MOD_CONFIG_OFFSET_SIZE);
const _: () = assert!(offset_of!(ConfigModPayload, rep) == MOD_CONFIG_OFFSET_REP);

#[derive(FromBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigPatternPayload {
    pub bank: u8,
    pub emission_type: u8,
    pub divider: U16,
    pub size: U32,
    pub num_foci: u8,
    _reserved: u8,
    pub sound_speed: U16,
    pub rep: U16,
}

const _: () = assert!(offset_of!(ConfigPatternPayload, bank) == EM_CONFIG_OFFSET_BANK);
const _: () = assert!(offset_of!(ConfigPatternPayload, emission_type) == EM_CONFIG_OFFSET_TYPE);
const _: () = assert!(offset_of!(ConfigPatternPayload, divider) == EM_CONFIG_OFFSET_DIVIDER);
const _: () = assert!(offset_of!(ConfigPatternPayload, size) == EM_CONFIG_OFFSET_SIZE);
const _: () = assert!(offset_of!(ConfigPatternPayload, num_foci) == EM_CONFIG_OFFSET_NUM_FOCI);
const _: () =
    assert!(offset_of!(ConfigPatternPayload, sound_speed) == EM_CONFIG_OFFSET_SOUND_SPEED);
const _: () = assert!(offset_of!(ConfigPatternPayload, rep) == EM_CONFIG_OFFSET_REP);

#[derive(FromBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ChangeBankPayload {
    pub bank: u8,
    pub transition_mode: u8,
    pub transition_value: U64,
}

const _: () = assert!(offset_of!(ChangeBankPayload, bank) == CHANGE_BANK_OFFSET_BANK);
const _: () =
    assert!(offset_of!(ChangeBankPayload, transition_mode) == CHANGE_BANK_OFFSET_TRANSITION_MODE);
const _: () =
    assert!(offset_of!(ChangeBankPayload, transition_value) == CHANGE_BANK_OFFSET_TRANSITION_VALUE);

#[derive(FromBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct SilencerPayload {
    pub flag: u8,
    _reserved: u8,
    pub update_rate_intensity: U16,
    pub update_rate_phase: U16,
    pub completion_steps_intensity: U16,
    pub completion_steps_phase: U16,
}

const _: () = assert!(offset_of!(SilencerPayload, flag) == SILENCER_OFFSET_FLAG);
const _: () = assert!(
    offset_of!(SilencerPayload, update_rate_intensity) == SILENCER_OFFSET_UPDATE_RATE_INTENSITY
);
const _: () =
    assert!(offset_of!(SilencerPayload, update_rate_phase) == SILENCER_OFFSET_UPDATE_RATE_PHASE);
const _: () = assert!(
    offset_of!(SilencerPayload, completion_steps_intensity)
        == SILENCER_OFFSET_COMPLETION_STEPS_INTENSITY
);
const _: () = assert!(
    offset_of!(SilencerPayload, completion_steps_phase) == SILENCER_OFFSET_COMPLETION_STEPS_PHASE
);

#[derive(FromBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct XorHashPayload {
    pub sleep_ms: U16,
    pub data_len: U16,
    pub data: [u8; XOR_HASH_MAX_DATA_LEN],
}

const _: () = assert!(size_of::<XorHashPayload>() == PAYLOAD_BYTES);
const _: () = assert!(offset_of!(XorHashPayload, sleep_ms) == XOR_HASH_OFFSET_SLEEP_MS);
const _: () = assert!(offset_of!(XorHashPayload, data_len) == XOR_HASH_OFFSET_DATA_LEN);
const _: () = assert!(offset_of!(XorHashPayload, data) == XOR_HASH_OFFSET_DATA);
