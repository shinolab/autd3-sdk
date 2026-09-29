use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::frame::{Command, DATAGRAM_HEADER_BYTES, FRAME_HEADER_BYTES, MORE_DATAGRAMS, WKC_BYTES};
use super::master::{AUTD3_PRODUCT_CODE, AUTD3_VENDOR_ID};
use super::raw::RawBus;
use super::reg::{self, AlState};

const MEM_BYTES: usize = 0x2000;
const REGISTER_BYTES: usize = 0x1000;
const CYCLE_NS: u64 = 1_000_000;
const HOP_NS: u64 = 300;
const SII_BYTES: usize = 2048;

const AL_STATUS_CODE_INVALID_MAILBOX: u16 = 0x0016;
const AL_STATUS_CODE_INVALID_SM_OUT: u16 = 0x001d;
const AL_STATUS_CODE_INVALID_SM_IN: u16 = 0x001e;
const AL_STATUS_CODE_SYNC_ERROR: u16 = 0x001a;

pub trait ProcessData: Send {
    fn exchange(&mut self, outputs: &[u8], inputs: &mut [u8]);
}

pub struct NopProcessData;

impl ProcessData for NopProcessData {
    fn exchange(&mut self, _outputs: &[u8], _inputs: &mut [u8]) {}
}

struct SmConfig {
    start: u16,
    length: u16,
    enabled: bool,
}

struct SubDevice {
    mem: Vec<u8>,
    sii: [u8; SII_BYTES],
    clock_offset_ns: i64,
    process: Box<dyn ProcessData>,
    outputs: Vec<u8>,
    inputs: Vec<u8>,
    outputs_written: bool,
    outputs_written_in_safe_op: bool,
    port1_linked: bool,
}

impl SubDevice {
    fn new(
        clock_offset_ns: i64,
        vendor: u32,
        product: u32,
        port1_linked: bool,
        process: Box<dyn ProcessData>,
    ) -> Self {
        let mut mem = vec![0u8; MEM_BYTES];
        mem[usize::from(reg::TYPE)] = 0x11;
        mem[usize::from(reg::AL_STATUS)] = AlState::Init.code();
        let mut status = reg::DL_STATUS_PORT0_LINK;
        if port1_linked {
            status |= reg::DL_STATUS_PORT1_LINK;
        }
        let at = usize::from(reg::DL_STATUS);
        mem[at..at + 2].copy_from_slice(&status.to_le_bytes());
        let mut sii = [0u8; SII_BYTES];
        let vendor_at = usize::from(reg::SII_WORD_VENDOR_ID) * 2;
        sii[vendor_at..vendor_at + 4].copy_from_slice(&vendor.to_le_bytes());
        let product_at = usize::from(reg::SII_WORD_PRODUCT_CODE) * 2;
        sii[product_at..product_at + 4].copy_from_slice(&product.to_le_bytes());
        Self {
            mem,
            sii,
            clock_offset_ns,
            process,
            outputs: Vec::new(),
            inputs: Vec::new(),
            outputs_written: false,
            outputs_written_in_safe_op: false,
            port1_linked,
        }
    }

    fn read_u16(&self, at: u16) -> u16 {
        let at = usize::from(at);
        u16::from_le_bytes([self.mem[at], self.mem[at + 1]])
    }

    fn read_u32(&self, at: u16) -> u32 {
        let at = usize::from(at);
        u32::from_le_bytes(self.mem[at..at + 4].try_into().unwrap())
    }

    fn read_u64(&self, at: u16) -> u64 {
        let at = usize::from(at);
        u64::from_le_bytes(self.mem[at..at + 8].try_into().unwrap())
    }

    fn write_bytes(&mut self, at: u16, data: &[u8]) {
        let at = usize::from(at);
        self.mem[at..at + data.len()].copy_from_slice(data);
    }

    fn al_state(&self) -> Option<AlState> {
        AlState::from_code(self.mem[usize::from(reg::AL_STATUS)])
    }

    fn sync_manager(&self, index: u16) -> SmConfig {
        let base = reg::sync_manager(index);
        SmConfig {
            start: self.read_u16(base),
            length: self.read_u16(base + 2),
            enabled: self.mem[usize::from(base + 6)] & 0x01 != 0,
        }
    }

    fn local_time(&self, now_ns: u64) -> u64 {
        now_ns.wrapping_add_signed(self.clock_offset_ns)
    }

    fn system_time(&self, now_ns: u64) -> u64 {
        self.local_time(now_ns)
            .wrapping_add(self.read_u64(reg::DC_SYSTEM_TIME_OFFSET))
    }

    fn latch_port_times(&mut self, port0_global: u64, port1_global: u64) {
        let local0 = self.local_time(port0_global);
        self.write_bytes(
            reg::DC_RECEIVE_TIME_PORT0,
            &(local0 & 0xffff_ffff).to_le_bytes()[..4],
        );
        if self.port1_linked {
            let local1 = self.local_time(port1_global);
            self.write_bytes(
                reg::DC_RECEIVE_TIME_PORT0 + 4,
                &(local1 & 0xffff_ffff).to_le_bytes()[..4],
            );
        }
        self.write_bytes(reg::DC_RECEIVE_TIME_PROCESSING_UNIT, &local0.to_le_bytes());
    }

    fn sync0(&mut self) {
        if self.al_state() != Some(AlState::Op) || !self.outputs_written {
            return;
        }
        let sm3 = self.sync_manager(3);
        self.inputs.resize(usize::from(sm3.length), 0);
        self.process.exchange(&self.outputs, &mut self.inputs);
        let inputs = self.inputs.clone();
        self.write_bytes(sm3.start, &inputs);
    }

    fn handle(
        &mut self,
        command: Command,
        address: &mut [u8; 4],
        data: &mut [u8],
        wkc: &mut u16,
        now_ns: u64,
    ) {
        let register = u16::from_le_bytes([address[2], address[3]]);
        match command {
            Command::Nop => {}
            Command::Apwr => {
                let adp = u16::from_le_bytes([address[0], address[1]]);
                if adp == 0 {
                    self.register_write(register, data, wkc, now_ns);
                }
                address[..2].copy_from_slice(&adp.wrapping_add(1).to_le_bytes());
            }
            Command::Fprd | Command::Fpwr | Command::Frmw => {
                let node = u16::from_le_bytes([address[0], address[1]]);
                let own = node == self.read_u16(reg::STATION_ADDRESS);
                match command {
                    Command::Fprd | Command::Frmw if own => {
                        self.register_read(register, data, wkc, now_ns);
                    }
                    Command::Fpwr if own => self.register_write(register, data, wkc, now_ns),
                    Command::Frmw => self.register_write(register, data, wkc, now_ns),
                    _ => {}
                }
            }
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
            Command::Lrd | Command::Lwr => {
                self.logical(command, u32::from_le_bytes(*address), data, wkc);
            }
        }
    }

    fn register_read(&mut self, register: u16, data: &mut [u8], wkc: &mut u16, now_ns: u64) {
        let at = usize::from(register);
        if at + data.len() > REGISTER_BYTES {
            return;
        }
        if register == reg::DC_SYSTEM_TIME && data.len() >= 8 {
            let system_time = self.system_time(now_ns);
            self.write_bytes(reg::DC_SYSTEM_TIME, &system_time.to_le_bytes());
        }
        data.copy_from_slice(&self.mem[at..at + data.len()]);
        *wkc += 1;
    }

    fn register_write(&mut self, register: u16, data: &[u8], wkc: &mut u16, now_ns: u64) {
        if usize::from(register) + data.len() > REGISTER_BYTES {
            return;
        }
        *wkc += 1;
        if register == reg::DC_RECEIVE_TIME_PORT0 {
            return;
        }
        self.write_bytes(register, data);
        match register {
            reg::AL_CONTROL => self.apply_al_control(self.read_u16(reg::AL_CONTROL)),
            reg::SII_CONTROL => self.apply_sii_command(),
            reg::DC_SYSTEM_TIME if data.len() >= 8 => {
                let reference = u64::from_le_bytes(data[..8].try_into().unwrap());
                let difference = reference
                    .wrapping_add(u64::from(self.read_u32(reg::DC_SYSTEM_TIME_DELAY)))
                    .wrapping_sub(self.system_time(now_ns))
                    .cast_signed();
                let magnitude =
                    u32::try_from(difference.unsigned_abs().min(u64::from(u32::MAX >> 1))).unwrap();
                let encoded = if difference < 0 {
                    magnitude
                } else {
                    magnitude | 0x8000_0000
                };
                self.write_bytes(reg::DC_SYSTEM_TIME_DIFFERENCE, &encoded.to_le_bytes());
            }
            _ => {}
        }
    }

    fn apply_sii_command(&mut self) {
        let control = self.read_u16(reg::SII_CONTROL);
        if control & 0x0100 == 0 {
            return;
        }
        let word = usize::try_from(self.read_u32(reg::SII_ADDRESS) & 0xffff).unwrap();
        let mut value = [0u8; 4];
        for (i, byte) in value.iter_mut().enumerate() {
            *byte = self.sii.get(word * 2 + i).copied().unwrap_or(0);
        }
        self.write_bytes(reg::SII_DATA, &value);
        self.write_bytes(reg::SII_CONTROL, &(control & !0x8100).to_le_bytes());
    }

    fn set_al_status(&mut self, status: u8, code: u16) {
        self.mem[usize::from(reg::AL_STATUS)] = status;
        self.write_bytes(reg::AL_STATUS_CODE, &code.to_le_bytes());
    }

    fn apply_al_control(&mut self, control: u16) {
        let requested = control.to_le_bytes()[0] & AlState::STATE_MASK;
        let Some(target) = AlState::from_code(requested) else {
            return;
        };
        let acknowledged = control & u16::from(AlState::ERROR_FLAG) != 0;
        let status = self.mem[usize::from(reg::AL_STATUS)];
        if status & AlState::ERROR_FLAG != 0 && !acknowledged {
            return;
        }
        if let Some(code) = self.rejects(target) {
            self.set_al_status((status & AlState::STATE_MASK) | AlState::ERROR_FLAG, code);
            return;
        }
        if target == AlState::SafeOp {
            self.outputs_written_in_safe_op = false;
        }
        self.set_al_status(target.code(), 0);
    }

    fn rejects(&self, target: AlState) -> Option<u16> {
        match target {
            AlState::PreOp => {
                let out = self.sync_manager(0);
                let inp = self.sync_manager(1);
                (!out.enabled || out.length == 0 || !inp.enabled || inp.length == 0)
                    .then_some(AL_STATUS_CODE_INVALID_MAILBOX)
            }
            AlState::SafeOp => {
                let outputs = self.sync_manager(2);
                let inputs = self.sync_manager(3);
                if !outputs.enabled || outputs.length == 0 {
                    return Some(AL_STATUS_CODE_INVALID_SM_OUT);
                }
                (!inputs.enabled || inputs.length == 0).then_some(AL_STATUS_CODE_INVALID_SM_IN)
            }
            AlState::Op => {
                let activation = self.mem[usize::from(reg::DC_SYNC_ACTIVATION)];
                let cycle = self.read_u32(reg::DC_SYNC0_CYCLE_TIME);
                let armed = activation & reg::DC_SYNC_ACTIVATION_CYCLIC != 0
                    && activation & reg::DC_SYNC_ACTIVATION_SYNC0 != 0
                    && cycle != 0
                    && self
                        .read_u64(reg::DC_SYNC_START_TIME)
                        .is_multiple_of(u64::from(cycle));
                (!armed || !self.outputs_written_in_safe_op).then_some(AL_STATUS_CODE_SYNC_ERROR)
            }
            _ => None,
        }
    }

    fn logical(&mut self, command: Command, logical: u32, data: &mut [u8], wkc: &mut u16) {
        let mut touched = false;
        for index in 0..3u16 {
            let base = reg::fmmu(index);
            if self.mem[usize::from(base) + 0x0c] & 0x01 == 0 {
                continue;
            }
            let start = self.read_u32(base);
            let length = u32::from(self.read_u16(base + 4));
            let physical = self.read_u16(base + 8);
            let kind = self.mem[usize::from(base) + 0x0b];
            let overlap_start = start.max(logical);
            let overlap_end = (start + length).min(logical + u32::try_from(data.len()).unwrap());
            if overlap_end <= overlap_start {
                continue;
            }
            let in_frame = usize::try_from(overlap_start - logical).unwrap();
            let in_device = usize::from(physical) + usize::try_from(overlap_start - start).unwrap();
            let count = usize::try_from(overlap_end - overlap_start).unwrap();
            if command == Command::Lrd && kind & 0x01 != 0 {
                data[in_frame..in_frame + count]
                    .copy_from_slice(&self.mem[in_device..in_device + count]);
                touched = true;
            }
            if command == Command::Lwr && kind & 0x02 != 0 {
                self.mem[in_device..in_device + count]
                    .copy_from_slice(&data[in_frame..in_frame + count]);
                touched = true;
                let sm2 = self.sync_manager(2);
                let at = usize::from(sm2.start);
                self.outputs = self.mem[at..at + usize::from(sm2.length)].to_vec();
                self.outputs_written = true;
                self.outputs_written_in_safe_op = true;
            }
        }
        if touched {
            *wkc += 1;
        }
    }
}

struct Inner {
    devices: Vec<SubDevice>,
    now_ns: u64,
    pending: VecDeque<Vec<u8>>,
}

#[derive(Clone)]
pub struct EscSim(Arc<Mutex<Inner>>);

impl EscSim {
    fn build(
        count: usize,
        vendor: u32,
        mut factory: impl FnMut(usize) -> Box<dyn ProcessData>,
    ) -> Self {
        let devices = (0..count)
            .map(|index| {
                SubDevice::new(
                    i64::try_from(index).unwrap() * 12_345,
                    vendor,
                    AUTD3_PRODUCT_CODE,
                    index + 1 != count,
                    factory(index),
                )
            })
            .collect();
        Self(Arc::new(Mutex::new(Inner {
            devices,
            now_ns: 1_000_000,
            pending: VecDeque::new(),
        })))
    }

    pub fn with_process_data(
        count: usize,
        factory: impl FnMut(usize) -> Box<dyn ProcessData>,
    ) -> Self {
        Self::build(count, AUTD3_VENDOR_ID, factory)
    }

    pub fn nop(count: usize) -> Self {
        Self::with_process_data(count, |_| Box::new(NopProcessData))
    }

    pub fn foreign(count: usize) -> Self {
        Self::build(count, 0x0000_0002, |_| Box::new(NopProcessData))
    }

    pub fn latch_al_error(&self, state: AlState, code: u16) {
        for device in &mut self.0.lock().unwrap().devices {
            device.set_al_status(state.code() | AlState::ERROR_FLAG, code);
        }
    }

    pub fn all_in(&self, state: AlState) -> bool {
        let inner = self.0.lock().unwrap();
        inner
            .devices
            .iter()
            .all(|device| device.mem[usize::from(reg::AL_STATUS)] == state.code())
    }
}

impl Inner {
    fn process(&mut self, frame: &mut [u8]) {
        let mut at = FRAME_HEADER_BYTES;
        let count = u64::try_from(self.devices.len()).unwrap();
        while at + DATAGRAM_HEADER_BYTES + WKC_BYTES <= frame.len() {
            let Some(command) = Command::from_code(frame[at]) else {
                break;
            };
            let register = u16::from_le_bytes([frame[at + 4], frame[at + 5]]);
            if command == Command::Frmw && register == reg::DC_SYSTEM_TIME {
                self.now_ns += CYCLE_NS;
                for device in &mut self.devices {
                    device.sync0();
                }
            }
            if command == Command::Bwr && register == reg::DC_RECEIVE_TIME_PORT0 {
                for (index, device) in self.devices.iter_mut().enumerate() {
                    let index = u64::try_from(index).unwrap();
                    device.latch_port_times(
                        self.now_ns + index * HOP_NS,
                        self.now_ns + (2 * (count - 1) - index) * HOP_NS,
                    );
                }
            }
            let length_field = u16::from_le_bytes([frame[at + 6], frame[at + 7]]);
            let len = usize::from(length_field & 0x07ff);
            let data_at = at + DATAGRAM_HEADER_BYTES;
            if data_at + len + WKC_BYTES > frame.len() {
                break;
            }
            let mut address = [frame[at + 2], frame[at + 3], frame[at + 4], frame[at + 5]];
            let mut wkc = 0u16;
            let now = self.now_ns;
            for (index, device) in self.devices.iter_mut().enumerate() {
                let arrival = now + u64::try_from(index).unwrap() * HOP_NS;
                device.handle(
                    command,
                    &mut address,
                    &mut frame[data_at..data_at + len],
                    &mut wkc,
                    arrival,
                );
            }
            frame[at + 2..at + 6].copy_from_slice(&address);
            frame[data_at + len..data_at + len + WKC_BYTES].copy_from_slice(&wkc.to_le_bytes());
            at = data_at + len + WKC_BYTES;
            if length_field & MORE_DATAGRAMS == 0 {
                break;
            }
        }
    }
}

impl RawBus for EscSim {
    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        let mut inner = self.0.lock().unwrap();
        let mut response = frame.to_vec();
        inner.process(&mut response);
        inner.pending.push_back(response);
        Ok(())
    }

    fn receive(&mut self, buf: &mut [u8], _timeout: Duration) -> io::Result<Option<usize>> {
        let Some(frame) = self.0.lock().unwrap().pending.pop_front() else {
            return Ok(None);
        };
        buf[..frame.len()].copy_from_slice(&frame);
        Ok(Some(frame.len()))
    }

    fn mtu(&self) -> usize {
        1500
    }
}
