use crate::commands::operation::{Distribution, Nop, Operation};
use crate::error::Error;
use crate::mirror::FirmwareState;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

pub(crate) fn each_slot_frames(devices: &[Vec<Box<dyn Operation + '_>>]) -> Vec<usize> {
    let num_slots = devices.iter().map(Vec::len).max().unwrap_or(0);
    let mut slot_frames = vec![0usize; num_slots];
    for ops in devices {
        for (slot, op) in ops.iter().enumerate() {
            slot_frames[slot] = slot_frames[slot].max(op.frames());
        }
    }
    slot_frames
}

fn each_locate(slot_frames: &[usize], frame: usize) -> Option<(usize, usize)> {
    let mut remaining = frame;
    for (slot, &frames) in slot_frames.iter().enumerate() {
        if remaining < frames {
            return Some((slot, remaining));
        }
        remaining -= frames;
    }
    None
}

pub(crate) fn each_encode(
    devices: &[Vec<Box<dyn Operation + '_>>],
    slot_frames: &[usize],
    device: usize,
    frame: usize,
    out: &mut [u8; PAYLOAD_BYTES],
) -> Result<Cmd, Error> {
    if let Some((slot, subframe)) = each_locate(slot_frames, frame) {
        if let Some(op) = devices.get(device).and_then(|ops| ops.get(slot))
            && subframe < op.frames()
        {
            return op.encode(device, subframe, out);
        }
        return Nop.encode(device, subframe, out);
    }
    Nop.encode(device, frame, out)
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

pub(crate) struct EachOwned<'a> {
    pub(crate) devices: Vec<Vec<Box<dyn Operation + 'a>>>,
    pub(crate) slot_frames: Vec<usize>,
}

impl Operation for EachOwned<'_> {
    fn frames(&self) -> usize {
        self.slot_frames.iter().sum()
    }

    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(
        &self,
        device: usize,
        frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        each_encode(&self.devices, &self.slot_frames, device, frame, out)
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        each_reflect(&self.devices, device, state)
    }
}
