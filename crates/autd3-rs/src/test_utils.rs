use crate::commands::Command;
use crate::commands::operation::{Distribution, Encoded, Operation};
use crate::datagram::{Expansion, Frames};
use crate::error::{Error, PayloadError};
use crate::geometry::{Autd3, Device, Geometry};
use crate::protocol::{Cmd, PAYLOAD_BYTES};

pub(crate) fn test_geometry(num_devices: usize) -> Geometry {
    Geometry::new((0..num_devices).map(|_| Autd3::default()).collect())
}

pub(crate) fn test_device(idx: usize) -> Device {
    test_geometry(idx + 1)[idx].clone()
}

pub(crate) fn encode(op: &impl Operation) -> Result<(Encoded, [u8; PAYLOAD_BYTES]), Error> {
    let mut out = [0u8; PAYLOAD_BYTES];
    let encoded = op.encode(&test_device(0), &mut out)?;
    Ok((encoded, out))
}

pub(crate) fn build<'a>(num_devices: usize, cmd: impl Command<'a>) -> Result<Frames, Error> {
    Frames::encode(&test_geometry(num_devices), cmd)
}

pub(crate) fn payload(frames: &Frames, frame: usize, device: usize) -> &[u8] {
    frames.frame(frame).unwrap().datagrams()[device].payload()
}

pub(crate) fn cmds(frames: &Frames) -> Vec<Cmd> {
    frames
        .iter()
        .map(|frame| frame.datagrams()[0].cmd)
        .collect()
}

pub(crate) fn cmd_at(frames: &Frames, frame: usize, device: usize) -> Cmd {
    frames.frame(frame).unwrap().datagrams()[device].cmd
}

#[derive(Clone, Copy)]
pub(crate) struct Marker(pub(crate) u8);

impl crate::sealed::Sealed for Marker {}

impl Operation for Marker {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        out[0] = self.0;
        Ok(Encoded::new(Cmd::ConfigModulation, 1))
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Multi(pub(crate) usize);

impl<'a> Command<'a> for Multi {
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
        for frame in 0..self.0 {
            expansion.push(Marker(u8::try_from(frame).unwrap()))?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FailAt(pub(crate) usize);

impl crate::sealed::Sealed for FailAt {}

impl Operation for FailAt {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        if device.idx() == self.0 {
            return Err(PayloadError::ModulationDataEmpty.into());
        }
        out[0] = 0xFF;
        Ok(Encoded::new(Cmd::ConfigModulation, 1))
    }
}
