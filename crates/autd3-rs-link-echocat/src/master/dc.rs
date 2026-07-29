use std::time::{Duration, Instant};

use autd3_rs_core::value::DcSysTime;

use super::Master;
use crate::bus::RawBus;
use crate::error::EchocatError;
use crate::reg;
use crate::wire::{Address, Command};

pub const DC_RECEIVE_TIME_PROCESSING_UNIT: u16 = 0x0918;

const MAX_PLAUSIBLE_DELAY: Duration = Duration::from_millis(1);

#[derive(Clone, Debug)]
pub struct DcMeasurement {
    pub delays: Vec<u32>,
    pub host_time: u64,
}

impl<B: RawBus> Master<B> {
    pub fn init_dc(&mut self) -> Result<(), EchocatError> {
        let measurement = self.measure_propagation_delays()?;
        self.align_system_times(&measurement)?;
        self.compensate_static_drift()?;
        self.wait_for_dc_sync()?;
        self.configure_sync0()?;
        Ok(())
    }

    pub fn wait_for_dc_sync(&mut self) -> Result<(), EchocatError> {
        let tolerance = u32::try_from(self.config.sync_tolerance.as_nanos())
            .expect("tolerance fits in u32 nanoseconds");
        let deadline = Instant::now() + self.config.sync_timeout;
        let node = Self::station_address(0);
        loop {
            let mut time = [0u8; 8];
            self.read_bytes(
                Command::Frmw,
                Address::node(node, reg::DC_SYSTEM_TIME),
                &mut time,
            )?;

            let mut worst = 0u32;
            for index in 0..self.devices {
                let (difference, _) = self.read_u32(
                    Command::Fprd,
                    Address::node(Self::station_address(index), reg::DC_SYSTEM_TIME_DIFFERENCE),
                )?;
                worst = worst.max(difference & 0x7fff_ffff);
            }
            if worst <= tolerance {
                tracing::debug!(worst_ns = worst, "distributed clocks settled");
                return Ok(());
            }
            if Instant::now() >= deadline {
                tracing::warn!(
                    worst_ns = worst,
                    tolerance_ns = tolerance,
                    "distributed clocks did not settle",
                );
                return Err(EchocatError::DcSyncTimeout(self.config.sync_timeout));
            }
        }
    }

    pub fn measure_propagation_delays(&mut self) -> Result<DcMeasurement, EchocatError> {
        let expected = u16::try_from(self.devices).expect("device count fits in u16");
        let wkc = self.write_u32(
            Command::Bwr,
            Address::broadcast(reg::DC_RECEIVE_TIME_PORT0),
            0,
        )?;
        let host_time = DcSysTime::now().sys_time();
        Self::expect_wkc(wkc, expected)?;

        let mut round_trips = Vec::with_capacity(self.devices);
        for index in 0..self.devices {
            let node = Self::station_address(index);
            let (dl_status, wkc) =
                self.read_u16(Command::Fprd, Address::node(node, reg::DL_STATUS))?;
            Self::expect_wkc(wkc, 1)?;

            let mut times = [0u8; 8];
            let wkc = self.read_bytes(
                Command::Fprd,
                Address::node(node, reg::DC_RECEIVE_TIME_PORT0),
                &mut times,
            )?;
            Self::expect_wkc(wkc, 1)?;
            let port0 = u32::from_le_bytes(times[..4].try_into().expect("4 bytes"));
            let port1 = u32::from_le_bytes(times[4..].try_into().expect("4 bytes"));

            round_trips.push(if dl_status & reg::DL_STATUS_PORT1_LINK == 0 {
                0
            } else {
                port1.wrapping_sub(port0)
            });
        }

        let mut delays = Vec::with_capacity(self.devices);
        delays.push(0u32);
        for index in 1..self.devices {
            let previous = round_trips[index - 1];
            let current = round_trips[index];
            let hop = previous.wrapping_sub(current) / 2;
            delays.push(delays[index - 1].wrapping_add(hop));
        }
        tracing::debug!(
            ?delays,
            ?round_trips,
            "measured EtherCAT propagation delays"
        );

        if let Some((index, delay)) = delays
            .iter()
            .enumerate()
            .find(|(_, delay)| u128::from(**delay) > MAX_PLAUSIBLE_DELAY.as_nanos())
        {
            return Err(EchocatError::ImplausiblePropagationDelay {
                index,
                delay_ns: *delay,
            });
        }
        Ok(DcMeasurement { delays, host_time })
    }

    pub fn align_system_times(&mut self, measurement: &DcMeasurement) -> Result<(), EchocatError> {
        let mut latched = Vec::with_capacity(self.devices);
        for index in 0..self.devices {
            let node = Self::station_address(index);
            let (time, wkc) = self.read_u64(
                Command::Fprd,
                Address::node(node, DC_RECEIVE_TIME_PROCESSING_UNIT),
            )?;
            Self::expect_wkc(wkc, 1)?;
            latched.push(time);
        }
        let reference = measurement.host_time;

        for (index, local) in latched.iter().enumerate() {
            let node = Self::station_address(index);
            let delay = measurement.delays.get(index).copied().unwrap_or(0);
            let wkc = self.write_u64(
                Command::Fpwr,
                Address::node(node, reg::DC_SYSTEM_TIME_OFFSET),
                reference
                    .wrapping_add(u64::from(delay))
                    .wrapping_sub(*local),
            )?;
            Self::expect_wkc(wkc, 1)?;
            let wkc = self.write_u32(
                Command::Fpwr,
                Address::node(node, reg::DC_SYSTEM_TIME_DELAY),
                delay,
            )?;
            Self::expect_wkc(wkc, 1)?;
        }

        let expected = u16::try_from(self.devices).expect("device count fits in u16");
        let wkc = self.write_u16(
            Command::Bwr,
            Address::broadcast(reg::DC_SPEED_COUNTER_START),
            reg::SPEED_COUNTER_START_DEFAULT,
        )?;
        Self::expect_wkc(wkc, expected)?;
        Ok(())
    }

    pub fn compensate_static_drift(&mut self) -> Result<(), EchocatError> {
        let node = Self::station_address(0);
        let expected = u16::try_from(self.devices).expect("device count fits in u16");
        for _ in 0..self.config.dc_static_sync_iterations {
            let mut time = [0u8; 8];
            let wkc = self.read_bytes(
                Command::Frmw,
                Address::node(node, reg::DC_SYSTEM_TIME),
                &mut time,
            )?;
            Self::expect_wkc(wkc, expected)?;
        }
        Ok(())
    }

    pub fn configure_sync0(&mut self) -> Result<(), EchocatError> {
        let expected = u16::try_from(self.devices).expect("device count fits in u16");
        let cycle_ns =
            u32::try_from(self.config.cycle.as_nanos()).expect("cycle fits in u32 nanoseconds");

        let wkc = self.write_u8(Command::Bwr, Address::broadcast(reg::DC_SYNC_ACTIVATION), 0)?;
        Self::expect_wkc(wkc, expected)?;
        let wkc = self.write_u32(
            Command::Bwr,
            Address::broadcast(reg::DC_SYNC0_CYCLE_TIME),
            cycle_ns,
        )?;
        Self::expect_wkc(wkc, expected)?;
        let wkc = self.write_u32(
            Command::Bwr,
            Address::broadcast(reg::DC_SYNC1_CYCLE_TIME),
            0,
        )?;
        Self::expect_wkc(wkc, expected)?;

        let (now, wkc) = self.read_u64(
            Command::Fprd,
            Address::node(Self::station_address(0), reg::DC_SYSTEM_TIME),
        )?;
        Self::expect_wkc(wkc, 1)?;

        let start_delay =
            u64::try_from(self.config.dc_start_delay.as_nanos()).expect("start delay fits in u64");
        let cycle_ns = u64::from(cycle_ns);
        let start = (now.wrapping_add(start_delay) / cycle_ns + 1) * cycle_ns;
        let wkc = self.write_u64(
            Command::Bwr,
            Address::broadcast(reg::DC_SYNC_START_TIME),
            start,
        )?;
        Self::expect_wkc(wkc, expected)?;

        let wkc = self.write_u8(
            Command::Bwr,
            Address::broadcast(reg::DC_SYNC_ACTIVATION),
            reg::DC_SYNC_ACTIVATION_CYCLIC | reg::DC_SYNC_ACTIVATION_SYNC0,
        )?;
        Self::expect_wkc(wkc, expected)?;
        tracing::debug!(start, cycle_ns, "armed SYNC0");
        Ok(())
    }
}
