use crate::client::MAX_DEVICES;
use crate::commands::operation::{Distribution, Operation};
use crate::error::{Error, PayloadError};
use crate::geometry::Geometry;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::each::{each_encode, each_frames};

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

fn encode_slots(
    slots: &mut [Datagram],
    mut encode: impl FnMut(usize, &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error>,
) -> Result<(), Error> {
    for (device, slot) in slots.iter_mut().enumerate() {
        slot.cmd = encode(device, &mut slot.payload)?;
    }
    Ok(())
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
        geometry: &Geometry,
    ) -> Result<(), Error> {
        if geometry.is_empty() {
            return Err(PayloadError::DeviceCountOutOfRange {
                got: 0,
                max: MAX_DEVICES,
            }
            .into());
        }
        let dist = op.distribution();
        let encode_devices = match dist {
            Distribution::Broadcast => 1,
            Distribution::PerDevice => geometry.num_devices(),
        };
        let start = self.payloads.len();
        self.payloads
            .resize_with(start + encode_devices, || Datagram::no_payload(Cmd::Nop));
        if let Err(e) = encode_slots(&mut self.payloads[start..], |device, payload| {
            op.encode(&geometry[device], payload)
        }) {
            self.payloads.truncate(start);
            return Err(e);
        }
        self.frames.push(FrameDesc {
            dist,
            start,
            len: encode_devices,
        });
        Ok(())
    }

    pub(crate) fn push_each_step(
        &mut self,
        devices: &[Vec<Box<dyn Operation + '_>>],
        geometry: &Geometry,
    ) -> Result<(), Error> {
        let num_devices = geometry.num_devices();
        for frame in 0..each_frames(devices) {
            let start = self.payloads.len();
            self.payloads
                .resize_with(start + num_devices, || Datagram::no_payload(Cmd::Nop));
            if let Err(e) = encode_slots(&mut self.payloads[start..], |device, payload| {
                each_encode(devices, &geometry[device], frame, payload)
            }) {
                self.payloads.truncate(start);
                return Err(e);
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
