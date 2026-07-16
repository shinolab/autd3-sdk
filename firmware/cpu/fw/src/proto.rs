use core::cell::Cell;
use core::sync::atomic::AtomicU8;

pub const RX_FRAME_BYTES: usize = 626;
pub const PAYLOAD_BYTES: usize = 624;
pub const TX_FRAME_BYTES: usize = 2;

pub const WIRE_RX_FRAME_BYTES: usize = RX_FRAME_BYTES + 2;
pub const WIRE_RX_GAP_START: usize = 498;
pub const WIRE_RX_GAP_END: usize = 500;

macro_rules! wire_enum {
    ($vis:vis enum $name:ident { $($variant:ident = $value:expr,)+ }) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        #[repr(u8)]
        $vis enum $name {
            $($variant = $value,)+
        }

        impl $name {
            #[must_use]
            $vis const fn from_u8(value: u8) -> Option<Self> {
                $(if value == $value {
                    return Some(Self::$variant);
                })+
                None
            }
        }
    };
}
pub(crate) use wire_enum;

wire_enum! {
    pub enum Cmd {
        Reset = 0x00,
        Synchronize = 0x01,
        SetMode = 0x02,
        Clear = 0x03,
        Nop = 0x04,
        Stop = 0x05,
        WritePatternBuffer = 0x10,
        ConfigPattern = 0x11,
        ChangePatternBank = 0x12,
        WritePatternCompressed = 0x13,
        WriteModBuffer = 0x20,
        ConfigMod = 0x21,
        ChangeModBank = 0x22,
        SetSilencer = 0x30,
        SetPhaseCorr = 0x40,
        SetOutputMask = 0x41,
        SetPwe = 0x42,
        EmulateGpioIn = 0x50,
        SetGpioOut = 0x52,
        ForceFan = 0x60,
        ReadErrorDetail = 0xE0,
        ReadCpuFwVersionMajor = 0xE1,
        ReadCpuFwVersionMinor = 0xE2,
        ReadCpuFwVersionPatch = 0xE3,
        ReadFpgaFwVersionMajor = 0xE4,
        ReadFpgaFwVersionMinor = 0xE5,
        ReadFpgaFwVersionPatch = 0xE6,
        ReadFpgaState = 0xE7,
        ReadTelemetry = 0xE8,
        ReadFpgaFunctions = 0xE9,
        XorHash = 0xF0,
    }
}

wire_enum! {
    pub enum Error {
        UnknownCmd = 0x01,
        InvalidPayload = 0x02,
        InvalidData = 0x03,
        InvalidSilencerSetting = 0x04,
        InvalidTransitionMode = 0x05,
        MissTransitionTime = 0x06,
        FpgaTimeout = 0x07,
        SyncNotReady = 0x08,
        InvalidSync0Cycle = 0x09,
    }
}

wire_enum! {
    pub enum Mode {
        Fifo = 0x00,
        LowLatency = 0x01,
    }
}

wire_enum! {
    pub enum Telemetry {
        FifoDrop = 0x00,
        Dedup = 0x01,
        SeqMismatch = 0x02,
        DispatchError = 0x03,
        Processed = 0x04,
        Failsafe = 0x05,
    }
}

impl Telemetry {
    pub const COUNT: usize = 6;
}

pub const MOD_BUFFER_SAMPLES: u32 = 65536;
pub const EMISSION_RAM_WORDS: u32 = 262_144; // 256 * 1024 (raw) = 4 * 65536 (foci)
pub const EMISSION_SLOT_WORDS: u32 = 256;
pub const FOCUS_WORDS: u32 = 4;
pub const MAX_FOCI_TOTAL: u32 = 65536;

pub const OUTPUT_MASK_WORDS: usize = crate::params::NUM_TRANSDUCERS.div_ceil(16);

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

pub(crate) struct ProtoState {
    pub expected_seq: AtomicU8,
    pub fw_version_major: Cell<u8>,
    pub fw_version_minor: Cell<u8>,
    pub fw_version_patch: Cell<u8>,
    pub error_detail: Cell<Option<Error>>,
}

impl ProtoState {
    pub(crate) const fn new() -> Self {
        Self {
            expected_seq: AtomicU8::new(0),
            fw_version_major: Cell::new(crate::version::FW_VERSION_MAJOR),
            fw_version_minor: Cell::new(crate::version::FW_VERSION_MINOR),
            fw_version_patch: Cell::new(crate::version::FW_VERSION_PATCH),
            error_detail: Cell::new(None),
        }
    }

    pub(crate) fn init(&self) {
        self.expected_seq
            .store(0, core::sync::atomic::Ordering::Relaxed);
        self.fw_version_major.set(crate::version::FW_VERSION_MAJOR);
        self.fw_version_minor.set(crate::version::FW_VERSION_MINOR);
        self.fw_version_patch.set(crate::version::FW_VERSION_PATCH);
        self.error_detail.set(None);
    }
}
