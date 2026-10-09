#![allow(clippy::cast_possible_truncation)]

use std::sync::Arc;
use std::time::Duration;

use autd3_rs::commands::Command;
use autd3_rs::error::Error;
use autd3_rs::{Frame, Frames};
use autd3_rs_core::common::ULTRASOUND_PERIOD;
use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode, FRAME_BYTES_MAX, FrameHeader, Seq};
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
    geometry: Arc<Geometry>,
    devices: Vec<Device>,
    records: Vec<Vec<RawTransducerRecord>>,
    seq: Seq,
    start_ns: u64,
    current_ns: u64,
}

impl Recorder {
    #[must_use]
    pub fn new(geometry: &Geometry, start_ns: u64) -> Self {
        let devices: Vec<Device> = geometry
            .iter()
            .map(|d| Device::new(d.positions().len()))
            .collect();
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
            geometry: Arc::new(geometry.clone()),
            devices,
            records,
            seq: Seq::ZERO,
            start_ns,
            current_ns: start_ns,
        }
    }

    #[must_use]
    pub fn into_record(self) -> Record {
        let records = self
            .records
            .into_iter()
            .zip(self.geometry.iter())
            .flat_map(|(dev, device)| {
                dev.into_iter()
                    .zip(device.positions())
                    .map(|(tr, position)| TransducerRecord {
                        pulse_width: tr.pulse_width,
                        phase: tr.phase,
                        position: *position,
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
        let end = self.current_ns + tick.as_nanos() as u64;
        for t in (self.current_ns..end).step_by(period as usize) {
            for (dev, recs) in self.devices.iter_mut().zip(&mut self.records) {
                dev.fpga_mut().update_with_sys_time(t);
                let fpga = dev.fpga();
                let m = fpga.modulation();
                let (phases, intensities) = fpga.emissions();
                for (rec, (phase, intensity)) in
                    recs.iter_mut().zip(phases.iter().zip(&intensities))
                {
                    let intensity_mod = ((u16::from(intensity.0) * u16::from(m)) / 255) as u8;
                    let silenced_int = rec.silencer_intensity.apply(intensity_mod);
                    let pw = fpga.pulse_width_table(silenced_int as usize);
                    let ph = rec.silencer_phase.apply(phase.0);
                    rec.pulse_width.push(pw);
                    rec.phase.push(ph);
                    rec.last_intensity = silenced_int;
                    rec.last_phase = ph;
                }
            }
        }
        self.current_ns = end;
        Ok(())
    }

    fn stage_and_send(&mut self, frame: &Frame<'_>) -> Result<(), EmulatorError> {
        let seq = self.seq;
        self.seq = seq.next();
        let touches_silencer = frame
            .datagrams()
            .iter()
            .any(|d| matches!(d.cmd, Cmd::SetSilencer | Cmd::Clear));
        let mut rejected = None;
        for (device, dev) in self.devices.iter_mut().enumerate() {
            let dg = frame.datagram_for(device);
            let payload = dg.payload();
            let len = size_of::<FrameHeader>() + payload.len();
            let mut buf = [0u8; FRAME_BYTES_MAX];
            let (header, body) = buf.split_at_mut(size_of::<FrameHeader>());
            header.copy_from_slice(&[seq.get(), dg.cmd.as_u8()]);
            body[..payload.len()].copy_from_slice(payload);
            let status = dev.send(&buf[..len]).status;
            if status != DeviceErrorCode::None && rejected.is_none() {
                rejected = Some(Error::DeviceError {
                    device,
                    code: status.as_u8(),
                });
            }
        }
        if touches_silencer {
            for (dev, recs) in self.devices.iter().zip(&mut self.records) {
                for rec in recs {
                    rec.silencer_phase = dev.fpga().silencer_emulator_phase(rec.last_phase);
                    rec.silencer_intensity =
                        dev.fpga().silencer_emulator_intensity(rec.last_intensity);
                }
            }
        }
        rejected.map_or(Ok(()), |e| Err(e.into()))
    }
}

impl ClientApi for Recorder {
    type Error = EmulatorError;

    fn send<'a, C: Command<'a>>(
        &mut self,
        cmd: C,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        let sent = Frames::encode(&self.geometry, cmd)
            .map_err(EmulatorError::from)
            .and_then(|frames| {
                frames
                    .iter()
                    .try_for_each(|frame| self.stage_and_send(&frame))
            });
        std::future::ready(sent)
    }

    fn send_frame(&mut self, frame: Frame<'_>) -> impl Future<Output = Result<(), Self::Error>> {
        std::future::ready(self.stage_and_send(&frame))
    }
}
