use std::time::{Duration, Instant};

use autd3_rs::protocol::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs::value::DcSysTime;

use super::EcatError;
use super::frame::{
    Address, Command, DATAGRAM_OVERHEAD_BYTES, ECAT_HEADER_BYTES, ETH_HEADER_BYTES, FrameBuilder,
    FrameError, FrameView, MAX_DATAGRAM_DATA_BYTES, MIN_ETHERNET_FRAME_BYTES, Slot,
    frame_bytes_for, frame_index,
};
use super::raw::RawBus;
use super::reg::{self, AlState};

pub const FIRST_STATION_ADDRESS: u16 = 0x1000;
pub const AUTD3_VENDOR_ID: u32 = 0x0000_08a9;
pub const AUTD3_PRODUCT_CODE: u32 = 0x0000_0001;

const SM_MAILBOX_OUT_START: u16 = 0x1000;
const SM_MAILBOX_IN_START: u16 = 0x1400;
const SM_MAILBOX_BYTES: u16 = 128;
const SM_MAILBOX_OUT_CONTROL: u8 = 0x26;
const SM_MAILBOX_IN_CONTROL: u8 = 0x22;
const SM_OUTPUTS_START: u16 = 0x1800;
const SM_INPUTS_START: u16 = 0x1f80;
const SM_OUTPUTS_CONTROL: u8 = 0x64;
const SM_INPUTS_CONTROL: u8 = 0x20;

pub const OUTPUT_BYTES: u16 = 626;
pub const INPUT_BYTES: u16 = 2;
const _: () = assert!(OUTPUT_BYTES as usize == TX_FRAME_BYTES);
const _: () = assert!(INPUT_BYTES as usize == RX_FRAME_BYTES);

const OUTPUT_LOGICAL_BASE: u32 = 0x0000_0000;
const INPUT_LOGICAL_BASE: u32 = 0x1000_0000;
const FMMU_TYPE_INPUTS: u8 = 0x01;
const FMMU_TYPE_OUTPUTS: u8 = 0x02;
const FMMU_OUTPUTS: u16 = 0;
const FMMU_INPUTS: u16 = 1;
const FMMU_COUNT: usize = 3;
const SM_COUNT: usize = 4;

const WATCHDOG_DIVIDER_100US: u16 = 0x09c2;
const WATCHDOG_TICK: Duration = Duration::from_micros(100);

const SII_READ_COMMAND: u16 = 0x0100;
const SII_BUSY: u16 = 0x8000;
const SII_ERROR_MASK: u16 = 0x7800;
const SII_TIMEOUT: Duration = Duration::from_millis(500);

const MAX_PLAUSIBLE_DELAY: Duration = Duration::from_millis(1);
const OP_PRIMING_CYCLES: usize = 8;
const PHASE_CORRECTION_DIVISOR: i64 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MasterConfig {
    pub cycle: Duration,
    pub pdu_timeout: Duration,
    pub state_transition_timeout: Duration,
    pub dc_static_sync_iterations: u32,
    pub dc_start_delay: Duration,
    pub sync_tolerance: Duration,
    pub sync_timeout: Duration,
    pub process_data_watchdog: Duration,
}

const DEFAULT_CYCLE: Duration = Duration::from_millis(2);

impl Default for MasterConfig {
    fn default() -> Self {
        Self {
            cycle: DEFAULT_CYCLE,
            pdu_timeout: Duration::from_millis(100),
            state_transition_timeout: Duration::from_secs(10),
            dc_static_sync_iterations: 10_000,
            dc_start_delay: Duration::from_millis(100),
            sync_tolerance: Duration::from_micros(1),
            sync_timeout: Duration::from_secs(10),
            process_data_watchdog: Duration::from_millis(100),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    DcTime,
    AlStatus,
    DeviceAlStatus,
    AlControl,
    Inputs,
    Outputs { offset: usize },
}

#[derive(Clone, Copy, Debug)]
struct Planned {
    command: Command,
    address: Address,
    len: usize,
    expected_wkc: u16,
    role: Role,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Recovery {
    device: usize,
    control: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CycleReport {
    pub rx_valid: bool,
    pub dc_system_time: u64,
}

pub struct Master<B: RawBus> {
    bus: B,
    tx: Vec<u8>,
    rx: Vec<u8>,
    index: u8,
    config: MasterConfig,
    devices: usize,
    frames: Vec<Vec<Planned>>,
    buffers: Vec<Vec<u8>>,
    slots: Vec<Vec<Slot>>,
    indices: Vec<u8>,
    sent: Vec<usize>,
    received: Vec<bool>,
    echo_pending: Vec<bool>,
    next_at: Option<Instant>,
    phase_bias_ns: i64,
    op_entered: bool,
    rotation: usize,
    recovery: Option<Recovery>,
}

fn device_count(devices: usize) -> u16 {
    u16::try_from(devices).expect("device count fits in u16")
}

fn sync_manager_entry(start: u16, length: u16, control: u8) -> [u8; 8] {
    let mut entry = [0u8; 8];
    entry[..2].copy_from_slice(&start.to_le_bytes());
    entry[2..4].copy_from_slice(&length.to_le_bytes());
    entry[4] = control;
    entry[6] = 0x01;
    entry
}

fn fmmu_entry(logical: u32, length: u16, physical: u16, kind: u8) -> [u8; 16] {
    let mut entry = [0u8; 16];
    entry[..4].copy_from_slice(&logical.to_le_bytes());
    entry[4..6].copy_from_slice(&length.to_le_bytes());
    entry[7] = 7;
    entry[8..10].copy_from_slice(&physical.to_le_bytes());
    entry[11] = kind;
    entry[12] = 0x01;
    entry
}

fn logical_address(base: u32, index: usize, stride: u16) -> u32 {
    base + u32::try_from(index).expect("device index fits in u32") * u32::from(stride)
}

#[must_use]
pub fn next_cycle_wait(dc_system_time: u64, cycle: Duration, landing_ns: u64) -> Duration {
    let cycle_ns = u64::try_from(cycle.as_nanos()).expect("cycle fits in u64 nanoseconds");
    if cycle_ns == 0 {
        return Duration::ZERO;
    }
    let phase = dc_system_time % cycle_ns;
    Duration::from_nanos((cycle_ns - phase) + landing_ns % cycle_ns)
}

fn overlapping_devices(offset: usize, len: usize, devices: usize) -> u16 {
    let stride = usize::from(OUTPUT_BYTES);
    let count = (0..devices)
        .filter(|index| {
            let start = index * stride;
            start < offset + len && offset < start + stride
        })
        .count();
    device_count(count)
}

impl<B: RawBus> Master<B> {
    pub fn new(bus: B, config: MasterConfig) -> Self {
        let mtu = bus.mtu();
        Self {
            bus,
            tx: vec![0; mtu + ETH_HEADER_BYTES],
            rx: vec![0; mtu + ETH_HEADER_BYTES],
            index: 0,
            config,
            devices: 0,
            frames: Vec::new(),
            buffers: Vec::new(),
            slots: Vec::new(),
            indices: Vec::new(),
            sent: Vec::new(),
            received: Vec::new(),
            echo_pending: Vec::new(),
            next_at: None,
            phase_bias_ns: 0,
            op_entered: false,
            rotation: 0,
            recovery: None,
        }
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.devices
    }

    #[must_use]
    pub fn station_address(index: usize) -> u16 {
        FIRST_STATION_ADDRESS + u16::try_from(index).expect("device index fits in u16")
    }

    fn cycle_ns(&self) -> u64 {
        u64::try_from(self.config.cycle.as_nanos()).unwrap_or(u64::MAX)
    }

    fn landing_ns(&self) -> u64 {
        self.cycle_ns() / 2
    }

    fn transact(
        &mut self,
        command: Command,
        address: Address,
        data: &mut [u8],
    ) -> Result<u16, EcatError> {
        self.index = self.index.wrapping_add(1);
        let index = self.index;
        let need = frame_bytes_for(data.len());
        if self.tx.len() < need {
            self.tx.resize(need, 0);
        }
        let mut builder = FrameBuilder::new(&mut self.tx[..need], index);
        let slot = builder.push(command, address, data.len())?;
        builder.data_mut(slot).copy_from_slice(data);
        let sent = builder.finish();
        self.bus.send(&self.tx[..sent])?;

        let deadline = Instant::now() + self.config.pdu_timeout;
        let mut echo_pending = true;
        let len = loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(EcatError::Timeout(self.config.pdu_timeout));
            }
            let Some(len) = self.bus.receive(&mut self.rx, deadline - now)? else {
                continue;
            };
            if echo_pending && self.rx[..len] == self.tx[..sent] {
                echo_pending = false;
                continue;
            }
            match FrameView::parse(&self.rx[..len], index) {
                Ok(_) => break len,
                Err(FrameError::IndexMismatch { .. }) => {}
                Err(e) => return Err(e.into()),
            }
        };
        let view = FrameView::parse(&self.rx[..len], index)?;
        data.copy_from_slice(view.data(slot)?);
        Ok(view.wkc(slot)?)
    }

    fn expect_wkc(wkc: u16, expected: u16) -> Result<(), EcatError> {
        if wkc == expected {
            Ok(())
        } else {
            Err(EcatError::WorkingCounter {
                expected,
                received: wkc,
            })
        }
    }

    fn read<const N: usize>(
        &mut self,
        command: Command,
        address: Address,
    ) -> Result<([u8; N], u16), EcatError> {
        let mut data = [0u8; N];
        let wkc = self.transact(command, address, &mut data)?;
        Ok((data, wkc))
    }

    fn read_u16(&mut self, command: Command, address: Address) -> Result<(u16, u16), EcatError> {
        let (data, wkc) = self.read::<2>(command, address)?;
        Ok((u16::from_le_bytes(data), wkc))
    }

    fn read_u32(&mut self, command: Command, address: Address) -> Result<(u32, u16), EcatError> {
        let (data, wkc) = self.read::<4>(command, address)?;
        Ok((u32::from_le_bytes(data), wkc))
    }

    fn read_u64(&mut self, command: Command, address: Address) -> Result<(u64, u16), EcatError> {
        let (data, wkc) = self.read::<8>(command, address)?;
        Ok((u64::from_le_bytes(data), wkc))
    }

    fn write(&mut self, command: Command, address: Address, data: &[u8]) -> Result<u16, EcatError> {
        let mut scratch = data.to_vec();
        self.transact(command, address, &mut scratch)
    }

    fn write_all(&mut self, register: u16, data: &[u8]) -> Result<(), EcatError> {
        let wkc = self.write(Command::Bwr, Address::broadcast(register), data)?;
        Self::expect_wkc(wkc, device_count(self.devices))
    }

    fn write_node(&mut self, index: usize, register: u16, data: &[u8]) -> Result<(), EcatError> {
        let wkc = self.write(
            Command::Fpwr,
            Address::node(Self::station_address(index), register),
            data,
        )?;
        Self::expect_wkc(wkc, 1)
    }

    pub fn probe(&mut self) -> Result<usize, EcatError> {
        let (_, wkc) = self.read::<1>(Command::Brd, Address::broadcast(reg::TYPE))?;
        Ok(usize::from(wkc))
    }

    fn enumerate(&mut self) -> Result<usize, EcatError> {
        self.devices = self.probe()?;
        if self.devices == 0 {
            return Err(EcatError::NoSubDevices);
        }

        self.write(
            Command::Bwr,
            Address::broadcast(reg::fmmu(0)),
            &[0u8; FMMU_COUNT * 16],
        )?;
        self.write(
            Command::Bwr,
            Address::broadcast(reg::sync_manager(0)),
            &[0u8; SM_COUNT * 8],
        )?;
        self.write(Command::Bwr, Address::broadcast(reg::DL_CONTROL_LOOP), &[0])?;
        self.write(
            Command::Bwr,
            Address::broadcast(reg::EEPROM_CONFIGURATION),
            &[0],
        )?;

        for index in 0..self.devices {
            let position = u16::try_from(index).expect("device index fits in u16");
            let dl_control = if index == 0 {
                reg::DL_CONTROL_DESTROY_NON_ETHERCAT
            } else {
                0
            };
            let wkc = self.write(
                Command::Apwr,
                Address::position(position, reg::DL_CONTROL),
                &dl_control.to_le_bytes(),
            )?;
            Self::expect_wkc(wkc, 1)?;
            let wkc = self.write(
                Command::Apwr,
                Address::position(position, reg::STATION_ADDRESS),
                &Self::station_address(index).to_le_bytes(),
            )?;
            Self::expect_wkc(wkc, 1)?;
        }

        let found = self.probe()?;
        if found != self.devices {
            return Err(EcatError::SubDeviceCountChanged {
                expected: self.devices,
                received: found,
            });
        }
        Ok(self.devices)
    }

    pub fn bring_up(&mut self) -> Result<(), EcatError> {
        self.enumerate()?;
        self.acknowledge_errors_and_request_init()?;
        self.verify_identity()?;
        self.write_all(
            reg::sync_manager(0),
            &sync_manager_entry(
                SM_MAILBOX_OUT_START,
                SM_MAILBOX_BYTES,
                SM_MAILBOX_OUT_CONTROL,
            ),
        )?;
        self.write_all(
            reg::sync_manager(1),
            &sync_manager_entry(SM_MAILBOX_IN_START, SM_MAILBOX_BYTES, SM_MAILBOX_IN_CONTROL),
        )?;
        self.request_state(AlState::PreOp)?;
        self.configure_process_data()?;
        self.init_dc()?;
        self.plan_cycle();
        self.request_state(AlState::SafeOp)
    }

    fn verify_identity(&mut self) -> Result<(), EcatError> {
        for index in 0..self.devices {
            let vendor = self.sii_read_u32(index, reg::SII_WORD_VENDOR_ID)?;
            let product = self.sii_read_u32(index, reg::SII_WORD_PRODUCT_CODE)?;
            if vendor != AUTD3_VENDOR_ID || product != AUTD3_PRODUCT_CODE {
                return Err(EcatError::ForeignSubDevice {
                    index,
                    vendor,
                    product,
                });
            }
        }
        Ok(())
    }

    fn sii_read_u32(&mut self, index: usize, word: u16) -> Result<u32, EcatError> {
        let node = Self::station_address(index);
        self.sii_wait_idle(index, word)?;
        self.write(
            Command::Fpwr,
            Address::node(node, reg::SII_ADDRESS),
            &u32::from(word).to_le_bytes(),
        )?;
        self.write(
            Command::Fpwr,
            Address::node(node, reg::SII_CONTROL),
            &SII_READ_COMMAND.to_le_bytes(),
        )?;
        if self.sii_wait_idle(index, word)? & SII_ERROR_MASK != 0 {
            return Err(EcatError::SiiTimeout { index, word });
        }
        Ok(self
            .read_u32(Command::Fprd, Address::node(node, reg::SII_DATA))?
            .0)
    }

    fn sii_wait_idle(&mut self, index: usize, word: u16) -> Result<u16, EcatError> {
        let node = Self::station_address(index);
        let deadline = Instant::now() + SII_TIMEOUT;
        loop {
            let (status, _) =
                self.read_u16(Command::Fprd, Address::node(node, reg::SII_CONTROL))?;
            if status & SII_BUSY == 0 {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(EcatError::SiiTimeout { index, word });
            }
        }
    }

    fn configure_process_data(&mut self) -> Result<(), EcatError> {
        self.write_all(
            reg::sync_manager(2),
            &sync_manager_entry(SM_OUTPUTS_START, OUTPUT_BYTES, SM_OUTPUTS_CONTROL),
        )?;
        self.write_all(
            reg::sync_manager(3),
            &sync_manager_entry(SM_INPUTS_START, INPUT_BYTES, SM_INPUTS_CONTROL),
        )?;
        self.write(
            Command::Bwr,
            Address::broadcast(reg::WATCHDOG_DIVIDER),
            &WATCHDOG_DIVIDER_100US.to_le_bytes(),
        )?;
        let ticks = u16::try_from(
            self.config
                .process_data_watchdog
                .as_nanos()
                .div_ceil(WATCHDOG_TICK.as_nanos()),
        )
        .unwrap_or(u16::MAX);
        self.write(
            Command::Bwr,
            Address::broadcast(reg::WATCHDOG_TIME_PROCESS_DATA),
            &ticks.to_le_bytes(),
        )?;

        for index in 0..self.devices {
            self.write_node(
                index,
                reg::fmmu(FMMU_OUTPUTS),
                &fmmu_entry(
                    logical_address(OUTPUT_LOGICAL_BASE, index, OUTPUT_BYTES),
                    OUTPUT_BYTES,
                    SM_OUTPUTS_START,
                    FMMU_TYPE_OUTPUTS,
                ),
            )?;
            self.write_node(
                index,
                reg::fmmu(FMMU_INPUTS),
                &fmmu_entry(
                    logical_address(INPUT_LOGICAL_BASE, index, INPUT_BYTES),
                    INPUT_BYTES,
                    SM_INPUTS_START,
                    FMMU_TYPE_INPUTS,
                ),
            )?;
        }
        Ok(())
    }

    fn init_dc(&mut self) -> Result<(), EcatError> {
        let (delays, host_time) = self.measure_propagation_delays()?;
        self.align_system_times(&delays, host_time)?;
        let expected = device_count(self.devices);
        let reference = Address::node(Self::station_address(0), reg::DC_SYSTEM_TIME);
        for _ in 0..self.config.dc_static_sync_iterations {
            let (_, wkc) = self.read::<8>(Command::Frmw, reference)?;
            Self::expect_wkc(wkc, expected)?;
        }
        self.wait_for_dc_sync()?;
        self.configure_sync0()
    }

    fn measure_propagation_delays(&mut self) -> Result<(Vec<u32>, u64), EcatError> {
        self.write_all(reg::DC_RECEIVE_TIME_PORT0, &0u32.to_le_bytes())?;
        let host_time = DcSysTime::now()?.sys_time();

        let mut round_trips = Vec::with_capacity(self.devices);
        for index in 0..self.devices {
            let node = Self::station_address(index);
            let (dl_status, wkc) =
                self.read_u16(Command::Fprd, Address::node(node, reg::DL_STATUS))?;
            Self::expect_wkc(wkc, 1)?;
            let (times, wkc) = self.read::<8>(
                Command::Fprd,
                Address::node(node, reg::DC_RECEIVE_TIME_PORT0),
            )?;
            Self::expect_wkc(wkc, 1)?;
            let port0 = u32::from_le_bytes([times[0], times[1], times[2], times[3]]);
            let port1 = u32::from_le_bytes([times[4], times[5], times[6], times[7]]);
            round_trips.push(if dl_status & reg::DL_STATUS_PORT1_LINK == 0 {
                0
            } else {
                port1.wrapping_sub(port0)
            });
        }

        let mut delays = vec![0u32; self.devices];
        for index in 1..self.devices {
            let hop = round_trips[index - 1].wrapping_sub(round_trips[index]) / 2;
            delays[index] = delays[index - 1].wrapping_add(hop);
        }
        if let Some((index, &delay_ns)) = delays
            .iter()
            .enumerate()
            .find(|(_, delay)| u128::from(**delay) > MAX_PLAUSIBLE_DELAY.as_nanos())
        {
            return Err(EcatError::ImplausiblePropagationDelay { index, delay_ns });
        }
        Ok((delays, host_time))
    }

    fn align_system_times(&mut self, delays: &[u32], host_time: u64) -> Result<(), EcatError> {
        let mut latched = Vec::with_capacity(self.devices);
        for index in 0..self.devices {
            let (time, wkc) = self.read_u64(
                Command::Fprd,
                Address::node(
                    Self::station_address(index),
                    reg::DC_RECEIVE_TIME_PROCESSING_UNIT,
                ),
            )?;
            Self::expect_wkc(wkc, 1)?;
            latched.push(time);
        }
        for (index, (&local, &delay)) in latched.iter().zip(delays).enumerate() {
            let offset = host_time.wrapping_add(u64::from(delay)).wrapping_sub(local);
            self.write_node(index, reg::DC_SYSTEM_TIME_OFFSET, &offset.to_le_bytes())?;
            self.write_node(index, reg::DC_SYSTEM_TIME_DELAY, &delay.to_le_bytes())?;
        }
        self.write_all(
            reg::DC_SPEED_COUNTER_START,
            &reg::SPEED_COUNTER_START_DEFAULT.to_le_bytes(),
        )
    }

    fn wait_for_dc_sync(&mut self) -> Result<(), EcatError> {
        let tolerance = u32::try_from(self.config.sync_tolerance.as_nanos()).unwrap_or(u32::MAX);
        let deadline = Instant::now() + self.config.sync_timeout;
        let reference = Address::node(Self::station_address(0), reg::DC_SYSTEM_TIME);
        loop {
            self.read::<8>(Command::Frmw, reference)?;
            let mut worst = 0u32;
            for index in 0..self.devices {
                let (difference, _) = self.read_u32(
                    Command::Fprd,
                    Address::node(Self::station_address(index), reg::DC_SYSTEM_TIME_DIFFERENCE),
                )?;
                worst = worst.max(difference & 0x7fff_ffff);
            }
            if worst <= tolerance {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(EcatError::DcSyncTimeout(self.config.sync_timeout));
            }
        }
    }

    fn configure_sync0(&mut self) -> Result<(), EcatError> {
        let cycle_ns = u32::try_from(self.config.cycle.as_nanos()).expect("cycle fits in u32 ns");
        self.write_all(reg::DC_SYNC_ACTIVATION, &[0])?;
        self.write_all(reg::DC_SYNC0_CYCLE_TIME, &cycle_ns.to_le_bytes())?;
        self.write_all(reg::DC_SYNC1_CYCLE_TIME, &0u32.to_le_bytes())?;
        let (now, wkc) = self.read_u64(
            Command::Fprd,
            Address::node(Self::station_address(0), reg::DC_SYSTEM_TIME),
        )?;
        Self::expect_wkc(wkc, 1)?;
        let start_delay = u64::try_from(self.config.dc_start_delay.as_nanos()).unwrap_or(u64::MAX);
        let cycle_ns = u64::from(cycle_ns);
        let start = (now.wrapping_add(start_delay) / cycle_ns + 1) * cycle_ns;
        self.write_all(reg::DC_SYNC_START_TIME, &start.to_le_bytes())?;
        self.write_all(
            reg::DC_SYNC_ACTIVATION,
            &[reg::DC_SYNC_ACTIVATION_CYCLIC | reg::DC_SYNC_ACTIVATION_SYNC0],
        )
    }

    fn al_states(&mut self) -> Result<Vec<(Option<AlState>, u16)>, EcatError> {
        let mut observed = Vec::with_capacity(self.devices);
        for index in 0..self.devices {
            let node = Self::station_address(index);
            let (raw, _) = self.read_u16(Command::Fprd, Address::node(node, reg::AL_STATUS))?;
            let raw = raw.to_le_bytes()[0];
            let code = if raw & AlState::ERROR_FLAG == 0 {
                0
            } else {
                self.read_u16(Command::Fprd, Address::node(node, reg::AL_STATUS_CODE))?
                    .0
            };
            observed.push((AlState::from_code(raw), code));
        }
        Ok(observed)
    }

    fn write_al_control(&mut self, control: u8) -> Result<(), EcatError> {
        self.write_all(reg::AL_CONTROL, &u16::from(control).to_le_bytes())
    }

    fn acknowledge_errors_and_request_init(&mut self) -> Result<(), EcatError> {
        let deadline = Instant::now() + self.config.state_transition_timeout;
        loop {
            self.write_al_control(AlState::Init.code() | AlState::ERROR_FLAG)?;
            if self
                .al_states()?
                .iter()
                .all(|&(status, code)| status == Some(AlState::Init) && code == 0)
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(EcatError::AlTimeout {
                    target: AlState::Init,
                    timeout: self.config.state_transition_timeout,
                });
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn request_state(&mut self, target: AlState) -> Result<(), EcatError> {
        self.write_al_control(target.code())?;
        let deadline = Instant::now() + self.config.state_transition_timeout;
        loop {
            match self.check_states(target)? {
                true => return Ok(()),
                false if Instant::now() >= deadline => {
                    return Err(EcatError::AlTimeout {
                        target,
                        timeout: self.config.state_transition_timeout,
                    });
                }
                false => std::thread::sleep(Duration::from_millis(1)),
            }
        }
    }

    fn check_states(&mut self, target: AlState) -> Result<bool, EcatError> {
        let states = self.al_states()?;
        if let Some((index, &(status, code))) =
            states.iter().enumerate().find(|(_, (_, code))| *code != 0)
        {
            return Err(EcatError::AlTransition {
                index,
                target,
                status,
                code,
            });
        }
        Ok(states.iter().all(|&(status, _)| status == Some(target)))
    }

    fn plan_cycle(&mut self) {
        let devices = self.devices;
        let expected = device_count(devices);
        let capacity = self.bus.mtu() - ECAT_HEADER_BYTES;
        let first = Address::node(Self::station_address(0), reg::AL_STATUS);
        let mut current = vec![
            Planned {
                command: Command::Frmw,
                address: Address::node(Self::station_address(0), reg::DC_SYSTEM_TIME),
                len: 8,
                expected_wkc: expected,
                role: Role::DcTime,
            },
            Planned {
                command: Command::Brd,
                address: Address::broadcast(reg::AL_STATUS),
                len: 2,
                expected_wkc: expected,
                role: Role::AlStatus,
            },
            Planned {
                command: Command::Fprd,
                address: first,
                len: reg::AL_STATUS_SPAN,
                expected_wkc: 1,
                role: Role::DeviceAlStatus,
            },
            Planned {
                command: Command::Nop,
                address: Address::node(Self::station_address(0), reg::AL_CONTROL),
                len: 2,
                expected_wkc: 0,
                role: Role::AlControl,
            },
            Planned {
                command: Command::Lrd,
                address: Address::Logical(INPUT_LOGICAL_BASE),
                len: devices * usize::from(INPUT_BYTES),
                expected_wkc: expected,
                role: Role::Inputs,
            },
        ];
        let mut used: usize = current
            .iter()
            .map(|d| DATAGRAM_OVERHEAD_BYTES + d.len)
            .sum();
        let mut frames = Vec::new();
        let total = devices * usize::from(OUTPUT_BYTES);
        let mut offset = 0;
        while offset < total {
            let available = capacity
                .saturating_sub(used)
                .saturating_sub(DATAGRAM_OVERHEAD_BYTES)
                .min(MAX_DATAGRAM_DATA_BYTES);
            if available == 0 {
                frames.push(std::mem::take(&mut current));
                used = 0;
                continue;
            }
            let len = available.min(total - offset);
            current.push(Planned {
                command: Command::Lwr,
                address: Address::Logical(
                    OUTPUT_LOGICAL_BASE + u32::try_from(offset).expect("offset fits in u32"),
                ),
                len,
                expected_wkc: overlapping_devices(offset, len, devices),
                role: Role::Outputs { offset },
            });
            used += DATAGRAM_OVERHEAD_BYTES + len;
            offset += len;
        }
        frames.push(current);

        self.buffers = frames
            .iter()
            .map(|datagrams| {
                let bytes: usize = datagrams
                    .iter()
                    .map(|d| DATAGRAM_OVERHEAD_BYTES + d.len)
                    .sum();
                vec![
                    0u8;
                    (bytes + ECAT_HEADER_BYTES + ETH_HEADER_BYTES).max(MIN_ETHERNET_FRAME_BYTES)
                ]
            })
            .collect();
        self.slots = frames.iter().map(|d| Vec::with_capacity(d.len())).collect();
        self.indices = vec![0; frames.len()];
        self.sent = vec![0; frames.len()];
        self.received = vec![false; frames.len()];
        self.echo_pending = vec![true; frames.len()];
        self.frames = frames;
    }

    pub fn wait_next_cycle(&mut self) {
        if let Some(deadline) = self.next_at {
            let now = Instant::now();
            if deadline > now {
                std::thread::sleep(deadline - now);
            }
        }
    }

    pub fn cycle(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<CycleReport, EcatError> {
        if !self.op_entered {
            self.reach_op(tx, rx)?;
            self.op_entered = true;
        }
        self.paced_cycle(tx, rx)
    }

    fn reach_op(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<(), EcatError> {
        for _ in 0..OP_PRIMING_CYCLES {
            self.paced_cycle(tx, rx)?;
        }
        self.write_al_control(AlState::Op.code())?;
        let deadline = Instant::now() + self.config.state_transition_timeout;
        loop {
            self.paced_cycle(tx, rx)?;
            if self.check_states(AlState::Op)? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(EcatError::AlTimeout {
                    target: AlState::Op,
                    timeout: self.config.state_transition_timeout,
                });
            }
        }
    }

    fn paced_cycle(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<CycleReport, EcatError> {
        self.wait_next_cycle();
        let anchor = Instant::now();
        let report = self.exchange(tx, rx)?;
        let landing = self.landing_ns();
        self.update_phase_bias(report.dc_system_time, landing);
        let wait = next_cycle_wait(report.dc_system_time, self.config.cycle, landing);
        let bias = Duration::from_nanos(self.phase_bias_ns.unsigned_abs());
        self.next_at = (report.dc_system_time != 0).then(|| anchor + wait.saturating_sub(bias));
        Ok(report)
    }

    fn update_phase_bias(&mut self, dc_system_time: u64, landing: u64) {
        let cycle_ns = self.cycle_ns();
        if cycle_ns == 0 || dc_system_time == 0 {
            return;
        }
        let (Ok(cycle), Ok(target), Ok(phase)) = (
            i64::try_from(cycle_ns),
            i64::try_from(landing),
            i64::try_from(dc_system_time % cycle_ns),
        ) else {
            return;
        };
        let error = (phase - target + cycle / 2).rem_euclid(cycle) - cycle / 2;
        self.phase_bias_ns =
            (self.phase_bias_ns + error / PHASE_CORRECTION_DIVISOR).clamp(0, target);
    }

    fn exchange(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<CycleReport, EcatError> {
        if self.frames.is_empty() {
            return Err(EcatError::Closed);
        }
        let observed = self.rotation;
        let recovery = self.recovery.take();
        self.send_cycle(tx, observed, recovery)?;
        let report = self.collect_cycle(rx, observed)?;
        self.rotation = (self.rotation + 1) % self.devices;
        Ok(report)
    }

    fn send_cycle(
        &mut self,
        tx: &[u8],
        observed: usize,
        recovery: Option<Recovery>,
    ) -> Result<(), EcatError> {
        for frame in 0..self.frames.len() {
            self.index = self.index.wrapping_add(1);
            self.indices[frame] = self.index;
            self.received[frame] = false;
            self.slots[frame].clear();

            let mut builder = FrameBuilder::new(&mut self.buffers[frame], self.index);
            for datagram in &self.frames[frame] {
                let (command, address) = match (datagram.role, recovery) {
                    (Role::DeviceAlStatus, _) => (
                        datagram.command,
                        Address::node(Self::station_address(observed), reg::AL_STATUS),
                    ),
                    (Role::AlControl, Some(recovery)) => (
                        Command::Fpwr,
                        Address::node(Self::station_address(recovery.device), reg::AL_CONTROL),
                    ),
                    _ => (datagram.command, datagram.address),
                };
                let slot = builder.push(command, address, datagram.len)?;
                match (datagram.role, recovery) {
                    (Role::Outputs { offset }, _) => builder
                        .data_mut(slot)
                        .copy_from_slice(&tx[offset..offset + datagram.len]),
                    (Role::AlControl, Some(recovery)) => builder
                        .data_mut(slot)
                        .copy_from_slice(&recovery.control.to_le_bytes()),
                    _ => {}
                }
                self.slots[frame].push(slot);
            }
            let len = builder.finish();
            self.sent[frame] = len;
            self.bus.send(&self.buffers[frame][..len])?;
        }
        Ok(())
    }

    fn collect_cycle(&mut self, rx: &mut [u8], observed: usize) -> Result<CycleReport, EcatError> {
        let mut rx_valid = true;
        let mut dc_system_time = 0u64;
        self.echo_pending.fill(true);
        let mut outstanding = self.frames.len();
        let deadline = Instant::now() + self.config.cycle.min(self.config.pdu_timeout);

        while outstanding > 0 {
            let now = Instant::now();
            if now >= deadline {
                rx_valid = false;
                break;
            }
            let Some(len) = self.bus.receive(&mut self.rx, deadline - now)? else {
                continue;
            };
            let Some(frame) = frame_index(&self.rx[..len])
                .and_then(|received| self.indices.iter().position(|&index| index == received))
            else {
                continue;
            };
            if self.received[frame] {
                continue;
            }
            if self.echo_pending[frame] && self.rx[..len] == self.buffers[frame][..self.sent[frame]]
            {
                self.echo_pending[frame] = false;
                continue;
            }
            let Ok(view) = FrameView::parse(&self.rx[..len], self.indices[frame]) else {
                continue;
            };
            self.received[frame] = true;
            outstanding -= 1;

            for (datagram, &slot) in self.frames[frame].iter().zip(&self.slots[frame]) {
                let wkc = view.wkc(slot)?;
                if wkc != datagram.expected_wkc && datagram.role != Role::AlControl {
                    rx_valid = false;
                }
                match datagram.role {
                    Role::DcTime => {
                        let data = view.data(slot)?;
                        dc_system_time = u64::from_le_bytes(data.try_into().expect("8 bytes"));
                    }
                    Role::DeviceAlStatus if self.op_entered && wkc == datagram.expected_wkc => {
                        let status = view.data(slot)?[0];
                        if status != AlState::Op.code() {
                            self.recovery = Some(Recovery {
                                device: observed,
                                control: if status & AlState::ERROR_FLAG != 0 {
                                    u16::from(status)
                                } else {
                                    u16::from(AlState::Op.code())
                                },
                            });
                        }
                    }
                    Role::Inputs => {
                        let data = view.data(slot)?;
                        rx[..data.len()].copy_from_slice(data);
                    }
                    _ => {}
                }
            }
        }
        if outstanding > 0 {
            rx_valid = false;
        }
        Ok(CycleReport {
            rx_valid,
            dc_system_time,
        })
    }

    pub fn close(&mut self) -> Result<(), EcatError> {
        self.write(
            Command::Bwr,
            Address::broadcast(reg::AL_CONTROL),
            &u16::from(AlState::Init.code()).to_le_bytes(),
        )?;
        self.next_at = None;
        self.frames.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_landing_phase_is_half_a_cycle_past_the_sync0_edge() {
        let cycle = Duration::from_millis(1);
        assert_eq!(
            next_cycle_wait(0, cycle, 500_000),
            Duration::from_micros(1500)
        );
        assert_eq!(next_cycle_wait(500_000, cycle, 500_000), cycle);
        assert_eq!(
            next_cycle_wait(999_999, cycle, 500_000),
            Duration::from_nanos(500_001)
        );
    }

    #[test]
    fn output_chunks_expect_a_working_counter_from_every_device_they_touch() {
        assert_eq!(overlapping_devices(0, 626, 4), 1);
        assert_eq!(overlapping_devices(0, 1252, 4), 2);
        assert_eq!(overlapping_devices(600, 100, 4), 2);
        assert_eq!(overlapping_devices(0, 626 * 4, 4), 4);
    }
}
