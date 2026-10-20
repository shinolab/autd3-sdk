use std::rc::Rc;

use crate::commands::Command;
use crate::commands::operation::{Distribution, Encoded, Nop, Operation};
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::PAYLOAD_BYTES;

use super::expansion::Expansion;

pub(crate) type OpsPerDevice<'a> = Vec<Vec<Box<dyn Operation + 'a>>>;

pub struct Each<F>(F);

#[must_use]
pub fn each<'a, C, F>(assign: F) -> Each<F>
where
    C: Command<'a>,
    F: FnMut(&Device) -> Option<C>,
{
    Each(assign)
}

impl<'a, C, F> Command<'a> for Each<F>
where
    C: Command<'a>,
    F: FnMut(&Device) -> Option<C>,
{
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
        expansion.push_per_device(self.0)?;
        Ok(())
    }
}

pub(crate) struct PerDeviceOpsAt<'a> {
    ops_per_device: Rc<OpsPerDevice<'a>>,
    op_index: usize,
}

impl<'a> PerDeviceOpsAt<'a> {
    pub(crate) fn transpose(ops_per_device: OpsPerDevice<'a>) -> impl Iterator<Item = Self> {
        let max_ops_per_device = ops_per_device.iter().map(Vec::len).max().unwrap_or(0);
        let ops_per_device = Rc::new(ops_per_device);
        (0..max_ops_per_device).map(move |op_index| Self {
            ops_per_device: Rc::clone(&ops_per_device),
            op_index,
        })
    }

    fn op(&self, device: usize) -> Option<&(dyn Operation + 'a)> {
        self.ops_per_device
            .get(device)
            .and_then(|ops| ops.get(self.op_index))
            .map(Box::as_ref)
    }
}

impl crate::sealed::Sealed for PerDeviceOpsAt<'_> {}

impl Operation for PerDeviceOpsAt<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        match self.op(device.idx()) {
            Some(op) => op.encode(device, out),
            None => Nop.encode(device, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::operation::{ConfigModulation, Distribution, Operation};
    use crate::commands::{Command, Pattern, WriteModulationBuffer};
    use crate::datagram::{Expansion, Frames, each};
    use crate::error::{Error, PayloadError};
    use crate::geometry::Autd3;
    use crate::protocol::{Cmd, PAYLOAD_BYTES};
    use crate::test_utils::{FailAt, Marker, Multi, build, cmd_at, payload, test_geometry};
    use crate::value::{
        Intensity, LoopBehavior, ModulationBank, Phase, SamplingConfig, TransitionMode,
    };

    #[test]
    fn each_routes_per_device() {
        let frames = build(
            2,
            each(|device| {
                Some(ConfigModulation {
                    bank: if device.idx() == 0 {
                        ModulationBank::B0
                    } else {
                        ModulationBank::B1
                    },
                    config: SamplingConfig::FREQ_40K,
                    size: 2,
                    loop_behavior: LoopBehavior::Infinite,
                })
            }),
        )
        .unwrap();

        assert_eq!(frames.len(), 1);
        let frame = frames.frame(0).unwrap();
        assert_eq!(frame.distribution(), Distribution::PerDevice);
        assert_eq!(payload(&frames, 0, 0)[0], 0, "device 0 -> bank B0");
        assert_eq!(payload(&frames, 0, 1)[0], 1, "device 1 -> bank B1");
    }

    #[test]
    fn each_fills_unassigned_with_nop() {
        let frames = build(2, each(|device| (device.idx() == 0).then_some(Marker(0)))).unwrap();

        assert_eq!(cmd_at(&frames, 0, 0), Cmd::ConfigModulation);
        assert_eq!(cmd_at(&frames, 0, 1), Cmd::Nop, "unassigned -> Nop");
    }

    #[test]
    fn each_pads_shorter_device_with_nop() {
        let frames = build(
            2,
            each(|device| {
                Some(if device.idx() == 0 {
                    Multi(1)
                } else {
                    Multi(3)
                })
            }),
        )
        .unwrap();

        assert_eq!(frames.len(), 3, "frame count = max over devices");
        assert_eq!(cmd_at(&frames, 0, 0), Cmd::ConfigModulation);
        assert_eq!(cmd_at(&frames, 1, 0), Cmd::Nop);
        assert_eq!(cmd_at(&frames, 2, 0), Cmd::Nop);
        for frame in 0..3 {
            assert_eq!(cmd_at(&frames, frame, 1), Cmd::ConfigModulation);
            assert_eq!(payload(&frames, frame, 1)[0] as usize, frame);
        }
    }

    #[test]
    fn nested_each_keeps_every_frame() {
        struct Nested;

        impl<'a> Command<'a> for Nested {
            fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
                expansion.push(each(|device| (device.idx() == 1).then_some(Multi(2))))?;
                Ok(())
            }
        }

        let frames = build(2, each(|device| (device.idx() == 1).then_some(Nested))).unwrap();

        assert_eq!(frames.len(), 2, "inner frames must not collapse");
        for frame in 0..2 {
            assert_eq!(cmd_at(&frames, frame, 0), Cmd::Nop);
            assert_eq!(cmd_at(&frames, frame, 1), Cmd::ConfigModulation);
            assert_eq!(payload(&frames, frame, 1)[0] as usize, frame);
        }
    }

    #[test]
    fn each_propagates_rejection_from_the_per_device_command() {
        let result = build(
            2,
            each(|device| {
                (device.idx() == 1).then_some(WriteModulationBuffer {
                    bank: ModulationBank::B0,
                    offset: 0,
                    data: &[],
                })
            }),
        );

        assert!(matches!(result, Err(Error::InvalidPayload(_))));
    }

    #[test]
    fn each_accepts_heterogeneous_boxed_commands() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let frames = build(
            2,
            each(|device| {
                Some(if device.idx() == 0 {
                    Pattern::new(&phases, &intensities).boxed()
                } else {
                    Marker(0).boxed()
                })
            }),
        )
        .unwrap();

        assert_eq!(frames.len(), 1, "the pattern is a single fused frame");
        assert_eq!(cmd_at(&frames, 0, 0), Cmd::WritePatternFused);
        assert_eq!(cmd_at(&frames, 0, 1), Cmd::ConfigModulation);
    }

    #[test]
    fn consecutive_each_stay_sequential_even_when_their_devices_are_disjoint() {
        let frames = build(
            2,
            (
                each(|device| (device.idx() == 0).then_some(Marker(0))),
                each(|device| (device.idx() == 1).then_some(Marker(1))),
            ),
        )
        .unwrap();

        assert_eq!(frames.len(), 2);
        assert_eq!(payload(&frames, 0, 0)[0], 0);
        assert_eq!(cmd_at(&frames, 0, 1), Cmd::Nop);
        assert_eq!(cmd_at(&frames, 1, 0), Cmd::Nop);
        assert_eq!(payload(&frames, 1, 1)[0], 1);
    }

    #[test]
    fn a_failing_device_inside_each_leaves_no_frame_behind() {
        let mut frames = Frames::default();
        let result = frames.encode_into(&test_geometry(2), (Marker(1), each(|_| Some(FailAt(1)))));

        assert!(matches!(result, Err(Error::InvalidPayload(_))));
        assert!(frames.is_empty());
    }

    #[test]
    fn each_on_an_empty_geometry_is_rejected_like_a_plain_command() {
        let expected = PayloadError::DeviceCountOutOfRange {
            got: 0,
            max: crate::client::MAX_DEVICES,
        };

        let plain = build(0, Marker(0));
        assert!(matches!(plain, Err(Error::InvalidPayload(e)) if e == expected));

        let per_device = build(0, (Marker(0), each(|_| Some(Marker(0)))));
        assert!(matches!(per_device, Err(Error::InvalidPayload(e)) if e == expected));
    }

    #[test]
    fn each_frames_are_the_per_device_encodings_padded_with_untouched_nops() {
        let geometry = test_geometry(3);
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 3];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 3];
        let config = ConfigModulation {
            bank: ModulationBank::B1,
            config: SamplingConfig::FREQ_40K,
            size: 2,
            loop_behavior: LoopBehavior::Infinite,
        };
        let staged = || Pattern {
            transition_mode: TransitionMode::Later,
            ..Pattern::new(&phases, &intensities)
        };

        let frames = Frames::encode(
            &geometry,
            each(|device| match device.idx() {
                0 => Some(staged().boxed()),
                1 => Some(config.boxed()),
                _ => None,
            }),
        )
        .unwrap();

        let reference = build(3, staged()).unwrap();

        let mut config_payload = [0u8; PAYLOAD_BYTES];
        let config_encoded = config.encode(&geometry[1], &mut config_payload).unwrap();

        assert_eq!(frames.len(), 2);
        assert_eq!(frames.len(), reference.len());
        for (index, frame) in frames.iter().enumerate() {
            assert_eq!(frame.distribution(), Distribution::PerDevice);
            let datagrams = frame.datagrams();
            assert_eq!(datagrams.len(), 3);

            let expected = &reference.frame(index).unwrap().datagrams()[0];
            assert_eq!(datagrams[0].cmd, expected.cmd);
            assert_eq!(datagrams[0].payload_len, expected.payload_len);
            assert_eq!(datagrams[0].payload, expected.payload);

            if index == 0 {
                assert_eq!(datagrams[1].cmd, config_encoded.cmd);
                assert_eq!(datagrams[1].payload_len, config_encoded.len);
                assert_eq!(datagrams[1].payload, config_payload);
            } else {
                assert_eq!(datagrams[1].cmd, Cmd::Nop);
                assert_eq!(datagrams[1].payload_len, 0);
                assert_eq!(datagrams[1].payload, [0u8; PAYLOAD_BYTES]);
            }

            assert_eq!(datagrams[2].cmd, Cmd::Nop);
            assert_eq!(datagrams[2].payload_len, 0);
            assert_eq!(datagrams[2].payload, [0u8; PAYLOAD_BYTES]);
        }
    }

    #[test]
    fn re_encoding_each_reuses_the_buffer_without_growing() {
        let geometry = test_geometry(2);
        let per_device = || {
            each(|device| {
                Some(if device.idx() == 0 {
                    Multi(1)
                } else {
                    Multi(3)
                })
            })
        };

        let mut frames = Frames::default();
        frames.encode_into(&geometry, per_device()).unwrap();
        let capacity = frames.datagram_capacity();
        frames.encode_into(&geometry, per_device()).unwrap();

        assert_eq!(frames.len(), 3);
        assert_eq!(frames.datagram_capacity(), capacity);
    }
}
