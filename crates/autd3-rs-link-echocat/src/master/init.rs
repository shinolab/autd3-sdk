use std::time::{Duration, Instant};

use super::Master;
use crate::bus::RawBus;
use crate::error::EchocatError;
use crate::reg;
use crate::wire::{Address, Command};

pub const AUTD3_VENDOR_ID: u32 = 0x0000_08a9;
pub const AUTD3_PRODUCT_CODE: u32 = 0x0000_0001;

pub const SM_MAILBOX_OUT_START: u16 = 0x1000;
pub const SM_MAILBOX_IN_START: u16 = 0x1400;
pub const SM_MAILBOX_BYTES: u16 = 128;
pub const SM_MAILBOX_OUT_CONTROL: u8 = 0x26;
pub const SM_MAILBOX_IN_CONTROL: u8 = 0x22;

pub const SM_OUTPUTS_START: u16 = 0x1800;
pub const SM_INPUTS_START: u16 = 0x1f80;
pub const SM_OUTPUTS_CONTROL: u8 = 0x64;
pub const SM_INPUTS_CONTROL: u8 = 0x20;

pub const OUTPUT_BYTES: u16 = 626;
pub const INPUT_BYTES: u16 = 2;

pub const OUTPUT_LOGICAL_BASE: u32 = 0x0000_0000;
pub const INPUT_LOGICAL_BASE: u32 = 0x1000_0000;

const FMMU_TYPE_INPUTS: u8 = 0x01;
const FMMU_TYPE_OUTPUTS: u8 = 0x02;
const FMMU_OUTPUTS: u16 = 0;
const FMMU_INPUTS: u16 = 1;
const FMMU_COUNT: u16 = 3;
const SM_COUNT: u16 = 4;

const WATCHDOG_DIVIDER_100US: u16 = 0x09c2;
const WATCHDOG_TICK: Duration = Duration::from_micros(100);

const SII_READ_COMMAND: u16 = 0x0100;
const SII_BUSY: u16 = 0x8000;
const SII_ERROR_MASK: u16 = 0x7800;
const SII_TIMEOUT: Duration = Duration::from_millis(500);

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
    entry[6] = 0;
    entry[7] = 7;
    entry[8..10].copy_from_slice(&physical.to_le_bytes());
    entry[10] = 0;
    entry[11] = kind;
    entry[12] = 0x01;
    entry
}

#[must_use]
pub fn output_logical_address(index: usize) -> u32 {
    OUTPUT_LOGICAL_BASE
        + u32::try_from(index).expect("device index fits in u32") * u32::from(OUTPUT_BYTES)
}

#[must_use]
pub fn input_logical_address(index: usize) -> u32 {
    INPUT_LOGICAL_BASE
        + u32::try_from(index).expect("device index fits in u32") * u32::from(INPUT_BYTES)
}

impl<B: RawBus> Master<B> {
    pub fn enumerate(&mut self) -> Result<usize, EchocatError> {
        let mut probe = [0u8; 1];
        let wkc = self.read_bytes(Command::Brd, Address::broadcast(reg::TYPE), &mut probe)?;
        if wkc == 0 {
            return Err(EchocatError::NoSubDevices(
                "the selected interface".to_owned(),
            ));
        }
        self.devices = usize::from(wkc);

        self.write_bytes(
            Command::Bwr,
            Address::broadcast(reg::fmmu(0)),
            &vec![0u8; usize::from(FMMU_COUNT) * 16],
        )?;
        self.write_bytes(
            Command::Bwr,
            Address::broadcast(reg::sync_manager(0)),
            &vec![0u8; usize::from(SM_COUNT) * 8],
        )?;
        self.write_u8(Command::Bwr, Address::broadcast(reg::DL_CONTROL_LOOP), 0)?;
        self.write_u8(
            Command::Bwr,
            Address::broadcast(reg::EEPROM_CONFIGURATION),
            0,
        )?;

        for index in 0..self.devices {
            let position = u16::try_from(index).expect("device index fits in u16");
            let wkc = self.write_u16(
                Command::Apwr,
                Address::position(position, reg::DL_CONTROL),
                if index == 0 {
                    reg::DL_CONTROL_DESTROY_NON_ETHERCAT
                } else {
                    0
                },
            )?;
            Self::expect_wkc(wkc, 1)?;
            let wkc = self.write_u16(
                Command::Apwr,
                Address::position(position, reg::STATION_ADDRESS),
                Self::station_address(index),
            )?;
            Self::expect_wkc(wkc, 1)?;
        }

        let mut probe = [0u8; 1];
        let wkc = self.read_bytes(Command::Brd, Address::broadcast(reg::TYPE), &mut probe)?;
        if usize::from(wkc) != self.devices {
            return Err(EchocatError::SubDeviceCountChanged {
                expected: self.devices,
                received: usize::from(wkc),
            });
        }
        Ok(self.devices)
    }

    pub fn verify_identity(&mut self) -> Result<(), EchocatError> {
        for index in 0..self.devices {
            let vendor = self.sii_read_u32(index, reg::SII_WORD_VENDOR_ID)?;
            let product = self.sii_read_u32(index, reg::SII_WORD_PRODUCT_CODE)?;
            if vendor != AUTD3_VENDOR_ID || product != AUTD3_PRODUCT_CODE {
                return Err(EchocatError::ForeignSubDevice {
                    index,
                    vendor,
                    product,
                });
            }
        }
        Ok(())
    }

    fn sii_read_u32(&mut self, index: usize, word: u16) -> Result<u32, EchocatError> {
        let node = Self::station_address(index);
        self.sii_wait_idle(index, word)?;
        self.write_u32(
            Command::Fpwr,
            Address::node(node, reg::SII_ADDRESS),
            u32::from(word),
        )?;
        self.write_u16(
            Command::Fpwr,
            Address::node(node, reg::SII_CONTROL),
            SII_READ_COMMAND,
        )?;
        let status = self.sii_wait_idle(index, word)?;
        if status & SII_ERROR_MASK != 0 {
            return Err(EchocatError::SiiTimeout { index, word });
        }
        let (value, _) = self.read_u32(Command::Fprd, Address::node(node, reg::SII_DATA))?;
        Ok(value)
    }

    fn sii_wait_idle(&mut self, index: usize, word: u16) -> Result<u16, EchocatError> {
        let node = Self::station_address(index);
        let deadline = Instant::now() + SII_TIMEOUT;
        loop {
            let (status, _) =
                self.read_u16(Command::Fprd, Address::node(node, reg::SII_CONTROL))?;
            if status & SII_BUSY == 0 {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(EchocatError::SiiTimeout { index, word });
            }
        }
    }

    pub fn configure_mailbox_sync_managers(&mut self) -> Result<(), EchocatError> {
        let expected = u16::try_from(self.devices).expect("device count fits in u16");
        let wkc = self.write_bytes(
            Command::Bwr,
            Address::broadcast(reg::sync_manager(0)),
            &sync_manager_entry(
                SM_MAILBOX_OUT_START,
                SM_MAILBOX_BYTES,
                SM_MAILBOX_OUT_CONTROL,
            ),
        )?;
        Self::expect_wkc(wkc, expected)?;
        let wkc = self.write_bytes(
            Command::Bwr,
            Address::broadcast(reg::sync_manager(1)),
            &sync_manager_entry(SM_MAILBOX_IN_START, SM_MAILBOX_BYTES, SM_MAILBOX_IN_CONTROL),
        )?;
        Self::expect_wkc(wkc, expected)?;
        Ok(())
    }

    pub fn configure_process_data(&mut self) -> Result<(), EchocatError> {
        let expected = u16::try_from(self.devices).expect("device count fits in u16");
        let wkc = self.write_bytes(
            Command::Bwr,
            Address::broadcast(reg::sync_manager(2)),
            &sync_manager_entry(SM_OUTPUTS_START, OUTPUT_BYTES, SM_OUTPUTS_CONTROL),
        )?;
        Self::expect_wkc(wkc, expected)?;
        let wkc = self.write_bytes(
            Command::Bwr,
            Address::broadcast(reg::sync_manager(3)),
            &sync_manager_entry(SM_INPUTS_START, INPUT_BYTES, SM_INPUTS_CONTROL),
        )?;
        Self::expect_wkc(wkc, expected)?;

        self.write_u16(
            Command::Bwr,
            Address::broadcast(reg::WATCHDOG_DIVIDER),
            WATCHDOG_DIVIDER_100US,
        )?;
        let ticks = u16::try_from(
            self.config
                .process_data_watchdog
                .as_nanos()
                .div_ceil(WATCHDOG_TICK.as_nanos()),
        )
        .unwrap_or(u16::MAX);
        self.write_u16(
            Command::Bwr,
            Address::broadcast(reg::WATCHDOG_TIME_PROCESS_DATA),
            ticks,
        )?;

        for index in 0..self.devices {
            let node = Self::station_address(index);
            let wkc = self.write_bytes(
                Command::Fpwr,
                Address::node(node, reg::fmmu(FMMU_OUTPUTS)),
                &fmmu_entry(
                    output_logical_address(index),
                    OUTPUT_BYTES,
                    SM_OUTPUTS_START,
                    FMMU_TYPE_OUTPUTS,
                ),
            )?;
            Self::expect_wkc(wkc, 1)?;
            let wkc = self.write_bytes(
                Command::Fpwr,
                Address::node(node, reg::fmmu(FMMU_INPUTS)),
                &fmmu_entry(
                    input_logical_address(index),
                    INPUT_BYTES,
                    SM_INPUTS_START,
                    FMMU_TYPE_INPUTS,
                ),
            )?;
            Self::expect_wkc(wkc, 1)?;
        }
        Ok(())
    }
}
