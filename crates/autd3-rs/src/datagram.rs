use std::sync::{Arc, Mutex, PoisonError};

use crate::command::Command;
use crate::error::Error;
use crate::mirror::FirmwareState;
use crate::operation::{Distribution, Nop, Operation};
use crate::protocol::{Cmd, PAYLOAD_BYTES};

#[derive(Clone, Debug)]
pub(crate) enum Mirror {
    Synced(Vec<FirmwareState>),
    Desynced,
}

#[derive(Clone)]
pub(crate) struct MirrorHandle {
    pub(crate) state: Arc<Mutex<Mirror>>,
    pub(crate) enabled: bool,
}

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
    mirror: Option<MirrorHandle>,
}

impl<'a> DatagramBuilder<'a> {
    #[must_use]
    pub fn new(num_devices: usize) -> Self {
        Self {
            num_devices,
            ops: Vec::new(),
            mirror: None,
        }
    }

    #[must_use]
    pub(crate) fn with_mirror(num_devices: usize, mirror: MirrorHandle) -> Self {
        Self {
            num_devices,
            ops: Vec::new(),
            mirror: Some(mirror),
        }
    }

    pub fn push<C: Command<'a>>(&mut self, cmd: C) -> &mut Self {
        cmd.expand(self);
        self
    }

    pub fn push_each<C, F>(&mut self, mut assign: F) -> &mut Self
    where
        C: Command<'a>,
        F: FnMut(usize) -> Option<C>,
    {
        let num_devices = self.num_devices;
        let mut devices: Vec<Vec<Box<dyn Operation + 'a>>> = Vec::with_capacity(num_devices);
        for device in 0..num_devices {
            match assign(device) {
                Some(cmd) => {
                    let mut sub = DatagramBuilder::new(num_devices);
                    cmd.expand(&mut sub);
                    devices.push(sub.take_ops());
                }
                None => devices.push(Vec::new()),
            }
        }

        let num_slots = devices.iter().map(Vec::len).max().unwrap_or(0);
        let mut slot_frames = vec![0usize; num_slots];
        for ops in &devices {
            for (slot, op) in ops.iter().enumerate() {
                slot_frames[slot] = slot_frames[slot].max(op.frames());
            }
        }

        self.push_op(Each {
            devices,
            slot_frames,
        });
        self
    }

    pub(crate) fn push_op<O: Operation + 'a>(&mut self, op: O) -> &mut Self {
        self.ops.push(Box::new(op));
        self
    }

    pub(crate) fn take_ops(self) -> Vec<Box<dyn Operation + 'a>> {
        self.ops
    }

    pub fn build(&self) -> Result<Datagrams, Error> {
        let mut out = Datagrams::default();
        self.build_into(&mut out)?;
        Ok(out)
    }

    pub fn build_into(&self, out: &mut Datagrams) -> Result<(), Error> {
        out.clear();

        let mut guard = self
            .mirror
            .as_ref()
            .filter(|handle| handle.enabled)
            .map(|handle| handle.state.lock().unwrap_or_else(PoisonError::into_inner));

        let mut work = match guard.as_deref() {
            Some(Mirror::Synced(states)) => Some(states.clone()),
            _ => None,
        };

        for op in &self.ops {
            out.push_op(op.as_ref(), self.num_devices)?;
            if let Some(work) = work.as_mut() {
                for (device, state) in work.iter_mut().enumerate() {
                    op.reflect(device, state)?;
                }
            }
        }

        if let (Some(guard), Some(work)) = (guard.as_mut(), work) {
            **guard = Mirror::Synced(work);
        }
        Ok(())
    }
}

struct Each<'a> {
    devices: Vec<Vec<Box<dyn Operation + 'a>>>,
    slot_frames: Vec<usize>,
}

impl<'a> Each<'a> {
    fn locate(&self, frame: usize) -> Option<(usize, usize)> {
        let mut remaining = frame;
        for (slot, &frames) in self.slot_frames.iter().enumerate() {
            if remaining < frames {
                return Some((slot, remaining));
            }
            remaining -= frames;
        }
        None
    }

    fn op_at(&self, device: usize, slot: usize, subframe: usize) -> Option<&(dyn Operation + 'a)> {
        let op = self.devices.get(device)?.get(slot)?;
        (subframe < op.frames()).then(|| op.as_ref())
    }
}

impl Operation for Each<'_> {
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
        match self.locate(frame) {
            Some((slot, subframe)) => match self.op_at(device, slot, subframe) {
                Some(op) => op.encode(device, subframe, out),
                None => Nop.encode(device, subframe, out),
            },
            None => Nop.encode(device, frame, out),
        }
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        if let Some(ops) = self.devices.get(device) {
            for op in ops {
                op.reflect(device, state)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Pattern;
    use crate::operation::{ConfigModulation, ConfigPattern, WritePatternBuffer};
    use crate::value::ModulationBank;

    #[derive(Clone, Copy)]
    struct Multi(usize);

    impl Operation for Multi {
        fn frames(&self) -> usize {
            self.0
        }

        fn distribution(&self) -> Distribution {
            Distribution::PerDevice
        }

        fn encode(
            &self,
            _device: usize,
            frame: usize,
            out: &mut [u8; PAYLOAD_BYTES],
        ) -> Result<Cmd, Error> {
            out[0] = u8::try_from(frame).unwrap();
            Ok(Cmd::ConfigModulation)
        }
    }

    fn cmd_at(datagrams: &Datagrams, frame: usize, device: usize) -> Cmd {
        datagrams.frame(frame).unwrap().datagrams()[device].cmd
    }

    #[test]
    fn push_each_routes_per_device() {
        let mut b = DatagramBuilder::new(2);
        b.push_each(|device| {
            Some(ConfigModulation {
                bank: if device == 0 {
                    ModulationBank::B0
                } else {
                    ModulationBank::B1
                },
                divider: 1,
                size: 1,
            })
        });
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 1);
        let frame = datagrams.frame(0).unwrap();
        assert_eq!(frame.distribution(), Distribution::PerDevice);
        assert_eq!(frame.datagrams()[0].payload[0], 0, "device 0 -> bank B0");
        assert_eq!(frame.datagrams()[1].payload[0], 1, "device 1 -> bank B1");
    }

    #[test]
    fn push_each_fills_unassigned_with_nop() {
        let mut b = DatagramBuilder::new(2);
        b.push_each(|device| {
            (device == 0).then_some(ConfigModulation {
                bank: ModulationBank::B0,
                divider: 1,
                size: 1,
            })
        });
        let datagrams = b.build().unwrap();

        assert_eq!(cmd_at(&datagrams, 0, 0), Cmd::ConfigModulation);
        assert_eq!(cmd_at(&datagrams, 0, 1), Cmd::Nop, "unassigned -> Nop");
    }

    #[test]
    fn push_each_pads_shorter_device_with_nop() {
        let mut b = DatagramBuilder::new(2);
        b.push_each(|device| Some(if device == 0 { Multi(1) } else { Multi(3) }));
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 3, "frame count = max over devices");
        assert_eq!(cmd_at(&datagrams, 0, 0), Cmd::ConfigModulation);
        assert_eq!(cmd_at(&datagrams, 1, 0), Cmd::Nop);
        assert_eq!(cmd_at(&datagrams, 2, 0), Cmd::Nop);
        for frame in 0..3 {
            assert_eq!(cmd_at(&datagrams, frame, 1), Cmd::ConfigModulation);
            assert_eq!(
                datagrams.frame(frame).unwrap().datagrams()[1].payload[0] as usize,
                frame
            );
        }
    }

    #[test]
    fn push_each_accepts_heterogeneous_boxed_commands() {
        let patterns = vec![[crate::value::Emission::default(); crate::params::NUM_TRANSDUCERS]; 2];
        let mut b = DatagramBuilder::new(2);
        b.push_each(|device| {
            Some(if device == 0 {
                Pattern::new(&patterns).boxed()
            } else {
                ConfigModulation {
                    bank: ModulationBank::B0,
                    divider: 1,
                    size: 1,
                }
                .boxed()
            })
        });
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 3);
        assert_eq!(cmd_at(&datagrams, 0, 0), Cmd::WritePatternBuffer);
        assert_eq!(cmd_at(&datagrams, 0, 1), Cmd::ConfigModulation);
        assert_eq!(cmd_at(&datagrams, 1, 1), Cmd::Nop);
        assert_eq!(cmd_at(&datagrams, 2, 1), Cmd::Nop);
    }
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
