use crate::commands::Command;
use crate::commands::operation::{Distribution, Operation};
use crate::error::{Error, PayloadError};
use crate::geometry::Geometry;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
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
        match self.dist {
            Distribution::Broadcast => &self.datagrams[0],
            Distribution::PerDevice => &self.datagrams[device],
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
    pub fn encode<'a>(geometry: &Geometry, cmd: impl Command<'a>) -> Result<Self, Error> {
        let mut frames = Self::default();
        frames.encode_into(geometry, cmd)?;
        Ok(frames)
    }

    pub fn encode_into<'a>(
        &mut self,
        geometry: &Geometry,
        cmd: impl Command<'a>,
    ) -> Result<(), Error> {
        self.clear();

        let mut expansion = Expansion::new(geometry);
        expansion.push(cmd)?;
        let ops = expansion.ops;

        if geometry.is_empty() && !ops.is_empty() {
            return Err(PayloadError::DeviceCountOutOfRange {
                got: 0,
                max: MAX_DEVICES,
            }
            .into());
        }

        let encoded = ops
            .iter()
            .try_for_each(|op| self.encode_op(op.as_ref(), geometry));
        if encoded.is_err() {
            self.clear();
        }
        tracing::trace!(ops = ops.len(), frames = self.len(), "encoded frames");
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

    fn encode_op(&mut self, op: &dyn Operation, geometry: &Geometry) -> Result<(), Error> {
        fn encode_into_datagrams(
            per_device: &mut [Datagram],
            op: &dyn Operation,
            geometry: &Geometry,
        ) -> Result<(), Error> {
            for (device, datagram) in per_device.iter_mut().enumerate() {
                let encoded = op.encode(&geometry[device], &mut datagram.payload)?;
                debug_assert!(encoded.len <= PAYLOAD_BYTES);
                datagram.cmd = encoded.cmd;
                datagram.payload_len = encoded.len;
            }
            Ok(())
        }

        debug_assert!(!geometry.is_empty());
        let dist = op.distribution();
        let len = match dist {
            Distribution::Broadcast => 1,
            Distribution::PerDevice => geometry.num_devices(),
        };
        let start = self.datagrams.len();
        self.datagrams
            .resize_with(start + len, || Datagram::no_payload(Cmd::Nop));
        if let Err(e) = encode_into_datagrams(&mut self.datagrams[start..], op, geometry) {
            self.datagrams.truncate(start);
            return Err(e);
        }
        tracing::trace!(
            cmd = ?self.datagrams[start].cmd,
            dist = ?dist,
            devices = len,
            "encoded frame"
        );
        self.frame_ranges.push(FrameRange { dist, start, len });
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

#[cfg(test)]
mod tests {
    use autd3_cpu_wire::payload::{
        ActivateModBankPayload, ActivatePatternBankPayload, GpioOutPayload,
    };

    use super::Frames;
    use crate::commands::Modulation;
    use crate::commands::operation::{
        ActivatePatternBank, ConfigPattern, Distribution, GpioOut, SetGpioOut, WritePatternBuffer,
    };
    use crate::datagram::each;
    use crate::error::Error;
    use crate::geometry::Autd3;
    use crate::protocol::Cmd;
    use crate::test_utils::{FailAt, Marker, build, cmd_at, cmds, payload, test_geometry};
    use crate::value::{
        Intensity, LoopBehavior, ModulationBank, PatternBank, Phase, SamplingConfig, SysTime,
        TransitionMode,
    };

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
    fn a_per_device_op_and_a_broadcast_op_keep_their_order_and_shape() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let write = WritePatternBuffer::new(PatternBank::B0, 0, &phases, &intensities);
        let frames = build(2, (write, CONFIG)).unwrap();

        assert_eq!(frames.len(), 2);
        let per_device = frames.frame(0).unwrap();
        assert_eq!(per_device.distribution(), Distribution::PerDevice);
        assert_eq!(per_device.datagrams().len(), 2);
        let broadcast = frames.frame(1).unwrap();
        assert_eq!(broadcast.distribution(), Distribution::Broadcast);
        assert_eq!(broadcast.datagrams().len(), 1);
        assert_eq!(broadcast.datagrams()[0].cmd, Cmd::ConfigPattern);
    }

    #[test]
    fn a_tuple_expands_its_commands_in_order() {
        let frames = build(1, (Marker(3), CONFIG, Marker(5), Marker(7))).unwrap();

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
        let result = frames.encode_into(&test_geometry(2), (Marker(1), FailAt(1)));

        assert!(matches!(result, Err(Error::InvalidPayload(_))));
        assert!(frames.is_empty());
        assert_eq!(frames.iter().count(), 0);
    }

    #[test]
    fn encode_into_replaces_the_previous_frames() {
        let geometry = test_geometry(1);
        let mut frames = Frames::default();
        frames
            .encode_into(&geometry, (Marker(1), Marker(2)))
            .unwrap();
        frames.encode_into(&geometry, Marker(9)).unwrap();

        assert_eq!(frames.len(), 1);
        assert_eq!(payload(&frames, 0, 0)[0], 9);
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
