use std::sync::Arc;

use autd3_rs_core::geometry::{Device, Geometry};

use crate::legacy::command::LegacyCommand;
use crate::legacy::error::LegacyError;
use crate::legacy::op::{LegacyOperation, Nop};
use crate::legacy::wire::{PAYLOAD_BYTES, TxFrame};

use super::frame::LegacyFrames;

trait DynOperation<'a> {
    fn boxed_clone(&self) -> Box<dyn DynOperation<'a> + 'a>;
    fn pack(&mut self, device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError>;
    fn is_done(&self) -> bool;
}

impl<'a, O: LegacyOperation + Clone + 'a> DynOperation<'a> for O {
    fn boxed_clone(&self) -> Box<dyn DynOperation<'a> + 'a> {
        Box::new(self.clone())
    }

    fn pack(&mut self, device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        LegacyOperation::pack(self, device, tx)
    }

    fn is_done(&self) -> bool {
        LegacyOperation::is_done(self)
    }
}

enum Step<'a> {
    All(Box<dyn DynOperation<'a> + 'a>),
    Each(Vec<Vec<Box<dyn DynOperation<'a> + 'a>>>),
}

pub struct LegacyDatagramBuilder<'a> {
    geometry: Arc<Geometry>,
    steps: Vec<Step<'a>>,
    dc_offset_ns: i64,
}

impl<'a> LegacyDatagramBuilder<'a> {
    #[must_use]
    pub fn new(geometry: Arc<Geometry>) -> Self {
        Self::with_dc_offset(geometry, 0)
    }

    #[must_use]
    pub fn with_dc_offset(geometry: Arc<Geometry>, dc_offset_ns: i64) -> Self {
        Self {
            geometry,
            steps: Vec::new(),
            dc_offset_ns,
        }
    }

    #[must_use]
    pub(crate) const fn dc_offset_ns(&self) -> i64 {
        self.dc_offset_ns
    }

    pub fn push<C: LegacyCommand<'a>>(&mut self, cmd: C) -> &mut Self {
        tracing::trace!(command = std::any::type_name::<C>(), "pushed command");
        cmd.expand(self);
        self
    }

    pub fn push_each<C, F>(&mut self, mut assign: F) -> &mut Self
    where
        C: LegacyCommand<'a>,
        F: FnMut(&Device) -> Option<C>,
    {
        let geometry = Arc::clone(&self.geometry);
        let devices = geometry
            .iter()
            .enumerate()
            .map(|(idx, device)| {
                assign(device).map_or_else(Vec::new, |cmd| {
                    let mut sub = LegacyDatagramBuilder::with_dc_offset(
                        Arc::clone(&geometry),
                        self.dc_offset_ns,
                    );
                    cmd.expand(&mut sub);
                    sub.queue_for(idx)
                })
            })
            .collect::<Vec<_>>();

        tracing::trace!(
            command = std::any::type_name::<C>(),
            assigned = devices.iter().filter(|ops| !ops.is_empty()).count(),
            num_devices = geometry.num_devices(),
            "pushed per-device commands"
        );

        self.steps.push(Step::Each(devices));
        self
    }

    pub(crate) fn push_op<O: LegacyOperation + Clone + 'a>(&mut self, op: O) -> &mut Self {
        self.steps.push(Step::All(Box::new(op)));
        self
    }

    fn queue_for(&self, device: usize) -> Vec<Box<dyn DynOperation<'a> + 'a>> {
        self.steps
            .iter()
            .flat_map(|step| match step {
                Step::All(op) => vec![op.boxed_clone()],
                Step::Each(devices) => devices[device]
                    .iter()
                    .map(|op| op.boxed_clone())
                    .collect::<Vec<_>>(),
            })
            .collect()
    }

    pub fn build(&self) -> Result<LegacyFrames, LegacyError> {
        let mut out = LegacyFrames::default();
        self.build_into(&mut out)?;
        Ok(out)
    }

    pub fn build_into(&self, out: &mut LegacyFrames) -> Result<(), LegacyError> {
        out.clear();

        if self.steps.is_empty() {
            return Ok(());
        }
        let num_devices = self.geometry.num_devices();
        if num_devices == 0 {
            return Err(LegacyError::NoDevices);
        }

        let mut queues = (0..num_devices)
            .map(|device| self.queue_for(device))
            .collect::<Vec<_>>();
        let mut cursors = vec![0usize; num_devices];

        let mut rounds = 0usize;
        while cursors
            .iter()
            .zip(&queues)
            .any(|(cursor, queue)| *cursor < queue.len())
        {
            let round = out.reserve_round(num_devices);
            for (device, frame) in round.iter_mut().enumerate() {
                if let Err(e) = pack_device(
                    &mut queues[device],
                    &mut cursors[device],
                    &self.geometry[device],
                    frame,
                ) {
                    out.clear();
                    return Err(e);
                }
            }
            rounds += 1;
        }

        tracing::trace!(
            steps = self.steps.len(),
            frames = rounds,
            "built legacy frames"
        );
        Ok(())
    }
}

fn pack_device<'a>(
    queue: &mut [Box<dyn DynOperation<'a> + 'a>],
    cursor: &mut usize,
    device: &Device,
    frame: &mut TxFrame,
) -> Result<(), LegacyError> {
    frame.header.slot_2_offset = 0;
    frame.payload.fill(0);

    if *cursor >= queue.len() {
        LegacyOperation::pack(&mut Nop::new(), device, &mut frame.payload)?;
        return Ok(());
    }

    let written = queue[*cursor].pack(device, &mut frame.payload)?;
    debug_assert_eq!(written % 2, 0);
    debug_assert!(written <= PAYLOAD_BYTES);
    if queue[*cursor].is_done() {
        *cursor += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Geometry};
    use autd3_rs_core::value::{Emission, SamplingConfig};
    use core::num::NonZeroU16;

    use super::*;
    use crate::legacy::op::{Clear, ForceFan, Gain, Modulation, ModulationOption, Sync};
    use crate::legacy::wire::Tag;

    fn geometry(n: usize) -> Arc<Geometry> {
        Arc::new(Geometry::new((0..n).map(|_| Autd3::default()).collect()))
    }

    #[test]
    fn an_empty_builder_produces_no_frames() {
        let mut b = LegacyDatagramBuilder::new(geometry(2));
        let frames = b.build().unwrap();
        assert_eq!(frames.len(), 0);
        assert!(frames.is_empty());
        b.push_op(Nop::new());
        assert_eq!(b.build().unwrap().len(), 1);
    }

    #[test]
    fn each_op_gets_its_own_frame_and_slot_2_is_never_used() {
        let mut b = LegacyDatagramBuilder::new(geometry(1));
        b.push_op(Clear::new())
            .push_op(Sync::new())
            .push_op(Nop::new());
        let frames = b.build().unwrap();

        assert_eq!(frames.len(), 3, "one operation per frame");
        for (round, tag) in [Tag::Clear, Tag::Sync, Tag::Nop].into_iter().enumerate() {
            let round = frames.frame(round).unwrap();
            let tx = &round.frames()[0];
            assert_eq!(tx.payload[0], tag.as_u8());
            assert_eq!(tx.header.slot_2_offset, 0);
        }
    }

    #[test]
    fn a_multi_frame_op_keeps_the_following_op_in_a_later_frame() {
        let geo = geometry(1);
        let data = vec![0x80u8; 300];
        let emissions = vec![vec![Emission::NULL; geo[0].num_transducers()]];

        let mut b = LegacyDatagramBuilder::new(Arc::clone(&geo));
        b.push_op(Modulation::new(
            SamplingConfig::new(NonZeroU16::new(0xFFFF).unwrap()),
            &data,
            ModulationOption::default(),
        ))
        .push_op(Gain::new(&emissions));
        let frames = b.build().unwrap();

        assert_eq!(frames.len(), 3, "2 modulation rounds, then the gain");
        for round in 0..2 {
            let round = frames.frame(round).unwrap();
            let tx = &round.frames()[0];
            assert_eq!(tx.payload[0], Tag::Modulation.as_u8());
            assert_eq!(tx.header.slot_2_offset, 0);
        }
        assert_eq!(
            frames.frame(2).unwrap().frames()[0].payload[0],
            Tag::Gain.as_u8()
        );
    }

    #[test]
    fn build_is_idempotent() {
        let geo = geometry(1);
        let data = vec![0x80u8; 700];
        let mut b = LegacyDatagramBuilder::new(geo);
        b.push_op(Modulation::new(
            SamplingConfig::new(NonZeroU16::new(0xFFFF).unwrap()),
            &data,
            ModulationOption::default(),
        ));
        let first = b.build().unwrap();
        let second = b.build().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn push_each_assigns_a_command_per_device() {
        let mut b = LegacyDatagramBuilder::new(geometry(3));
        b.push_each(|device| match device.idx() {
            0 => Some(ForceFan::new(true)),
            1 => Some(ForceFan::new(false)),
            _ => None,
        });
        let frames = b.build().unwrap();

        assert_eq!(frames.len(), 1);
        let round = frames.frame(0).unwrap();
        let round = round.frames();
        assert_eq!(round[0].payload[0], Tag::ForceFan.as_u8());
        assert_eq!(round[0].payload[1], 1);
        assert_eq!(round[1].payload[0], Tag::ForceFan.as_u8());
        assert_eq!(round[1].payload[1], 0);
        assert_eq!(round[2].payload[0], Tag::Nop.as_u8());
    }

    #[test]
    fn push_each_returning_none_everywhere_produces_no_frames() {
        let mut b = LegacyDatagramBuilder::new(geometry(2));
        b.push_each(|_| None::<Clear>);
        assert_eq!(b.build().unwrap().len(), 0);
    }

    #[test]
    fn consecutive_push_each_share_a_round_when_disjoint() {
        let mut b = LegacyDatagramBuilder::new(geometry(2));
        b.push_each(|device| (device.idx() == 0).then(Clear::new))
            .push_each(|device| (device.idx() == 1).then(Sync::new));
        let frames = b.build().unwrap();

        assert_eq!(frames.len(), 1);
        let round = frames.frame(0).unwrap();
        let round = round.frames();
        assert_eq!(round[0].payload[0], Tag::Clear.as_u8());
        assert_eq!(round[1].payload[0], Tag::Sync.as_u8());
    }

    #[test]
    fn push_and_push_each_interleave_in_order() {
        let mut b = LegacyDatagramBuilder::new(geometry(2));
        b.push_op(Clear::new())
            .push_each(|device| (device.idx() == 1).then(Sync::new))
            .push_op(Nop::new());
        let frames = b.build().unwrap();

        assert_eq!(frames.len(), 3);
        for (round, tags) in [
            (0, [Tag::Clear, Tag::Clear]),
            (1, [Tag::Nop, Tag::Sync]),
            (2, [Tag::Nop, Tag::Nop]),
        ] {
            let frame = frames.frame(round).unwrap();
            let frames = frame.frames();
            for (device, tag) in tags.into_iter().enumerate() {
                assert_eq!(frames[device].payload[0], tag.as_u8(), "round {round}");
            }
        }
    }

    #[test]
    fn rounds_match_the_longest_device_queue() {
        let geo = geometry(2);
        let data = vec![0x80u8; 300];

        let mut b = LegacyDatagramBuilder::new(Arc::clone(&geo));
        b.push_each(|device| {
            (device.idx() == 0).then(|| {
                Modulation::new(
                    SamplingConfig::new(NonZeroU16::new(0xFFFF).unwrap()),
                    &data,
                    ModulationOption::default(),
                )
            })
        });
        let frames = b.build().unwrap();

        assert_eq!(frames.len(), 2, "2 modulation rounds for device 0");
        for round in 0..2 {
            let frame = frames.frame(round).unwrap();
            let frames = frame.frames();
            assert_eq!(frames[0].payload[0], Tag::Modulation.as_u8());
            assert_eq!(frames[1].payload[0], Tag::Nop.as_u8());
        }
    }

    #[test]
    fn an_encode_error_leaves_no_partial_rounds() {
        let geo = geometry(1);
        let ragged = vec![vec![Emission::NULL; 1]];
        let mut b = LegacyDatagramBuilder::new(geo);
        b.push_op(Gain::new(&ragged));
        assert!(b.build().is_err());
    }

    #[test]
    fn an_encode_error_empties_the_caller_provided_buffer() {
        let geo = geometry(1);
        let data = vec![0x80u8; 300];
        let ragged = vec![vec![Emission::NULL; 1]];

        let mut out = LegacyFrames::default();
        let mut b = LegacyDatagramBuilder::new(Arc::clone(&geo));
        b.push_op(Nop::new());
        b.build_into(&mut out).unwrap();
        assert_eq!(out.len(), 1);

        let mut b = LegacyDatagramBuilder::new(geo);
        b.push_op(Modulation::new(
            SamplingConfig::new(NonZeroU16::new(0xFFFF).unwrap()),
            &data,
            ModulationOption::default(),
        ))
        .push_op(Gain::new(&ragged));
        assert!(b.build_into(&mut out).is_err());
        assert_eq!(out.len(), 0);
        assert!(out.is_empty());
        assert!(out.frame(0).is_none());
    }
}
