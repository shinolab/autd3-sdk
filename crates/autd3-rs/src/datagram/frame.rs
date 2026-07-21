use crate::commands::operation::{Distribution, Operation};
use crate::error::Error;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::each::{each_encode, each_slot_frames};

#[derive(Clone, Debug)]
pub struct Datagram {
    pub cmd: Cmd,
    pub payload: [u8; PAYLOAD_BYTES],
}

impl Datagram {
    #[must_use]
    pub const fn no_payload(cmd: Cmd) -> Self {
        Self {
            cmd,
            payload: [0u8; PAYLOAD_BYTES],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    dist: Distribution,
    datagrams: &'a [Datagram],
}

impl<'a> Frame<'a> {
    #[must_use]
    pub fn distribution(&self) -> Distribution {
        self.dist
    }

    #[must_use]
    pub fn datagrams(&self) -> &'a [Datagram] {
        self.datagrams
    }
}

#[derive(Debug)]
struct FrameDesc {
    dist: Distribution,
    start: usize,
    len: usize,
}

#[derive(Debug, Default)]
pub struct Frames {
    pub(crate) payloads: Vec<Datagram>,
    frames: Vec<FrameDesc>,
}

impl Frames {
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    #[must_use]
    pub fn frame(&self, index: usize) -> Option<Frame<'_>> {
        self.frames.get(index).map(|desc| Frame {
            dist: desc.dist,
            datagrams: &self.payloads[desc.start..desc.start + desc.len],
        })
    }

    #[must_use]
    pub fn iter(&self) -> FrameIter<'_> {
        FrameIter {
            frames: self,
            index: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.payloads.clear();
        self.frames.clear();
    }

    pub(crate) fn push_op<O: Operation + ?Sized>(
        &mut self,
        op: &O,
        num_devices: usize,
    ) -> Result<(), Error> {
        let dist = op.distribution();
        let encode_devices = match dist {
            Distribution::Broadcast => 1,
            Distribution::PerDevice => num_devices,
        };
        for frame in 0..op.frames() {
            let start = self.payloads.len();
            for device in 0..encode_devices {
                let mut payload = [0u8; PAYLOAD_BYTES];
                let cmd = op.encode(device, frame, &mut payload)?;
                self.payloads.push(Datagram { cmd, payload });
            }
            self.frames.push(FrameDesc {
                dist,
                start,
                len: encode_devices,
            });
        }
        Ok(())
    }

    pub(crate) fn push_each_step(
        &mut self,
        devices: &[Vec<Box<dyn Operation + '_>>],
        num_devices: usize,
    ) -> Result<(), Error> {
        let slot_frames = each_slot_frames(devices);
        let total: usize = slot_frames.iter().sum();
        for frame in 0..total {
            let start = self.payloads.len();
            for device in 0..num_devices {
                let mut payload = [0u8; PAYLOAD_BYTES];
                let cmd = each_encode(devices, &slot_frames, device, frame, &mut payload)?;
                self.payloads.push(Datagram { cmd, payload });
            }
            self.frames.push(FrameDesc {
                dist: Distribution::PerDevice,
                start,
                len: num_devices,
            });
        }
        Ok(())
    }
}

pub struct FrameIter<'a> {
    frames: &'a Frames,
    index: usize,
}

impl<'a> Iterator for FrameIter<'a> {
    type Item = Frame<'a>;

    fn next(&mut self) -> Option<Frame<'a>> {
        let frame = self.frames.frame(self.index)?;
        self.index += 1;
        Some(frame)
    }
}

impl<'a> IntoIterator for &'a Frames {
    type Item = Frame<'a>;
    type IntoIter = FrameIter<'a>;

    fn into_iter(self) -> FrameIter<'a> {
        self.iter()
    }
}
