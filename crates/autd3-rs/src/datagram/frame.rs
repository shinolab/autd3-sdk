use crate::commands::Command;
use crate::commands::operation::{Distribution, Operation};
use crate::error::{Error, PayloadError};
use crate::geometry::{Device, Geometry};
use crate::protocol::{Cmd, FrameHeader, PAYLOAD_BYTES};
use autd3_rs_core::MAX_DEVICES;

use super::expansion::Expansion;

#[derive(Clone, Debug)]
pub struct Datagram {
    pub cmd: Cmd,
    pub payload: [u8; PAYLOAD_BYTES],
    pub payload_len: usize,
}

impl Datagram {
    #[must_use]
    pub const fn no_payload(cmd: Cmd) -> Self {
        Self {
            cmd,
            payload: [0u8; PAYLOAD_BYTES],
            payload_len: 0,
        }
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload[..self.payload_len]
    }

    #[must_use]
    pub(crate) fn frame_header(&self, exec_at: u32) -> FrameHeader {
        FrameHeader {
            reserved: 0,
            cmd: self.cmd.as_u8(),
            exec_at: exec_at.into(),
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
    pub const fn new(dist: Distribution, datagrams: &'a [Datagram]) -> Self {
        Self { dist, datagrams }
    }

    #[must_use]
    pub fn distribution(&self) -> Distribution {
        self.dist
    }

    #[must_use]
    pub fn datagrams(&self) -> &'a [Datagram] {
        self.datagrams
    }

    #[must_use]
    pub fn datagram_for(&self, device: usize) -> &'a Datagram {
        &self.datagrams[self.datagram_index(device)]
    }

    pub(crate) const fn datagram_index(&self, device: usize) -> usize {
        match self.dist {
            Distribution::Broadcast => 0,
            Distribution::PerDevice => device,
        }
    }
}

#[derive(Debug)]
struct FrameRange {
    dist: Distribution,
    start: usize,
    len: usize,
}

#[derive(Debug, Default)]
pub struct Frames {
    datagrams: Vec<Datagram>,
    frame_ranges: Vec<FrameRange>,
}

impl Frames {
    pub fn encode(geometry: &Geometry, cmd: impl Command) -> Result<Self, Error> {
        let mut frames = Self::default();
        frames.encode_into(geometry, cmd)?;
        Ok(frames)
    }

    pub fn encode_into(&mut self, geometry: &Geometry, cmd: impl Command) -> Result<(), Error> {
        self.clear();
        let encoded = Expansion::new(geometry, self).push(cmd).map(|_| ());
        if encoded.is_err() {
            self.clear();
        }
        tracing::trace!(frames = self.len(), "encoded frames");
        encoded
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.frame_ranges.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frame_ranges.is_empty()
    }

    #[must_use]
    pub fn frame(&self, index: usize) -> Option<Frame<'_>> {
        self.frame_ranges.get(index).map(|range| Frame {
            dist: range.dist,
            datagrams: &self.datagrams[range.start..][..range.len],
        })
    }

    #[must_use]
    pub fn iter(&self) -> FrameIter<'_> {
        FrameIter {
            frames: self,
            index: 0,
        }
    }

    fn clear(&mut self) {
        self.datagrams.clear();
        self.frame_ranges.clear();
    }

    fn push_frame(&mut self, dist: Distribution, num_devices: usize) -> usize {
        let len = dist.num_datagrams(num_devices);
        let start = self.datagrams.len();
        self.datagrams
            .resize_with(start + len, || Datagram::no_payload(Cmd::Nop));
        self.frame_ranges.push(FrameRange { dist, start, len });
        start
    }

    pub(super) fn push_encoded(
        &mut self,
        op: &impl Operation,
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
        let start = self.push_frame(dist, geometry.num_devices());
        for (device, datagram) in self.datagrams[start..].iter_mut().enumerate() {
            op.encode(&geometry[device], datagram)?;
        }
        tracing::trace!(
            cmd = ?self.datagrams[start].cmd,
            dist = ?dist,
            devices = self.datagrams.len() - start,
            "encoded frame"
        );
        Ok(())
    }

    pub(super) fn push_per_device<O: Operation>(
        &mut self,
        geometry: &Geometry,
        mut assign: impl FnMut(&Device) -> Option<O>,
    ) -> Result<usize, Error> {
        let mut start = None;
        let mut assigned = 0;
        for device in geometry {
            let Some(op) = assign(device) else {
                continue;
            };
            assigned += 1;
            let start = *start.get_or_insert_with(|| {
                self.push_frame(Distribution::PerDevice, geometry.num_devices())
            });
            op.encode(device, &mut self.datagrams[start + device.idx()])?;
        }
        Ok(assigned)
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

#[cfg(test)]
mod tests {
    use autd3_cpu_wire::payload::{
        ActivateModBankPayload, ActivatePatternBankPayload, GpioOutPayload,
    };

    use super::Frames;
    use crate::commands::operation::{
        ActivatePatternBank, ConfigPattern, Distribution, GpioOut, SetGpioOut, WritePatternBuffer,
    };
    use crate::commands::{Command, Modulation};
    use crate::datagram::{Expansion, each};
    use crate::error::{Error, PayloadError};
    use crate::geometry::Autd3;
    use crate::protocol::Cmd;
    use crate::test_utils::{
        FailAt, FailBroadcast, Marker, build, cmd_at, cmds, payload, test_geometry,
    };
    use crate::value::{
        Intensity, LoopBehavior, ModulationBank, PatternBank, Phase, SamplingConfig, SysTime,
        TransitionMode,
    };

    #[test]
    fn the_frame_header_carries_the_cmd_and_exec_at() {
        use zerocopy::IntoBytes;
        let datagram = super::Datagram::no_payload(Cmd::Nop);
        assert_eq!(
            datagram.frame_header(0x0403_0201).as_bytes(),
            &[0, Cmd::Nop.as_u8(), 0x01, 0x02, 0x03, 0x04]
        );
    }

    struct Pushes<F>(F);

    fn pushes<F: FnOnce(&mut Expansion<'_>) -> Result<(), Error>>(push: F) -> Pushes<F> {
        Pushes(push)
    }

    impl<F: FnOnce(&mut Expansion<'_>) -> Result<(), Error>> Command for Pushes<F> {
        fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
            (self.0)(expansion)
        }
    }

    const DEVICE_TIME: SysTime = SysTime::from_nanos(2_000_000_000);

    const CONFIG: ConfigPattern = ConfigPattern {
        bank: PatternBank::B0,
        config: SamplingConfig::FREQ_40K,
        size: 2,
        loop_behavior: LoopBehavior::Infinite,
    };

    impl Frames {
        pub(crate) fn datagram_capacity(&self) -> usize {
            self.datagrams.capacity()
        }
    }

    #[test]
    fn a_per_device_op_and_a_broadcast_op_keep_their_shape() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let write = WritePatternBuffer::new(
            PatternBank::B0,
            0,
            core::slice::from_ref(&phases),
            &intensities,
        );
        let per_device = build(2, write).unwrap();
        let broadcast = build(2, CONFIG).unwrap();

        assert_eq!(per_device.len(), 1);
        let per_device = per_device.frame(0).unwrap();
        assert_eq!(per_device.distribution(), Distribution::PerDevice);
        assert_eq!(per_device.datagrams().len(), 2);
        assert_eq!(broadcast.len(), 1);
        let broadcast = broadcast.frame(0).unwrap();
        assert_eq!(broadcast.distribution(), Distribution::Broadcast);
        assert_eq!(broadcast.datagrams().len(), 1);
        assert_eq!(broadcast.datagrams()[0].cmd, Cmd::ConfigPattern);
    }

    #[test]
    fn pushed_commands_expand_in_order() {
        let frames = build(
            1,
            pushes(|expansion| {
                expansion
                    .push(Marker(3))?
                    .push(CONFIG)?
                    .push(Marker(5))?
                    .push(Marker(7))?;
                Ok(())
            }),
        )
        .unwrap();

        assert_eq!(
            cmds(&frames),
            [
                Cmd::ConfigModulation,
                Cmd::ConfigPattern,
                Cmd::ConfigModulation,
                Cmd::ConfigModulation,
            ]
        );
        assert_eq!(payload(&frames, 0, 0)[0], 3);
        assert_eq!(payload(&frames, 2, 0)[0], 5);
        assert_eq!(payload(&frames, 3, 0)[0], 7);
    }

    #[test]
    fn a_failing_op_leaves_no_frame_behind() {
        let mut frames = Frames::default();
        let result = frames.encode_into(
            &test_geometry(2),
            pushes(|expansion| {
                expansion.push(Marker(1))?.push(FailAt(1))?;
                Ok(())
            }),
        );

        assert!(matches!(result, Err(Error::InvalidPayload(_))));
        assert!(frames.is_empty());
        assert_eq!(frames.iter().count(), 0);
    }

    #[test]
    fn a_failing_op_stops_the_expansion_at_once() {
        struct Expanded<'c>(&'c std::cell::Cell<bool>);

        impl Command for Expanded<'_> {
            fn expand(self, _expansion: &mut Expansion<'_>) -> Result<(), Error> {
                self.0.set(true);
                Ok(())
            }
        }

        let expanded = std::cell::Cell::new(false);
        let result = build(
            2,
            pushes(|expansion| {
                expansion.push(FailAt(0))?.push(Expanded(&expanded))?;
                Ok(())
            }),
        );

        assert!(matches!(result, Err(Error::InvalidPayload(_))));
        assert!(!expanded.get());
    }

    #[test]
    fn a_swallowed_failure_leaves_its_frame_in_place_for_the_commands_that_follow() {
        struct SwallowsFailures;

        impl Command for SwallowsFailures {
            fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
                assert!(expansion.push(FailBroadcast).is_err());
                assert!(expansion.push(FailAt(1)).is_err());
                assert!(expansion.push(each(|_| Some(FailAt(0)))).is_err());
                expansion.push(Marker(5))?;
                expansion.push(each(|d| Some(Marker(u8::try_from(d.idx()).unwrap() + 7))))?;
                Ok(())
            }
        }

        let frames = build(2, SwallowsFailures).unwrap();

        assert_eq!(frames.len(), 5);
        let failed_broadcast = frames.frame(0).unwrap();
        assert_eq!(failed_broadcast.distribution(), Distribution::Broadcast);
        assert_eq!(failed_broadcast.datagrams().len(), 1);
        assert_eq!(frames.frame(1).unwrap().datagrams().len(), 2);
        assert_eq!(frames.frame(2).unwrap().datagrams().len(), 2);
        assert_eq!(payload(&frames, 3, 0)[0], 5);
        assert_eq!(payload(&frames, 3, 1)[0], 5);
        assert_eq!(payload(&frames, 4, 0)[0], 7);
        assert_eq!(payload(&frames, 4, 1)[0], 8);
    }

    #[test]
    fn encode_into_replaces_the_previous_frames() {
        let geometry = test_geometry(1);
        let mut frames = Frames::default();
        frames
            .encode_into(
                &geometry,
                pushes(|expansion| {
                    expansion.push(Marker(1))?.push(Marker(2))?;
                    Ok(())
                }),
            )
            .unwrap();
        frames.encode_into(&geometry, Marker(9)).unwrap();

        assert_eq!(frames.len(), 1);
        assert_eq!(payload(&frames, 0, 0)[0], 9);
    }

    #[test]
    fn a_command_on_an_empty_geometry_is_rejected() {
        let expected = PayloadError::DeviceCountOutOfRange {
            got: 0,
            max: autd3_rs_core::MAX_DEVICES,
        };

        let result = build(0, Marker(0));

        assert!(matches!(result, Err(Error::InvalidPayload(e)) if e == expected));
    }

    #[test]
    fn re_encoding_reuses_the_buffer_without_growing() {
        let geometry = test_geometry(1);
        let mut frames = Frames::default();
        frames.encode_into(&geometry, CONFIG).unwrap();
        let capacity = frames.datagram_capacity();
        frames.encode_into(&geometry, CONFIG).unwrap();

        assert_eq!(frames.len(), 1);
        assert_eq!(frames.datagram_capacity(), capacity);
    }

    #[test]
    fn a_sys_time_transition_goes_out_as_the_device_time_the_caller_wrote() {
        let cmd = ActivatePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::SysTime { time: DEVICE_TIME },
        };
        let frames = build(1, cmd).unwrap();
        let p = ActivatePatternBankPayload::parse(payload(&frames, 0, 0)).unwrap();
        assert_eq!(p.transition_value.get(), DEVICE_TIME.sys_time());

        let frames = build(2, each(|_| Some(cmd))).unwrap();
        for device in 0..2 {
            let p = ActivatePatternBankPayload::parse(payload(&frames, 0, device)).unwrap();
            assert_eq!(p.transition_value.get(), DEVICE_TIME.sys_time());
        }
    }

    #[test]
    fn a_gpio_sys_time_trigger_goes_out_as_the_device_time_the_caller_wrote() {
        let frames = build(
            1,
            SetGpioOut {
                outputs: [
                    GpioOut::SysTimeEq(DEVICE_TIME),
                    GpioOut::Off,
                    GpioOut::Off,
                    GpioOut::Off,
                ],
            },
        )
        .unwrap();
        let p = GpioOutPayload::parse(payload(&frames, 0, 0)).unwrap();

        assert_eq!(
            p.values[0].get() & 0x00FF_FFFF_FFFF_FFFF,
            ((DEVICE_TIME.sys_time() / 3125) << 6) >> 9
        );
    }

    #[test]
    fn a_sys_time_transition_reaches_the_modulation_activate_frame() {
        let data = [0u8; 4];
        let frames = build(
            1,
            Modulation {
                bank: ModulationBank::B0,
                config: SamplingConfig::FREQ_4K,
                data: &data,
                loop_behavior: LoopBehavior::Finite(std::num::NonZeroU16::new(1).unwrap()),
                transition_mode: TransitionMode::SysTime { time: DEVICE_TIME },
            },
        )
        .unwrap();

        assert_eq!(cmd_at(&frames, 2, 0), Cmd::ActivateModulationBank);
        let p = ActivateModBankPayload::parse(payload(&frames, 2, 0)).unwrap();
        assert_eq!(p.transition_value.get(), DEVICE_TIME.sys_time());
    }
}
