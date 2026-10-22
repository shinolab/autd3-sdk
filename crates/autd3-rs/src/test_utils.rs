use crate::commands::Command;
use crate::commands::operation::{Distribution, Operation, set_cmd};
use crate::datagram::{Datagram, Frames};
use crate::error::{Error, PayloadError};
use crate::geometry::{Autd3, Device, Geometry};
use crate::protocol::{Cmd, PAYLOAD_BYTES};

pub(crate) fn test_geometry(num_devices: usize) -> Geometry {
    Geometry::new((0..num_devices).map(|_| Autd3::default()))
}

pub(crate) fn test_device(idx: usize) -> Device {
    test_geometry(idx + 1)[idx].clone()
}

pub(crate) fn encode(op: &impl Operation) -> Result<((Cmd, usize), [u8; PAYLOAD_BYTES]), Error> {
    let mut out = Datagram::no_payload(Cmd::Nop);
    op.encode(&test_device(0), &mut out)?;
    Ok(((out.cmd, out.payload_len), out.payload))
}

pub(crate) fn build(num_devices: usize, cmd: impl Command) -> Result<Frames, Error> {
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

    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        out.payload[0] = self.0;
        set_cmd(out, Cmd::ConfigModulation, 1);
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FailBroadcast;

impl crate::sealed::Sealed for FailBroadcast {}

impl Operation for FailBroadcast {
    fn encode(&self, _device: &Device, _out: &mut Datagram) -> Result<(), Error> {
        Err(PayloadError::ModulationDataEmpty.into())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FailAt(pub(crate) usize);

impl crate::sealed::Sealed for FailAt {}

impl Operation for FailAt {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut Datagram) -> Result<(), Error> {
        if device.idx() == self.0 {
            return Err(PayloadError::ModulationDataEmpty.into());
        }
        out.payload[0] = 0xFF;
        set_cmd(out, Cmd::ConfigModulation, 1);
        Ok(())
    }
}
