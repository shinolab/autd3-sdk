use crate::command::Command;
use crate::error::Error;
use crate::operation::{Distribution, Operation};
use crate::protocol::{Cmd, PAYLOAD_BYTES};

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

struct FrameDesc {
    dist: Distribution,
    start: usize,
    len: usize,
}

#[derive(Default)]
pub struct Datagrams {
    payloads: Vec<Datagram>,
    frames: Vec<FrameDesc>,
}

impl Datagrams {
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
            datagrams: self,
            index: 0,
        }
    }

    fn clear(&mut self) {
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
}

pub struct FrameIter<'a> {
    datagrams: &'a Datagrams,
    index: usize,
}

impl<'a> Iterator for FrameIter<'a> {
    type Item = Frame<'a>;

    fn next(&mut self) -> Option<Frame<'a>> {
        let frame = self.datagrams.frame(self.index)?;
        self.index += 1;
        Some(frame)
    }
}

impl<'a> IntoIterator for &'a Datagrams {
    type Item = Frame<'a>;
    type IntoIter = FrameIter<'a>;

    fn into_iter(self) -> FrameIter<'a> {
        self.iter()
    }
}

pub struct DatagramBuilder<'a> {
    num_devices: usize,
    ops: Vec<Box<dyn Operation + 'a>>,
}

impl<'a> DatagramBuilder<'a> {
    #[must_use]
    pub fn new(num_devices: usize) -> Self {
        Self {
            num_devices,
            ops: Vec::new(),
        }
    }

    pub fn push<C: Command<'a>>(&mut self, cmd: C) -> &mut Self {
        cmd.expand(self);
        self
    }

    pub(crate) fn push_op<O: Operation + 'a>(&mut self, op: O) -> &mut Self {
        self.ops.push(Box::new(op));
        self
    }

    pub fn build(&self) -> Result<Datagrams, Error> {
        let mut out = Datagrams::default();
        self.build_into(&mut out)?;
        Ok(out)
    }

    pub fn build_into(&self, out: &mut Datagrams) -> Result<(), Error> {
        out.clear();
        for op in &self.ops {
            out.push_op(op.as_ref(), self.num_devices)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::{ConfigPattern, WritePatternBuffer};
    use crate::params::NUM_TRANSDUCERS;
    use crate::value::{Emission, PatternBank, PatternDataType};

    #[test]
    fn broadcast_op_yields_one_frame_of_one_datagram() {
        let op = ConfigPattern {
            bank: PatternBank::B0,
            divider: 1,
            size: 1,
            data_type: PatternDataType::Raw,
        };
        let mut b = DatagramBuilder::new(4);
        b.push(op);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 1);
        let frame = datagrams.frame(0).unwrap();
        assert_eq!(frame.distribution(), Distribution::Broadcast);
        assert_eq!(frame.datagrams().len(), 1);
        assert_eq!(frame.datagrams()[0].cmd, Cmd::ConfigPattern);
    }

    #[test]
    fn per_device_op_yields_one_datagram_per_device() {
        let patterns = vec![[Emission::default(); NUM_TRANSDUCERS]; 3];
        let op = WritePatternBuffer {
            bank: PatternBank::B0,
            index: 0,
            emissions: &patterns,
        };
        let mut b = DatagramBuilder::new(3);
        b.push(op);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 1);
        let frame = datagrams.frame(0).unwrap();
        assert_eq!(frame.distribution(), Distribution::PerDevice);
        assert_eq!(frame.datagrams().len(), 3);
    }

    #[test]
    fn composite_emission_orders_write_then_config() {
        let patterns = vec![[Emission::default(); NUM_TRANSDUCERS]; 2];
        let we = WritePatternBuffer {
            bank: PatternBank::B0,
            index: 0,
            emissions: &patterns,
        };
        let ce = ConfigPattern {
            bank: PatternBank::B0,
            divider: 1,
            size: 1,
            data_type: PatternDataType::Raw,
        };
        let mut b = DatagramBuilder::new(2);
        b.push(we).push(ce);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 2);
        assert_eq!(
            datagrams.frame(0).unwrap().distribution(),
            Distribution::PerDevice
        );
        assert_eq!(datagrams.frame(0).unwrap().datagrams().len(), 2);
        assert_eq!(
            datagrams.frame(1).unwrap().distribution(),
            Distribution::Broadcast
        );
        assert_eq!(
            datagrams.frame(1).unwrap().datagrams()[0].cmd,
            Cmd::ConfigPattern
        );
    }

    #[test]
    fn build_into_reuses_buffer_without_growing() {
        let op = ConfigPattern {
            bank: PatternBank::B0,
            divider: 1,
            size: 1,
            data_type: PatternDataType::Raw,
        };
        let mut b = DatagramBuilder::new(1);
        b.push(op);

        let mut buf = Datagrams::default();
        b.build_into(&mut buf).unwrap();
        let cap_after_first = buf.payloads.capacity();
        b.build_into(&mut buf).unwrap();

        assert_eq!(buf.len(), 1);
        assert_eq!(
            buf.payloads.capacity(),
            cap_after_first,
            "second build must not reallocate"
        );
    }
}
