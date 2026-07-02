#![allow(clippy::cast_possible_truncation)]

use std::time::Duration;

use autd3_rs::commands::Distribution;
use autd3_rs::{DatagramBuilder, Frame};
use autd3_rs_core::common::ULTRASOUND_PERIOD;
use autd3_rs_core::geometry::{Geometry, Point3};
use autd3_rs_core::protocol::{Cmd, Seq, TX_FRAME_BYTES};
use autd3_rs_firmware_emulator::{Device, SilencerEmulator};

use crate::client_api::ClientApi;
use crate::error::EmulatorError;
use crate::record::{Record, TransducerRecord};

struct RawTransducerRecord {
    pulse_width: Vec<u16>,
    phase: Vec<u8>,
    silencer_phase: SilencerEmulator,
    silencer_intensity: SilencerEmulator,
    last_phase: u8,
    last_intensity: u8,
}

pub struct Recorder {
    devices: Vec<Device>,
    records: Vec<Vec<RawTransducerRecord>>,
    positions: Vec<Vec<Point3<f32>>>,
    seq: Seq,
    start_ns: u64,
    current_ns: u64,
}

impl Recorder {
    pub(crate) fn open(geometry: &Geometry, start_ns: u64) -> Self {
        let devices: Vec<Device> = geometry
            .iter()
            .map(|d| Device::new(d.positions().len()))
            .collect();
        let positions: Vec<Vec<Point3<f32>>> =
            geometry.iter().map(|d| d.positions().to_vec()).collect();
        let records = devices
            .iter()
            .map(|dev| {
                (0..dev.fpga().num_transducers())
                    .map(|_| RawTransducerRecord {
                        pulse_width: Vec::new(),
                        phase: Vec::new(),
                        silencer_phase: dev.fpga().silencer_emulator_phase(0),
                        silencer_intensity: dev.fpga().silencer_emulator_intensity(0),
                        last_phase: 0,
                        last_intensity: 0,
                    })
                    .collect()
            })
            .collect();
        Self {
            devices,
            records,
            positions,
            seq: Seq::ZERO,
            start_ns,
            current_ns: start_ns,
        }
    }

    pub(crate) fn into_record(self) -> Record {
        let records = self
            .records
            .into_iter()
            .zip(self.positions)
            .flat_map(|(dev, dev_positions)| {
                dev.into_iter()
                    .zip(dev_positions)
                    .map(|(tr, position)| TransducerRecord {
                        pulse_width: tr.pulse_width,
                        phase: tr.phase,
                        position,
                    })
            })
            .collect();
        Record::new(records, self.start_ns, self.current_ns)
    }

    pub fn tick(&mut self, tick: Duration) -> Result<(), EmulatorError> {
        let period = ULTRASOUND_PERIOD.as_nanos();
        if tick.is_zero() || !tick.as_nanos().is_multiple_of(period) {
            return Err(EmulatorError::InvalidTick);
        }
        let period = period as u64;
        let mut t = self.current_ns;
        let end = t + tick.as_nanos() as u64;
        loop {
            for d in 0..self.devices.len() {
                self.devices[d].fpga_mut().update_with_sys_time(t);
                let m = self.devices[d].fpga().modulation();
                let drives = self.devices[d].fpga().drives();
                for (tr, drive) in drives.iter().enumerate() {
                    let rec = &mut self.records[d][tr];
                    let intensity_mod = ((u16::from(drive.intensity.0) * u16::from(m)) / 255) as u8;
                    let silenced_int = rec.silencer_intensity.apply(intensity_mod);
                    let pw = self.devices[d]
                        .fpga()
                        .pulse_width_table(silenced_int as usize);
                    let ph = rec.silencer_phase.apply(drive.phase.0);
                    rec.pulse_width.push(pw);
                    rec.phase.push(ph);
                    rec.last_intensity = silenced_int;
                    rec.last_phase = ph;
                }
            }
            t += period;
            if t >= end {
                break;
            }
        }
        self.current_ns = end;
        Ok(())
    }

    fn stage_and_send(&mut self, frame: &Frame<'_>) {
        let seq = self.seq;
        let touches_silencer = frame
            .datagrams()
            .iter()
            .any(|d| matches!(d.cmd, Cmd::SetSilencer | Cmd::Clear));
        for d in 0..self.devices.len() {
            let dg = match frame.distribution() {
                Distribution::Broadcast => &frame.datagrams()[0],
                Distribution::PerDevice => &frame.datagrams()[d],
            };
            let mut buf = [0u8; TX_FRAME_BYTES];
            buf[0] = seq.get();
            buf[1] = dg.cmd.as_u8();
            buf[2..].copy_from_slice(&dg.payload);
            self.devices[d].send(&buf);
        }
        self.seq = self.seq.next();
        if touches_silencer {
            for d in 0..self.devices.len() {
                for tr in 0..self.records[d].len() {
                    let last_phase = self.records[d][tr].last_phase;
                    let last_intensity = self.records[d][tr].last_intensity;
                    self.records[d][tr].silencer_phase =
                        self.devices[d].fpga().silencer_emulator_phase(last_phase);
                    self.records[d][tr].silencer_intensity = self.devices[d]
                        .fpga()
                        .silencer_emulator_intensity(last_intensity);
                }
            }
        }
    }
}

impl ClientApi for Recorder {
    type Error = EmulatorError;

    fn datagram_builder<'a>(&self) -> DatagramBuilder<'a> {
        DatagramBuilder::new(self.devices.len())
    }

    async fn send_checked(&mut self, frame: Frame<'_>) -> Result<(), Self::Error> {
        self.stage_and_send(&frame);
        Ok(())
    }
}
