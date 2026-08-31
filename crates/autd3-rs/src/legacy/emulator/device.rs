use autd3_rs_core::value::{Emission, Intensity, Phase};

use crate::legacy::error::{
    INVALID_GAIN_STM_MODE, INVALID_INFO_TYPE, INVALID_MSG_ID, INVALID_SEGMENT_TRANSITION,
    INVALID_SILENCER_SETTINGS, INVALID_TRANSITION_MODE, MISS_TRANSITION_TIME, NO_ERROR,
    NOT_SUPPORTED_TAG,
};
use crate::legacy::wire::params::{
    FOCI_STM_FLAG_BEGIN, FOCI_STM_FLAG_END, FOCI_STM_FLAG_TRANSITION, GAIN_FLAG_UPDATE,
    GAIN_STM_FLAG_BEGIN, GAIN_STM_FLAG_END, GAIN_STM_FLAG_SEGMENT, GAIN_STM_FLAG_TRANSITION,
    MODULATION_FLAG_BEGIN, MODULATION_FLAG_END, MODULATION_FLAG_SEGMENT,
    MODULATION_FLAG_TRANSITION, PWE_TABLE_SIZE, REP_INFINITE,
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    SILENCER_DEFAULT_UPDATE_RATE, SILENCER_FLAG_FIXED_UPDATE_RATE_MODE, SILENCER_FLAG_STRICT_MODE,
    SYS_TIME_TRANSITION_MARGIN_NS, TRANSITION_MODE_GPIO, TRANSITION_MODE_IMMEDIATE,
    TRANSITION_MODE_NONE, TRANSITION_MODE_SYNC_IDX, TRANSITION_MODE_SYS_TIME,
};
use crate::legacy::wire::{
    Ack, FpgaState, MsgId, RX_FRAME_BYTES, RxFrame, TX_FRAME_BYTES, TxFrame,
};

const LAST_MSG_ID_INIT: u8 = 0xFF;

const GAIN_STM_MODE_PHASE_INTENSITY_FULL: u8 = 0;
const GAIN_STM_MODE_PHASE_FULL: u8 = 1;
const GAIN_STM_MODE_PHASE_HALF: u8 = 2;

const MOD_CYCLE_INIT: u32 = 2;

const DEFAULT_CYCLE_PERIOD_NS: u64 = 1_000_000;

const ASIN_TABLE: [u8; PWE_TABLE_SIZE] = [
    0x00, 0x01, 0x01, 0x02, 0x03, 0x03, 0x04, 0x04, 0x05, 0x06, 0x06, 0x07, 0x08, 0x08, 0x09, 0x0a,
    0x0a, 0x0b, 0x0c, 0x0c, 0x0d, 0x0d, 0x0e, 0x0f, 0x0f, 0x10, 0x11, 0x11, 0x12, 0x13, 0x13, 0x14,
    0x15, 0x15, 0x16, 0x16, 0x17, 0x18, 0x18, 0x19, 0x1a, 0x1a, 0x1b, 0x1c, 0x1c, 0x1d, 0x1e, 0x1e,
    0x1f, 0x20, 0x20, 0x21, 0x21, 0x22, 0x23, 0x23, 0x24, 0x25, 0x25, 0x26, 0x27, 0x27, 0x28, 0x29,
    0x29, 0x2a, 0x2b, 0x2b, 0x2c, 0x2d, 0x2d, 0x2e, 0x2f, 0x2f, 0x30, 0x31, 0x31, 0x32, 0x33, 0x33,
    0x34, 0x35, 0x35, 0x36, 0x37, 0x37, 0x38, 0x39, 0x39, 0x3a, 0x3b, 0x3b, 0x3c, 0x3d, 0x3e, 0x3e,
    0x3f, 0x40, 0x40, 0x41, 0x42, 0x42, 0x43, 0x44, 0x44, 0x45, 0x46, 0x47, 0x47, 0x48, 0x49, 0x49,
    0x4a, 0x4b, 0x4c, 0x4c, 0x4d, 0x4e, 0x4e, 0x4f, 0x50, 0x51, 0x51, 0x52, 0x53, 0x53, 0x54, 0x55,
    0x56, 0x56, 0x57, 0x58, 0x59, 0x59, 0x5a, 0x5b, 0x5c, 0x5c, 0x5d, 0x5e, 0x5f, 0x5f, 0x60, 0x61,
    0x62, 0x63, 0x63, 0x64, 0x65, 0x66, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e,
    0x6f, 0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x7a, 0x7b,
    0x7c, 0x7d, 0x7e, 0x7f, 0x80, 0x81, 0x82, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a,
    0x8b, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a,
    0x9b, 0x9d, 0x9e, 0x9f, 0xa0, 0xa1, 0xa2, 0xa3, 0xa5, 0xa6, 0xa7, 0xa8, 0xaa, 0xab, 0xac, 0xad,
    0xaf, 0xb0, 0xb2, 0xb3, 0xb4, 0xb6, 0xb7, 0xb9, 0xba, 0xbc, 0xbd, 0xbf, 0xc1, 0xc2, 0xc4, 0xc6,
    0xc8, 0xca, 0xcc, 0xce, 0xd0, 0xd2, 0xd5, 0xd7, 0xda, 0xdd, 0xe0, 0xe3, 0xe7, 0xec, 0xf2, 0x00,
];

#[must_use]
#[allow(clippy::cast_lossless)]
pub const fn default_pulse_width_table() -> [u16; PWE_TABLE_SIZE] {
    let mut table = [0u16; PWE_TABLE_SIZE];
    let mut i = 0;
    while i < PWE_TABLE_SIZE {
        table[i] = ASIN_TABLE[i] as u16;
        i += 1;
    }
    table[PWE_TABLE_SIZE - 1] = 0x0100;
    table
}

fn prefix(data: &[u8], len: usize) -> Option<&[u8]> {
    data.get(..len)
}

fn gain_stm_emission(chunk: [u8; 2], mode: u8, shift: u32) -> Emission {
    if mode == GAIN_STM_MODE_PHASE_INTENSITY_FULL {
        return Emission {
            phase: Phase(chunk[0]),
            intensity: Intensity(chunk[1]),
        };
    }
    let word = u16::from_le_bytes(chunk);
    #[allow(clippy::cast_possible_truncation)]
    let phase = if mode == GAIN_STM_MODE_PHASE_FULL {
        Phase((word >> shift) as u8)
    } else {
        let nibble = ((word >> shift) & 0x0F) as u8;
        Phase(nibble << 4 | nibble)
    };
    Emission {
        phase,
        intensity: Intensity::MAX,
    }
}

fn body(data: &[u8], offset: usize, len: usize) -> Option<&[u8]> {
    data.get(offset..offset.checked_add(len)?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StmKind {
    Gain,
    Foci,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SegmentState {
    pub kind: StmKind,
    pub emissions: Vec<Vec<Emission>>,
    pub foci: Vec<u64>,
    pub cycle: u32,
    pub freq_div: u16,
    pub rep: u16,
    pub sound_speed: u16,
    pub modulation: Vec<u8>,
    pub mod_freq_div: u16,
    pub mod_rep: u16,
}

impl Default for SegmentState {
    fn default() -> Self {
        Self {
            kind: StmKind::Gain,
            emissions: Vec::new(),
            foci: Vec::new(),
            cycle: 1,
            freq_div: 0xFFFF,
            rep: REP_INFINITE,
            sound_speed: 0,
            modulation: Vec::new(),
            mod_freq_div: 0xFFFF,
            mod_rep: REP_INFINITE,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LegacyDevice {
    idx: usize,
    num_transducers: usize,
    last_msg_id: u8,
    ack: u8,
    err: u8,
    rx_data: u8,
    is_rx_data_used: bool,
    reads_fpga_state: bool,
    synchronized: bool,
    force_fan: bool,
    force_fan_pending: bool,
    thermal_assert: bool,
    cpu_version: (u8, u8),
    fpga_version: (u8, u8),
    fpga_functions: u8,
    dc_sys_time_ns: u64,
    cycle_period_ns: u64,
    segments: [SegmentState; 2],
    stm_segment: u8,
    mod_segment: u8,
    num_foci: u8,
    gain_stm_mode: u8,
    silencer_strict: bool,
    silencer_update_rate: (u16, u16),
    silencer_completion_steps: (u16, u16),
    silencer_fixed_update_rate: bool,
    output_mask: [Vec<bool>; 2],
    phase_correction: Vec<Phase>,
    pulse_width_table: Vec<u16>,
    gpio_out: [u64; 4],
    gpio_in: [bool; 4],
    gpio_in_pending: [bool; 4],
    cpu_gpio_out: u8,
    segment_out_of_range: bool,
    wedged: bool,
    mod_cycle: u32,
    stm_write_cursor: u32,
    stm_transition_mode: u8,
    stm_transition_value: u64,
    mod_transition_mode: u8,
    mod_transition_value: u64,
}

impl LegacyDevice {
    #[must_use]
    pub fn new(idx: usize, num_transducers: usize) -> Self {
        let mut device = Self {
            idx,
            num_transducers,
            last_msg_id: LAST_MSG_ID_INIT,
            ack: 0,
            err: NO_ERROR,
            rx_data: 0,
            is_rx_data_used: false,
            reads_fpga_state: false,
            synchronized: false,
            force_fan: false,
            force_fan_pending: false,
            thermal_assert: false,
            cpu_version: (crate::legacy::wire::params::CPU_VERSION_V12_1, 0x00),
            fpga_version: (crate::legacy::wire::params::CPU_VERSION_V12_1, 0x00),
            fpga_functions: 0,
            dc_sys_time_ns: autd3_rs_core::value::DcSysTime::now()
                .map_or(0, autd3_rs_core::value::DcSysTime::sys_time),
            cycle_period_ns: DEFAULT_CYCLE_PERIOD_NS,
            segments: [SegmentState::default(), SegmentState::default()],
            stm_segment: 0,
            mod_segment: 0,
            num_foci: 0,
            gain_stm_mode: GAIN_STM_MODE_PHASE_INTENSITY_FULL,
            silencer_strict: true,
            silencer_update_rate: (SILENCER_DEFAULT_UPDATE_RATE, SILENCER_DEFAULT_UPDATE_RATE),
            silencer_completion_steps: (
                SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY,
                SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
            ),
            silencer_fixed_update_rate: false,
            output_mask: [vec![true; num_transducers], vec![true; num_transducers]],
            phase_correction: vec![Phase::ZERO; num_transducers],
            pulse_width_table: default_pulse_width_table().to_vec(),
            gpio_out: [0; 4],
            gpio_in: [false; 4],
            gpio_in_pending: [false; 4],
            cpu_gpio_out: 0,
            segment_out_of_range: false,
            wedged: false,
            mod_cycle: MOD_CYCLE_INIT,
            stm_write_cursor: 0,
            stm_transition_mode: TRANSITION_MODE_SYNC_IDX,
            stm_transition_value: 0,
            mod_transition_mode: TRANSITION_MODE_SYNC_IDX,
            mod_transition_value: 0,
        };
        device.clear();
        device
    }

    #[must_use]
    pub const fn idx(&self) -> usize {
        self.idx
    }

    #[must_use]
    pub const fn num_transducers(&self) -> usize {
        self.num_transducers
    }

    #[must_use]
    pub const fn synchronized(&self) -> bool {
        self.synchronized
    }

    #[must_use]
    pub const fn force_fan(&self) -> bool {
        self.force_fan
    }

    #[must_use]
    pub const fn reads_fpga_state(&self) -> bool {
        self.reads_fpga_state
    }

    #[must_use]
    pub const fn silencer_strict(&self) -> bool {
        self.silencer_strict
    }

    #[must_use]
    pub const fn silencer_fixed_update_rate(&self) -> bool {
        self.silencer_fixed_update_rate
    }

    #[must_use]
    pub const fn silencer_update_rate(&self) -> (u16, u16) {
        self.silencer_update_rate
    }

    #[must_use]
    pub const fn silencer_completion_steps(&self) -> (u16, u16) {
        self.silencer_completion_steps
    }

    #[must_use]
    pub const fn err(&self) -> u8 {
        self.err
    }

    #[must_use]
    pub const fn num_foci(&self) -> u8 {
        self.num_foci
    }

    #[must_use]
    pub const fn gain_stm_mode(&self) -> u8 {
        self.gain_stm_mode
    }

    #[must_use]
    pub const fn mod_cycle(&self) -> u32 {
        self.mod_cycle
    }

    #[must_use]
    pub fn output_mask(&self, segment: crate::legacy::wire::Segment) -> &[bool] {
        &self.output_mask[segment.as_u8() as usize]
    }

    #[must_use]
    pub fn phase_correction(&self) -> &[Phase] {
        &self.phase_correction
    }

    #[must_use]
    pub fn pulse_width_table(&self) -> &[u16] {
        &self.pulse_width_table
    }

    #[must_use]
    pub const fn gpio_out(&self) -> [u64; 4] {
        self.gpio_out
    }

    #[must_use]
    pub const fn gpio_in(&self) -> [bool; 4] {
        self.gpio_in
    }

    #[must_use]
    pub const fn cpu_gpio_out(&self) -> u8 {
        self.cpu_gpio_out
    }

    #[must_use]
    pub const fn segment_out_of_range(&self) -> bool {
        self.segment_out_of_range
    }

    #[must_use]
    pub const fn dc_sys_time_ns(&self) -> u64 {
        self.dc_sys_time_ns
    }

    #[must_use]
    pub fn segment(&self, segment: crate::legacy::wire::Segment) -> &SegmentState {
        &self.segments[segment.as_u8() as usize]
    }

    #[must_use]
    pub fn current_stm_segment(&self) -> crate::legacy::wire::Segment {
        if self.stm_segment == 0 {
            crate::legacy::wire::Segment::S0
        } else {
            crate::legacy::wire::Segment::S1
        }
    }

    #[must_use]
    pub fn current_mod_segment(&self) -> crate::legacy::wire::Segment {
        if self.mod_segment == 0 {
            crate::legacy::wire::Segment::S0
        } else {
            crate::legacy::wire::Segment::S1
        }
    }

    #[must_use]
    pub const fn stm_transition(&self) -> (u8, u64) {
        (self.stm_transition_mode, self.stm_transition_value)
    }

    #[must_use]
    pub const fn mod_transition(&self) -> (u8, u64) {
        (self.mod_transition_mode, self.mod_transition_value)
    }

    pub const fn set_dc_sys_time(&mut self, ns: u64) {
        self.dc_sys_time_ns = ns;
    }

    pub const fn set_cycle_period_ns(&mut self, ns: u64) {
        self.cycle_period_ns = ns;
    }

    pub const fn set_thermal_assert(&mut self, value: bool) {
        self.thermal_assert = value;
    }

    pub const fn set_cpu_version(&mut self, major: u8, minor: u8) {
        self.cpu_version = (major, minor);
    }

    pub const fn set_fpga_version(&mut self, major: u8, minor: u8, functions: u8) {
        self.fpga_version = (major, minor);
        self.fpga_functions = functions;
    }

    #[must_use]
    pub fn fpga_state(&self) -> FpgaState {
        let mut bits = 0u8;
        if self.thermal_assert {
            bits |= 1 << 0;
        }
        bits |= self.mod_segment << 1;
        bits |= self.stm_segment << 2;
        if self.segments[self.stm_segment as usize].cycle <= 1 {
            bits |= 1 << 3;
        }
        FpgaState(bits)
    }

    #[must_use]
    pub fn rx(&self) -> RxFrame {
        RxFrame::new(self.rx_data, Ack::new(self.ack, self.err))
    }

    pub const fn set_ack_state(&mut self, ack: u8, err: u8) {
        self.ack = ack;
        self.err = err;
    }

    pub const fn set_last_msg_id(&mut self, msg_id: u8) {
        self.last_msg_id = msg_id;
    }

    pub const fn wedge(&mut self) {
        self.wedged = true;
    }

    pub fn cycle(&mut self, tx: &[u8; TX_FRAME_BYTES], rx: &mut [u8; RX_FRAME_BYTES]) {
        self.rx().write_to(rx);
        if self.wedged {
            return;
        }
        self.dc_sys_time_ns = self.dc_sys_time_ns.wrapping_add(self.cycle_period_ns);
        self.refresh_rx_data();
        self.recv(tx);
    }

    fn recv(&mut self, tx: &[u8; TX_FRAME_BYTES]) {
        let frame = TxFrame::parse(tx);
        let msg_id = MsgId::new(frame.header.msg_id);
        if self.last_msg_id == msg_id.get() {
            return;
        }
        self.last_msg_id = msg_id.get();
        self.ack = msg_id.get() & 0x0F;

        if msg_id > MsgId::MAX {
            self.err = INVALID_MSG_ID;
            return;
        }

        self.err = self.handle_payload(&frame.payload);
        if self.err != NO_ERROR {
            return;
        }
        let slot_2 = usize::from(frame.header.slot_2_offset);
        if slot_2 != 0 {
            self.err = match frame.payload.get(slot_2..) {
                Some(payload) => self.handle_payload(payload),
                None => NOT_SUPPORTED_TAG,
            };
            if self.err != NO_ERROR {
                return;
            }
        }
        self.force_fan = self.force_fan_pending;
        self.gpio_in = self.gpio_in_pending;
    }

    fn refresh_rx_data(&mut self) {
        if self.is_rx_data_used {
            return;
        }
        if self.reads_fpga_state {
            self.rx_data = FpgaState::READS_FPGA_STATE_ENABLED | self.fpga_state().0;
        } else {
            self.rx_data &= !FpgaState::READS_FPGA_STATE_ENABLED;
        }
    }

    fn handle_payload(&mut self, data: &[u8]) -> u8 {
        use crate::legacy::wire::Tag;

        let Some(&tag) = data.first() else {
            return NOT_SUPPORTED_TAG;
        };
        match tag {
            v if v == Tag::Nop.as_u8() => NO_ERROR,
            v if v == Tag::Clear.as_u8() => {
                self.clear();
                NO_ERROR
            }
            v if v == Tag::Sync.as_u8() => {
                self.synchronized = true;
                NO_ERROR
            }
            v if v == Tag::FirmInfo.as_u8() => self.firm_info(data),
            v if v == Tag::Modulation.as_u8() => self.write_modulation(data),
            v if v == Tag::ModulationLegacyChangePatternBank.as_u8() => {
                self.change_mod_segment(data)
            }
            v if v == Tag::Silencer.as_u8() => self.config_silencer(data),
            v if v == Tag::Gain.as_u8() => self.write_gain(data),
            v if v == Tag::GainLegacyChangePatternBank.as_u8() => self.change_gain_segment(data),
            v if v == Tag::GainStm.as_u8() => self.write_gain_stm(data),
            v if v == Tag::FociStm.as_u8() => self.write_foci_stm(data),
            v if v == Tag::GainStmLegacyChangePatternBank.as_u8() => {
                self.change_stm_segment(data, true)
            }
            v if v == Tag::FociStmLegacyChangePatternBank.as_u8() => {
                self.change_stm_segment(data, false)
            }
            v if v == Tag::ForceFan.as_u8() => match data.get(1) {
                Some(&value) => {
                    self.force_fan_pending = value != 0;
                    NO_ERROR
                }
                None => NOT_SUPPORTED_TAG,
            },
            v if v == Tag::ReadsFpgaState.as_u8() => match data.get(1) {
                Some(&value) => {
                    self.reads_fpga_state = value != 0;
                    NO_ERROR
                }
                None => NOT_SUPPORTED_TAG,
            },
            v if v == Tag::OutputMask.as_u8() => self.write_output_mask(data),
            v if v == Tag::PhaseCorrection.as_u8() => self.write_phase_correction(data),
            v if v == Tag::ConfigPulseWidthEncoder.as_u8() => self.write_pulse_width_table(data),
            v if v == Tag::FpgaGpioOut.as_u8() => self.write_gpio_out(data),
            v if v == Tag::EmulateGpioIn.as_u8() => self.write_gpio_in(data),
            v if v == Tag::CpuGpioOut.as_u8() => match data.get(1) {
                Some(&value) => {
                    self.cpu_gpio_out = value;
                    NO_ERROR
                }
                None => NOT_SUPPORTED_TAG,
            },
            _ => NOT_SUPPORTED_TAG,
        }
    }

    fn clear(&mut self) {
        self.force_fan_pending = false;
        self.gpio_in_pending = [false; 4];
        self.cpu_gpio_out = 0;

        self.silencer_strict = true;
        self.silencer_fixed_update_rate = false;
        self.silencer_update_rate = (SILENCER_DEFAULT_UPDATE_RATE, SILENCER_DEFAULT_UPDATE_RATE);
        self.silencer_completion_steps = (
            SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY,
            SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
        );

        self.segments = [SegmentState::default(), SegmentState::default()];
        for segment in &mut self.segments {
            segment.emissions = vec![vec![Emission::NULL; self.num_transducers]];
            segment.modulation = vec![0xFF; 2];
        }
        self.mod_cycle = MOD_CYCLE_INIT;
        self.stm_write_cursor = 0;
        self.stm_segment = 0;
        self.mod_segment = 0;
        self.stm_transition_mode = TRANSITION_MODE_SYNC_IDX;
        self.stm_transition_value = 0;
        self.mod_transition_mode = TRANSITION_MODE_SYNC_IDX;
        self.mod_transition_value = 0;

        self.output_mask = [
            vec![true; self.num_transducers],
            vec![true; self.num_transducers],
        ];
        self.phase_correction = vec![Phase::ZERO; self.num_transducers];
        self.pulse_width_table = default_pulse_width_table().to_vec();
        self.gpio_out = [0; 4];
    }

    fn firm_info(&mut self, data: &[u8]) -> u8 {
        use crate::legacy::wire::InfoType;

        let Some(&ty) = data.get(1) else {
            return NOT_SUPPORTED_TAG;
        };
        match ty {
            v if v == InfoType::CpuMajor.as_u8() => {
                self.is_rx_data_used = true;
                self.rx_data = self.cpu_version.0;
            }
            v if v == InfoType::CpuMinor.as_u8() => self.rx_data = self.cpu_version.1,
            v if v == InfoType::FpgaMajor.as_u8() => self.rx_data = self.fpga_version.0,
            v if v == InfoType::FpgaMinor.as_u8() => self.rx_data = self.fpga_version.1,
            v if v == InfoType::FpgaFunctions.as_u8() => self.rx_data = self.fpga_functions,
            v if v == InfoType::Clear.as_u8() => {
                self.is_rx_data_used = false;
                self.rx_data = 0;
            }
            _ => return INVALID_INFO_TYPE,
        }
        NO_ERROR
    }

    fn take_segment(&mut self, raw: u8) -> u8 {
        if raw > 1 {
            self.segment_out_of_range = true;
        }
        raw & 0x01
    }

    fn validate_transition_mode(current: u8, segment: u8, rep: u16, mode: u8) -> bool {
        if mode == TRANSITION_MODE_NONE {
            return false;
        }
        let is_sampling_synced = mode == TRANSITION_MODE_SYNC_IDX
            || mode == TRANSITION_MODE_SYS_TIME
            || mode == TRANSITION_MODE_GPIO;
        if current == segment {
            return is_sampling_synced;
        }
        if rep == REP_INFINITE {
            is_sampling_synced
        } else {
            mode == TRANSITION_MODE_IMMEDIATE
                || mode == crate::legacy::wire::params::TRANSITION_MODE_EXT
        }
    }

    fn validate_silencer(&self, stm_freq_div: u16, mod_freq_div: u16) -> bool {
        self.silencer_strict
            && (mod_freq_div < self.silencer_completion_steps.0
                || stm_freq_div < self.silencer_completion_steps.0
                || stm_freq_div < self.silencer_completion_steps.1)
    }

    fn config_silencer(&mut self, data: &[u8]) -> u8 {
        let Some(head) = prefix(data, 6) else {
            return NOT_SUPPORTED_TAG;
        };
        let flag = head[1];
        let value_intensity = u16::from_le_bytes([head[2], head[3]]);
        let value_phase = u16::from_le_bytes([head[4], head[5]]);

        if flag & SILENCER_FLAG_FIXED_UPDATE_RATE_MODE != 0 {
            self.silencer_update_rate = (value_intensity, value_phase);
            self.silencer_strict = false;
            self.silencer_fixed_update_rate = true;
            return NO_ERROR;
        }

        let restore = (
            self.silencer_strict,
            self.silencer_completion_steps,
            self.silencer_fixed_update_rate,
        );
        self.silencer_strict = flag & SILENCER_FLAG_STRICT_MODE != 0;
        self.silencer_completion_steps = (value_intensity, value_phase);
        self.silencer_fixed_update_rate = false;
        if self.validate_silencer(
            self.segments[self.stm_segment as usize].freq_div,
            self.segments[self.mod_segment as usize].mod_freq_div,
        ) {
            self.silencer_strict = restore.0;
            self.silencer_completion_steps = restore.1;
            self.silencer_fixed_update_rate = restore.2;
            return INVALID_SILENCER_SETTINGS;
        }
        NO_ERROR
    }

    fn write_gain(&mut self, data: &[u8]) -> u8 {
        let Some(head) = prefix(data, 4) else {
            return NOT_SUPPORTED_TAG;
        };
        let segment = self.take_segment(head[1]);
        let flag = head[2];
        let Some(words) = body(data, 4, self.num_transducers * size_of::<Emission>()) else {
            return NOT_SUPPORTED_TAG;
        };

        let emissions = words
            .as_chunks::<{ size_of::<Emission>() }>()
            .0
            .iter()
            .map(|chunk| Emission {
                phase: Phase(chunk[0]),
                intensity: Intensity(chunk[1]),
            })
            .collect::<Vec<_>>();

        self.stm_segment = segment;
        let state = &mut self.segments[segment as usize];
        state.emissions = vec![emissions];
        state.foci.clear();
        state.cycle = 1;
        state.freq_div = 0xFFFF;
        state.rep = REP_INFINITE;

        if flag & GAIN_FLAG_UPDATE != 0 {
            self.stm_transition_mode = TRANSITION_MODE_SYNC_IDX;
        }
        NO_ERROR
    }

    fn change_gain_segment(&mut self, data: &[u8]) -> u8 {
        let Some(head) = prefix(data, 2) else {
            return NOT_SUPPORTED_TAG;
        };
        let segment = self.take_segment(head[1]);
        let state = &self.segments[segment as usize];
        if state.kind != StmKind::Gain || state.cycle != 1 {
            return INVALID_SEGMENT_TRANSITION;
        }
        self.stm_segment = segment;
        self.stm_transition_mode = TRANSITION_MODE_SYNC_IDX;
        NO_ERROR
    }

    fn write_modulation(&mut self, data: &[u8]) -> u8 {
        let Some(head) = prefix(data, 4) else {
            return NOT_SUPPORTED_TAG;
        };
        let flag = head[1];
        let segment = u8::from(flag & MODULATION_FLAG_SEGMENT != 0);

        let (offset, size) = if flag & MODULATION_FLAG_BEGIN != 0 {
            let Some(head) = prefix(data, 16) else {
                return NOT_SUPPORTED_TAG;
            };
            let rep = u16::from_le_bytes([head[6], head[7]]);
            let transition_mode = head[3];
            if Self::validate_transition_mode(self.mod_segment, segment, rep, transition_mode) {
                return INVALID_TRANSITION_MODE;
            }
            let freq_div = u16::from_le_bytes([head[4], head[5]]);
            if self.validate_silencer(self.segments[self.stm_segment as usize].freq_div, freq_div) {
                return INVALID_SILENCER_SETTINGS;
            }
            let size = usize::from(head[2]);
            let transition_value = u64::from_le_bytes([
                head[8], head[9], head[10], head[11], head[12], head[13], head[14], head[15],
            ]);
            if transition_mode != TRANSITION_MODE_NONE {
                self.mod_segment = segment;
            }
            self.mod_cycle = 0;
            self.mod_transition_mode = transition_mode;
            self.mod_transition_value = transition_value;
            let state = &mut self.segments[segment as usize];
            state.mod_freq_div = freq_div;
            state.mod_rep = rep;
            state.modulation.clear();
            (16usize, size)
        } else {
            (4usize, usize::from(u16::from_le_bytes([head[2], head[3]])))
        };

        let Some(buffer) = body(data, offset, size) else {
            return NOT_SUPPORTED_TAG;
        };
        self.segments[segment as usize]
            .modulation
            .extend_from_slice(buffer);
        self.mod_cycle += u32::try_from(size).unwrap_or(u32::MAX);

        if flag & MODULATION_FLAG_END != 0 && flag & MODULATION_FLAG_TRANSITION != 0 {
            return self.mod_segment_update(self.mod_transition_mode, self.mod_transition_value);
        }
        NO_ERROR
    }

    fn mod_segment_update(&mut self, mode: u8, value: u64) -> u8 {
        if mode == TRANSITION_MODE_SYS_TIME
            && value < self.dc_sys_time_ns + SYS_TIME_TRANSITION_MARGIN_NS
        {
            return MISS_TRANSITION_TIME;
        }
        self.mod_transition_mode = mode;
        self.mod_transition_value = value;
        NO_ERROR
    }

    fn change_mod_segment(&mut self, data: &[u8]) -> u8 {
        let Some(head) = prefix(data, 16) else {
            return NOT_SUPPORTED_TAG;
        };
        let segment = self.take_segment(head[1]);
        let mode = head[2];
        let value = u64::from_le_bytes([
            head[8], head[9], head[10], head[11], head[12], head[13], head[14], head[15],
        ]);
        if Self::validate_transition_mode(
            self.mod_segment,
            segment,
            self.segments[segment as usize].mod_rep,
            mode,
        ) {
            return INVALID_TRANSITION_MODE;
        }
        if self.validate_silencer(
            self.segments[self.stm_segment as usize].freq_div,
            self.segments[segment as usize].mod_freq_div,
        ) {
            return INVALID_SILENCER_SETTINGS;
        }
        self.mod_segment = segment;
        self.mod_segment_update(mode, value)
    }

    fn write_foci_stm(&mut self, data: &[u8]) -> u8 {
        let Some(head) = prefix(data, 4) else {
            return NOT_SUPPORTED_TAG;
        };
        let flag = head[1];
        let send_num = usize::from(head[2]);
        let segment = self.take_segment(head[3]);

        let offset = if flag & FOCI_STM_FLAG_BEGIN != 0 {
            let Some(head) = prefix(data, 24) else {
                return NOT_SUPPORTED_TAG;
            };
            let rep = u16::from_le_bytes([head[10], head[11]]);
            let transition_mode = head[4];
            if Self::validate_transition_mode(self.stm_segment, segment, rep, transition_mode) {
                return INVALID_TRANSITION_MODE;
            }
            let freq_div = u16::from_le_bytes([head[8], head[9]]);
            if self.validate_silencer(
                freq_div,
                self.segments[self.mod_segment as usize].mod_freq_div,
            ) {
                return INVALID_SILENCER_SETTINGS;
            }
            let num_foci = head[5];
            let sound_speed = u16::from_le_bytes([head[6], head[7]]);
            let transition_value = u64::from_le_bytes([
                head[16], head[17], head[18], head[19], head[20], head[21], head[22], head[23],
            ]);
            if transition_mode != TRANSITION_MODE_NONE {
                self.stm_segment = segment;
            }
            self.stm_write_cursor = 0;
            self.stm_transition_mode = transition_mode;
            self.stm_transition_value = transition_value;
            self.num_foci = num_foci;
            let state = &mut self.segments[segment as usize];
            state.freq_div = freq_div;
            state.rep = rep;
            state.sound_speed = sound_speed;
            state.foci.clear();
            state.emissions.clear();
            24usize
        } else {
            4usize
        };

        let words = send_num * usize::from(self.num_foci);
        let Some(buffer) = body(data, offset, words * 8) else {
            return NOT_SUPPORTED_TAG;
        };
        for &chunk in buffer.as_chunks::<8>().0 {
            self.segments[segment as usize]
                .foci
                .push(u64::from_le_bytes(chunk));
        }
        self.stm_write_cursor += u32::try_from(words).unwrap_or(u32::MAX);

        if flag & FOCI_STM_FLAG_END != 0 {
            let cycle = self
                .stm_write_cursor
                .checked_div(u32::from(self.num_foci))
                .unwrap_or(0);
            let state = &mut self.segments[segment as usize];
            state.kind = StmKind::Foci;
            state.cycle = cycle;
            if flag & FOCI_STM_FLAG_TRANSITION != 0 {
                return self
                    .stm_segment_update(self.stm_transition_mode, self.stm_transition_value);
            }
        }
        NO_ERROR
    }

    fn write_gain_stm(&mut self, data: &[u8]) -> u8 {
        let Some(head) = prefix(data, 2) else {
            return NOT_SUPPORTED_TAG;
        };
        let flag = head[1];
        let segment = u8::from(flag & GAIN_STM_FLAG_SEGMENT != 0);
        let send = (flag >> 6) + 1;

        let offset = if flag & GAIN_STM_FLAG_BEGIN != 0 {
            let Some(head) = prefix(data, 16) else {
                return NOT_SUPPORTED_TAG;
            };
            let rep = u16::from_le_bytes([head[6], head[7]]);
            let transition_mode = head[3];
            if Self::validate_transition_mode(self.stm_segment, segment, rep, transition_mode) {
                return INVALID_TRANSITION_MODE;
            }
            let freq_div = u16::from_le_bytes([head[4], head[5]]);
            if self.validate_silencer(
                freq_div,
                self.segments[self.mod_segment as usize].mod_freq_div,
            ) {
                return INVALID_SILENCER_SETTINGS;
            }
            let mode = head[2];
            let transition_value = u64::from_le_bytes([
                head[8], head[9], head[10], head[11], head[12], head[13], head[14], head[15],
            ]);
            if transition_mode != TRANSITION_MODE_NONE {
                self.stm_segment = segment;
            }
            self.gain_stm_mode = mode;
            self.stm_transition_mode = transition_mode;
            self.stm_transition_value = transition_value;
            let state = &mut self.segments[segment as usize];
            state.cycle = 0;
            state.freq_div = freq_div;
            state.rep = rep;
            state.emissions.clear();
            state.foci.clear();
            16usize
        } else {
            2usize
        };

        let shifts: &[u32] = match self.gain_stm_mode {
            GAIN_STM_MODE_PHASE_INTENSITY_FULL => &[0],
            GAIN_STM_MODE_PHASE_FULL => {
                if send > 1 {
                    &[0, 8]
                } else {
                    &[0]
                }
            }
            GAIN_STM_MODE_PHASE_HALF => match send {
                1 => &[0],
                2 => &[0, 4],
                3 => &[0, 4, 8],
                _ => &[0, 4, 8, 12],
            },
            _ => return INVALID_GAIN_STM_MODE,
        };

        let Some(words) = body(data, offset, self.num_transducers * 2) else {
            return NOT_SUPPORTED_TAG;
        };
        let mode = self.gain_stm_mode;
        for &shift in shifts {
            let emissions = words
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&chunk| gain_stm_emission(chunk, mode, shift))
                .collect::<Vec<_>>();
            let state = &mut self.segments[segment as usize];
            state.emissions.push(emissions);
            state.cycle += 1;
        }

        if flag & GAIN_STM_FLAG_END != 0 {
            self.segments[segment as usize].kind = StmKind::Gain;
            if flag & GAIN_STM_FLAG_TRANSITION != 0 {
                return self
                    .stm_segment_update(self.stm_transition_mode, self.stm_transition_value);
            }
        }
        NO_ERROR
    }

    fn change_stm_segment(&mut self, data: &[u8], gain: bool) -> u8 {
        let Some(head) = prefix(data, 16) else {
            return NOT_SUPPORTED_TAG;
        };
        let segment = self.take_segment(head[1]);
        let mode = head[2];
        let value = u64::from_le_bytes([
            head[8], head[9], head[10], head[11], head[12], head[13], head[14], head[15],
        ]);
        let state = &self.segments[segment as usize];
        let kind_ok = if gain {
            state.kind == StmKind::Gain && state.cycle != 1
        } else {
            state.kind == StmKind::Foci
        };
        if !kind_ok {
            return INVALID_SEGMENT_TRANSITION;
        }
        if Self::validate_transition_mode(self.stm_segment, segment, state.rep, mode) {
            return INVALID_TRANSITION_MODE;
        }
        if self.validate_silencer(
            self.segments[segment as usize].freq_div,
            self.segments[self.mod_segment as usize].mod_freq_div,
        ) {
            return INVALID_SILENCER_SETTINGS;
        }
        self.stm_segment = segment;
        self.stm_segment_update(mode, value)
    }

    fn write_output_mask(&mut self, data: &[u8]) -> u8 {
        let Some(head) = prefix(data, 2) else {
            return NOT_SUPPORTED_TAG;
        };
        let segment = usize::from(self.take_segment(head[1]));
        let Some(bytes) = body(data, 2, self.num_transducers.div_ceil(8)) else {
            return NOT_SUPPORTED_TAG;
        };
        self.output_mask[segment] = (0..self.num_transducers)
            .map(|i| bytes[i / 8] & (1 << (i % 8)) != 0)
            .collect();
        NO_ERROR
    }

    fn write_phase_correction(&mut self, data: &[u8]) -> u8 {
        let Some(bytes) = body(data, 2, self.num_transducers) else {
            return NOT_SUPPORTED_TAG;
        };
        self.phase_correction = bytes.iter().map(|&v| Phase(v)).collect();
        NO_ERROR
    }

    fn write_pulse_width_table(&mut self, data: &[u8]) -> u8 {
        let Some(bytes) = body(data, 2, PWE_TABLE_SIZE * 2) else {
            return NOT_SUPPORTED_TAG;
        };
        self.pulse_width_table = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .collect();
        NO_ERROR
    }

    fn write_gpio_out(&mut self, data: &[u8]) -> u8 {
        let Some(bytes) = body(data, 8, 32) else {
            return NOT_SUPPORTED_TAG;
        };
        for (dst, &chunk) in self.gpio_out.iter_mut().zip(bytes.as_chunks::<8>().0) {
            *dst = u64::from_le_bytes(chunk);
        }
        NO_ERROR
    }

    fn write_gpio_in(&mut self, data: &[u8]) -> u8 {
        let Some(&flag) = data.get(1) else {
            return NOT_SUPPORTED_TAG;
        };
        for (i, dst) in self.gpio_in_pending.iter_mut().enumerate() {
            *dst = flag & (1 << i) != 0;
        }
        NO_ERROR
    }

    fn stm_segment_update(&mut self, mode: u8, value: u64) -> u8 {
        if mode == TRANSITION_MODE_SYS_TIME
            && value < self.dc_sys_time_ns + SYS_TIME_TRANSITION_MARGIN_NS
        {
            return MISS_TRANSITION_TIME;
        }
        self.stm_transition_mode = mode;
        self.stm_transition_value = value;
        NO_ERROR
    }
}
