use std::rc::Rc;

use crate::commands::operation::{Distribution, Nop, Operation};
use crate::error::Error;
use crate::geometry::Device;
use crate::mirror::FirmwareState;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

pub(crate) type EachOps<'a> = Vec<Vec<Box<dyn Operation + 'a>>>;

pub(crate) fn each_frames(devices: &[Vec<Box<dyn Operation + '_>>]) -> usize {
    devices.iter().map(Vec::len).max().unwrap_or(0)
}

pub(crate) fn each_encode(
    devices: &[Vec<Box<dyn Operation + '_>>],
    device: &Device,
    frame: usize,
    out: &mut [u8; PAYLOAD_BYTES],
) -> Result<Cmd, Error> {
    match devices.get(device.idx()).and_then(|ops| ops.get(frame)) {
        Some(op) => op.encode(device, out),
        None => Nop.encode(device, out),
    }
}

pub(crate) fn each_reflect(
    devices: &[Vec<Box<dyn Operation + '_>>],
    device: usize,
    state: &mut FirmwareState,
) -> Result<(), Error> {
    if let Some(ops) = devices.get(device) {
        for op in ops {
            op.reflect(device, state)?;
        }
    }
    Ok(())
}

pub(crate) struct EachFrame<'a> {
    devices: Rc<EachOps<'a>>,
    frame: usize,
}

impl<'a> EachFrame<'a> {
    pub(crate) fn flatten(devices: EachOps<'a>) -> impl Iterator<Item = Self> {
        let frames = each_frames(&devices);
        let devices = Rc::new(devices);
        (0..frames).map(move |frame| Self {
            devices: Rc::clone(&devices),
            frame,
        })
    }
}

impl Operation for EachFrame<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        each_encode(&self.devices, device, self.frame, out)
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        match self.devices.get(device).and_then(|ops| ops.get(self.frame)) {
            Some(op) => op.reflect(device, state),
            None => Ok(()),
        }
    }
}
