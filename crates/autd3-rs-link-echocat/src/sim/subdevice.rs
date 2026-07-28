use super::ProcessData;
use super::sii::{Identity, SiiImage};
use crate::master::dc::DC_RECEIVE_TIME_PROCESSING_UNIT;
use crate::reg;
use crate::wire::Command;

pub const MEM_BYTES: usize = 0x2000;
pub const REGISTER_BYTES: usize = 0x1000;

const AL_STATUS_CODE_INVALID_MAILBOX: u16 = 0x0016;
const AL_STATUS_CODE_INVALID_SM_OUT: u16 = 0x001d;
const AL_STATUS_CODE_INVALID_SM_IN: u16 = 0x001e;
const AL_STATUS_CODE_SYNC_ERROR: u16 = 0x001a;

const SM_CONTROL: u16 = 0x04;
const SM_ACTIVATE: u16 = 0x06;
const SM_ENABLE: u8 = 0x01;

const FMMU_TYPE_READ: u8 = 0x01;
const FMMU_TYPE_WRITE: u8 = 0x02;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmConfig {
    pub start: u16,
    pub length: u16,
    pub control: u8,
    pub enabled: bool,
}

pub struct SubDevice {
    mem: Vec<u8>,
    sii: SiiImage,
    position: u16,
    clock_offset_ns: i64,
    process: Box<dyn ProcessData>,
    outputs: Vec<u8>,
    inputs: Vec<u8>,
    outputs_written: bool,
    outputs_written_in_safe_op: bool,
    port1_linked: bool,
    sync0_count: u64,
}

impl SubDevice {
    #[must_use]
    pub fn new(
        position: u16,
        identity: Identity,
        clock_offset_ns: i64,
        process: Box<dyn ProcessData>,
    ) -> Self {
        let mut mem = vec![0u8; MEM_BYTES];
        mem[0x0000] = 0x11;
        mem[0x0004] = 3;
        mem[0x0005] = 4;
        mem[0x0006] = 8;
        mem[0x0007] = 0x0f;
        mem[0x0008..0x000a].copy_from_slice(&0x000cu16.to_le_bytes());
        mem[usize::from(reg::AL_STATUS)] = reg::AlState::Init.code();
        Self {
            mem,
            sii: SiiImage::autd3(identity),
            position,
            clock_offset_ns,
            process,
            outputs: Vec::new(),
            inputs: Vec::new(),
            outputs_written: false,
            outputs_written_in_safe_op: false,
            port1_linked: true,
            sync0_count: 0,
        }
    }

    #[must_use]
    pub fn station_address(&self) -> u16 {
        self.read_u16(reg::STATION_ADDRESS)
    }

    #[must_use]
    pub fn al_state(&self) -> Option<reg::AlState> {
        reg::AlState::from_code(self.mem[usize::from(reg::AL_STATUS)])
    }

    #[must_use]
    pub fn al_status_code(&self) -> u16 {
        self.read_u16(reg::AL_STATUS_CODE)
    }

    pub fn latch_al_error(&mut self, state: reg::AlState, code: u16) {
        self.mem[usize::from(reg::AL_STATUS)] = state.code() | reg::AlState::ERROR_FLAG;
        let at = usize::from(reg::AL_STATUS_CODE);
        self.mem[at..at + 2].copy_from_slice(&code.to_le_bytes());
    }

    #[must_use]
    pub fn sync0_count(&self) -> u64 {
        self.sync0_count
    }

    #[must_use]
    pub fn destroys_non_ethercat_frames(&self) -> bool {
        self.read_u16(reg::DL_CONTROL) & reg::DL_CONTROL_DESTROY_NON_ETHERCAT != 0
    }

    #[must_use]
    pub fn sync_manager(&self, index: u16) -> SmConfig {
        let base = reg::sync_manager(index);
        SmConfig {
            start: self.read_u16(base),
            length: self.read_u16(base + 2),
            control: self.mem[usize::from(base + SM_CONTROL)],
            enabled: self.mem[usize::from(base + SM_ACTIVATE)] & SM_ENABLE != 0,
        }
    }

    #[must_use]
    pub fn system_time_offset(&self) -> u64 {
        self.read_u64(reg::DC_SYSTEM_TIME_OFFSET)
    }

    #[must_use]
    pub fn system_time_delay(&self) -> u32 {
        self.read_u32(reg::DC_SYSTEM_TIME_DELAY)
    }

    #[must_use]
    pub fn sync0_cycle_time(&self) -> u32 {
        self.read_u32(reg::DC_SYNC0_CYCLE_TIME)
    }

    #[must_use]
    pub fn sync_start_time(&self) -> u64 {
        self.read_u64(reg::DC_SYNC_START_TIME)
    }

    #[must_use]
    pub fn local_time(&self, now_ns: u64) -> u64 {
        now_ns.wrapping_add_signed(self.clock_offset_ns)
    }

    #[must_use]
    pub fn system_time(&self, now_ns: u64) -> u64 {
        self.local_time(now_ns)
            .wrapping_add(self.system_time_offset())
    }

    fn read_u16(&self, at: u16) -> u16 {
        let at = usize::from(at);
        u16::from_le_bytes([self.mem[at], self.mem[at + 1]])
    }

    fn read_u32(&self, at: u16) -> u32 {
        let at = usize::from(at);
        u32::from_le_bytes(self.mem[at..at + 4].try_into().expect("4 bytes"))
    }

    fn read_u64(&self, at: u16) -> u64 {
        let at = usize::from(at);
        u64::from_le_bytes(self.mem[at..at + 8].try_into().expect("8 bytes"))
    }

    fn write_u32(&mut self, at: u16, value: u32) {
        let at = usize::from(at);
        self.mem[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u64(&mut self, at: u16, value: u64) {
        let at = usize::from(at);
        self.mem[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }

    pub(super) fn set_port1_link(&mut self, linked: bool) {
        let mut status = self.read_u16(reg::DL_STATUS) | reg::DL_STATUS_PORT0_LINK;
        if linked {
            status |= reg::DL_STATUS_PORT1_LINK;
        } else {
            status &= !reg::DL_STATUS_PORT1_LINK;
        }
        let at = usize::from(reg::DL_STATUS);
        self.mem[at..at + 2].copy_from_slice(&status.to_le_bytes());
        self.port1_linked = linked;
    }

    pub(super) fn latch_port_times(&mut self, port0_global: u64, port1_global: u64) {
        let local0 = self.local_time(port0_global);
        let port0 = u32::try_from(local0 & 0xffff_ffff).expect("masked");
        self.write_u32(reg::DC_RECEIVE_TIME_PORT0, port0);
        if self.port1_linked {
            let port1 = u32::try_from(self.local_time(port1_global) & 0xffff_ffff).expect("masked");
            self.write_u32(reg::DC_RECEIVE_TIME_PORT0 + 4, port1);
        }
        self.write_u64(DC_RECEIVE_TIME_PROCESSING_UNIT, local0);
    }

    pub(super) fn sync0(&mut self, now_ns: u64) {
        if self.al_state() != Some(reg::AlState::Op) || !self.outputs_written {
            return;
        }
        let _ = now_ns;
        self.sync0_count += 1;
        let sm3 = self.sync_manager(3);
        if self.inputs.len() != usize::from(sm3.length) {
            self.inputs = vec![0u8; usize::from(sm3.length)];
        }
        self.process.exchange(&self.outputs, &mut self.inputs);
        let at = usize::from(sm3.start);
        self.mem[at..at + self.inputs.len()].copy_from_slice(&self.inputs);
    }

    pub(super) fn handle(
        &mut self,
        command: Command,
        address: &mut [u8; 4],
        data: &mut [u8],
        wkc: &mut u16,
        now_ns: u64,
        is_reference: bool,
    ) {
        match command {
            Command::Nop => {}
            Command::Aprd | Command::Apwr | Command::Aprw | Command::Armw => {
                let adp = u16::from_le_bytes([address[0], address[1]]);
                let register = u16::from_le_bytes([address[2], address[3]]);
                if adp == 0 {
                    match command {
                        Command::Aprd | Command::Armw => {
                            self.register_read(register, data, wkc, now_ns);
                        }
                        Command::Apwr => self.register_write(register, data, wkc, now_ns),
                        _ => self.register_read_write(register, data, wkc, now_ns),
                    }
                } else if command == Command::Armw {
                    self.register_write(register, data, wkc, now_ns);
                }
                address[..2].copy_from_slice(&adp.wrapping_add(1).to_le_bytes());
            }
            Command::Fprd | Command::Fpwr | Command::Fprw | Command::Frmw => {
                let node = u16::from_le_bytes([address[0], address[1]]);
                let register = u16::from_le_bytes([address[2], address[3]]);
                if node == self.station_address() {
                    match command {
                        Command::Fprd | Command::Frmw => {
                            self.register_read(register, data, wkc, now_ns);
                        }
                        Command::Fpwr => self.register_write(register, data, wkc, now_ns),
                        _ => self.register_read_write(register, data, wkc, now_ns),
                    }
                } else if command == Command::Frmw {
                    self.register_write(register, data, wkc, now_ns);
                }
            }
            Command::Brd | Command::Bwr | Command::Brw => {
                let register = u16::from_le_bytes([address[2], address[3]]);
                match command {
                    Command::Brd => {
                        let mut scratch = vec![0u8; data.len()];
                        let mut local = 0;
                        self.register_read(register, &mut scratch, &mut local, now_ns);
                        if local > 0 {
                            for (dst, src) in data.iter_mut().zip(scratch) {
                                *dst |= src;
                            }
                            *wkc += 1;
                        }
                    }
                    Command::Bwr => self.register_write(register, data, wkc, now_ns),
                    _ => self.register_read_write(register, data, wkc, now_ns),
                }
            }
            Command::Lrd | Command::Lwr | Command::Lrw => {
                let logical = u32::from_le_bytes(*address);
                self.logical(command, logical, data, wkc);
            }
        }
        let _ = is_reference;
    }

    fn register_read(&mut self, register: u16, data: &mut [u8], wkc: &mut u16, now_ns: u64) {
        let at = usize::from(register);
        if at + data.len() > REGISTER_BYTES {
            return;
        }
        if register == reg::DC_SYSTEM_TIME && data.len() >= 8 {
            let system_time = self.system_time(now_ns);
            self.write_u64(reg::DC_SYSTEM_TIME, system_time);
        }
        data.copy_from_slice(&self.mem[at..at + data.len()]);
        *wkc += 1;
    }

    fn register_write(&mut self, register: u16, data: &[u8], wkc: &mut u16, now_ns: u64) {
        let at = usize::from(register);
        if at + data.len() > REGISTER_BYTES {
            return;
        }
        if register == reg::DC_RECEIVE_TIME_PORT0 {
            *wkc += 1;
            return;
        }
        self.mem[at..at + data.len()].copy_from_slice(data);
        *wkc += 1;
        self.on_register_written(register, data, now_ns);
    }

    fn register_read_write(&mut self, register: u16, data: &mut [u8], wkc: &mut u16, now_ns: u64) {
        let at = usize::from(register);
        if at + data.len() > REGISTER_BYTES {
            return;
        }
        let written = data.to_vec();
        let mut previous = vec![0u8; data.len()];
        previous.copy_from_slice(&self.mem[at..at + data.len()]);
        self.mem[at..at + data.len()].copy_from_slice(&written);
        data.copy_from_slice(&previous);
        *wkc += 3;
        self.on_register_written(register, &written, now_ns);
    }

    fn on_register_written(&mut self, register: u16, data: &[u8], now_ns: u64) {
        if register == reg::AL_CONTROL {
            let requested = self.read_u16(reg::AL_CONTROL);
            self.apply_al_control(requested);
        }
        if register == reg::SII_CONTROL {
            self.apply_sii_command();
        }
        if register == reg::DC_SYSTEM_TIME && data.len() >= 4 {
            let reference = if data.len() >= 8 {
                u64::from_le_bytes(data[..8].try_into().expect("8 bytes"))
            } else {
                u64::from(u32::from_le_bytes(data[..4].try_into().expect("4 bytes")))
            };
            let own = self.system_time(now_ns);
            let difference = reference
                .wrapping_add(u64::from(self.system_time_delay()))
                .wrapping_sub(own)
                .cast_signed();
            let magnitude = u32::try_from(difference.unsigned_abs().min(u64::from(u32::MAX >> 1)))
                .expect("masked to 31 bits");
            let encoded = if difference < 0 {
                magnitude
            } else {
                magnitude | 0x8000_0000
            };
            self.write_u32(reg::DC_SYSTEM_TIME_DIFFERENCE, encoded);
        }
    }

    fn apply_sii_command(&mut self) {
        let control = self.read_u16(reg::SII_CONTROL);
        if control & 0x0100 == 0 {
            return;
        }
        let word = u16::try_from(self.read_u32(reg::SII_ADDRESS) & 0xffff).expect("masked");
        let value = self.sii.read(word, 4);
        let at = usize::from(reg::SII_DATA);
        self.mem[at..at + 4].copy_from_slice(&value);
        let cleared = control & !0x8100;
        self.mem[usize::from(reg::SII_CONTROL)..usize::from(reg::SII_CONTROL) + 2]
            .copy_from_slice(&cleared.to_le_bytes());
    }

    fn apply_al_control(&mut self, control: u16) {
        let requested = u8::try_from(control & 0x00ff).expect("masked") & reg::AlState::STATE_MASK;
        let Some(target) = reg::AlState::from_code(requested) else {
            return;
        };
        let acknowledged = control & u16::from(reg::AlState::ERROR_FLAG) != 0;
        let latched = self.mem[usize::from(reg::AL_STATUS)] & reg::AlState::ERROR_FLAG != 0;
        if latched && !acknowledged {
            return;
        }
        if let Some(code) = self.rejects(target) {
            let current = self.mem[usize::from(reg::AL_STATUS)] & reg::AlState::STATE_MASK;
            self.mem[usize::from(reg::AL_STATUS)] = current | reg::AlState::ERROR_FLAG;
            let at = usize::from(reg::AL_STATUS_CODE);
            self.mem[at..at + 2].copy_from_slice(&code.to_le_bytes());
            return;
        }
        if target == reg::AlState::SafeOp {
            self.outputs_written_in_safe_op = false;
        }
        self.mem[usize::from(reg::AL_STATUS)] = target.code();
        let at = usize::from(reg::AL_STATUS_CODE);
        self.mem[at..at + 2].copy_from_slice(&0u16.to_le_bytes());
    }

    fn rejects(&self, target: reg::AlState) -> Option<u16> {
        match target {
            reg::AlState::PreOp => {
                let mailbox_out = self.sync_manager(0);
                let mailbox_in = self.sync_manager(1);
                (!mailbox_out.enabled
                    || mailbox_out.length == 0
                    || !mailbox_in.enabled
                    || mailbox_in.length == 0)
                    .then_some(AL_STATUS_CODE_INVALID_MAILBOX)
            }
            reg::AlState::SafeOp => {
                let outputs = self.sync_manager(2);
                let inputs = self.sync_manager(3);
                if !outputs.enabled || outputs.length == 0 {
                    return Some(AL_STATUS_CODE_INVALID_SM_OUT);
                }
                if !inputs.enabled || inputs.length == 0 {
                    return Some(AL_STATUS_CODE_INVALID_SM_IN);
                }
                None
            }
            reg::AlState::Op => {
                let activation = self.mem[usize::from(reg::DC_SYNC_ACTIVATION)];
                let cycle = self.sync0_cycle_time();
                if activation & reg::DC_SYNC_ACTIVATION_CYCLIC == 0
                    || activation & reg::DC_SYNC_ACTIVATION_SYNC0 == 0
                    || cycle == 0
                {
                    return Some(AL_STATUS_CODE_SYNC_ERROR);
                }
                if !self.sync_start_time().is_multiple_of(u64::from(cycle)) {
                    return Some(AL_STATUS_CODE_SYNC_ERROR);
                }
                if !self.outputs_written_in_safe_op {
                    return Some(AL_STATUS_CODE_SYNC_ERROR);
                }
                None
            }
            _ => None,
        }
    }

    fn logical(&mut self, command: Command, logical: u32, data: &mut [u8], wkc: &mut u16) {
        let mut did_read = false;
        let mut did_write = false;
        for index in 0..3u16 {
            let base = reg::fmmu(index);
            let activate = self.mem[usize::from(base) + 0x0c];
            if activate & 0x01 == 0 {
                continue;
            }
            let start = self.read_u32(base);
            let length = u32::from(self.read_u16(base + 4));
            let physical = self.read_u16(base + 8);
            let kind = self.mem[usize::from(base) + 0x0b];

            let overlap_start = start.max(logical);
            let overlap_end =
                (start + length).min(logical + u32::try_from(data.len()).expect("fits"));
            if overlap_end <= overlap_start {
                continue;
            }
            let in_frame = usize::try_from(overlap_start - logical).expect("fits");
            let in_device =
                usize::from(physical) + usize::try_from(overlap_start - start).expect("fits");
            let count = usize::try_from(overlap_end - overlap_start).expect("fits");

            let reads =
                matches!(command, Command::Lrd | Command::Lrw) && kind & FMMU_TYPE_READ != 0;
            let writes =
                matches!(command, Command::Lwr | Command::Lrw) && kind & FMMU_TYPE_WRITE != 0;
            if reads {
                data[in_frame..in_frame + count]
                    .copy_from_slice(&self.mem[in_device..in_device + count]);
                did_read = true;
            }
            if writes {
                self.mem[in_device..in_device + count]
                    .copy_from_slice(&data[in_frame..in_frame + count]);
                did_write = true;
                self.latch_outputs();
            }
        }
        if did_read {
            *wkc += 1;
        }
        if did_write {
            *wkc += if command == Command::Lrw { 2 } else { 1 };
        }
    }

    fn latch_outputs(&mut self) {
        let sm2 = self.sync_manager(2);
        let at = usize::from(sm2.start);
        let len = usize::from(sm2.length);
        if at + len > MEM_BYTES {
            return;
        }
        self.outputs.clear();
        self.outputs.extend_from_slice(&self.mem[at..at + len]);
        self.outputs_written = true;
        self.outputs_written_in_safe_op = true;
    }

    #[must_use]
    pub fn position(&self) -> u16 {
        self.position
    }
}
