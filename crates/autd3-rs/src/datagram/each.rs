use crate::commands::Command;
use crate::commands::operation::Operation;
use crate::error::Error;
use crate::geometry::Device;

use super::expansion::Expansion;

pub struct Each<F>(F);

#[must_use]
pub fn each<O, F>(assign: F) -> Each<F>
where
    O: Operation,
    F: FnMut(&Device) -> Option<O>,
{
    Each(assign)
}

impl<O, F> Command for Each<F>
where
    O: Operation,
    F: FnMut(&Device) -> Option<O>,
{
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        expansion.push_per_device(self.0)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::operation::{
        ConfigModulation, Distribution, Operation, WriteFociBuffer, WriteModulationBuffer,
        WritePatternBuffer,
    };
    use crate::commands::{Pattern, SetSilencer};
    use crate::datagram::{Datagram, Frames, each};
    use crate::error::{Error, PayloadError};
    use crate::geometry::{Autd3, Point3};
    use crate::protocol::{Cmd, PAYLOAD_BYTES};
    use crate::test_utils::{FailAt, Marker, build, cmd_at, payload, test_geometry};
    use crate::value::{
        ControlPoints, Intensity, LoopBehavior, ModulationBank, PatternBank, Phase, SamplingConfig,
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

        assert_eq!(frames.len(), 1);
        assert_eq!(cmd_at(&frames, 0, 0), Cmd::ConfigModulation);
        assert_eq!(cmd_at(&frames, 0, 1), Cmd::Nop, "unassigned -> Nop");
    }

    #[test]
    fn each_without_any_assignment_emits_no_frame() {
        let frames = build(2, (Marker(0), each(|_| None::<Marker>))).unwrap();

        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn each_propagates_rejection_from_the_per_device_operation() {
        let result = build(2, each(|device| (device.idx() == 1).then_some(FailAt(1))));

        assert!(matches!(result, Err(Error::InvalidPayload(_))));
    }

    #[test]
    fn each_accepts_heterogeneous_boxed_operations() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let frames = build(
            2,
            each(|device| {
                Some(if device.idx() == 0 {
                    Box::new(Pattern::new(&phases, &intensities)) as Box<dyn Operation>
                } else {
                    Box::new(SetSilencer::default())
                })
            }),
        )
        .unwrap();

        assert_eq!(frames.len(), 1);
        assert_eq!(cmd_at(&frames, 0, 0), Cmd::WritePatternFused);
        assert_eq!(cmd_at(&frames, 0, 1), Cmd::SetSilencer);
    }

    #[test]
    fn each_takes_the_buffer_writes_as_single_frame_operations() {
        let low = [0x11u8; 4];
        let high = [0x22u8; 4];
        let points = [ControlPoints::from(Point3::new(0.0, 0.0, 150.0))];
        let frames = build(
            2,
            (
                each(|device| {
                    Some(WriteModulationBuffer {
                        bank: ModulationBank::B0,
                        offset: 0,
                        data: if device.idx() == 0 { &low } else { &high },
                    })
                }),
                each(|device| {
                    (device.idx() == 1).then_some(WriteFociBuffer {
                        bank: PatternBank::B0,
                        start_index: 0,
                        points: &points,
                    })
                }),
            ),
        )
        .unwrap();

        assert_eq!(frames.len(), 2);
        assert_eq!(&payload(&frames, 0, 0)[6..], &low);
        assert_eq!(&payload(&frames, 0, 1)[6..], &high);
        assert_eq!(cmd_at(&frames, 1, 0), Cmd::Nop);
        assert_eq!(cmd_at(&frames, 1, 1), Cmd::WriteFociBuffer);
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
            max: autd3_rs_core::MAX_DEVICES,
        };

        let plain = build(0, Marker(0));
        assert!(matches!(plain, Err(Error::InvalidPayload(e)) if e == expected));

        let per_device = build(0, (Marker(0), each(|_| Some(Marker(0)))));
        assert!(matches!(per_device, Err(Error::InvalidPayload(e)) if e == expected));
    }

    #[test]
    fn each_frame_holds_the_per_device_encodings_and_untouched_nops() {
        let geometry = test_geometry(3);
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 3];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 3];
        let pattern = WritePatternBuffer::new(PatternBank::B0, 0, &phases, &intensities);
        let config = ConfigModulation {
            bank: ModulationBank::B1,
            config: SamplingConfig::FREQ_40K,
            size: 2,
            loop_behavior: LoopBehavior::Infinite,
        };

        let frames = Frames::encode(
            &geometry,
            each(|device| match device.idx() {
                0 => Some(Box::new(pattern) as Box<dyn Operation>),
                1 => Some(Box::new(config)),
                _ => None,
            }),
        )
        .unwrap();

        let mut pattern_datagram = Datagram::no_payload(Cmd::Nop);
        pattern.encode(&geometry[0], &mut pattern_datagram).unwrap();
        let mut config_datagram = Datagram::no_payload(Cmd::Nop);
        config.encode(&geometry[1], &mut config_datagram).unwrap();

        assert_eq!(frames.len(), 1);
        let frame = frames.frame(0).unwrap();
        assert_eq!(frame.distribution(), Distribution::PerDevice);
        let datagrams = frame.datagrams();
        assert_eq!(datagrams.len(), 3);

        assert_eq!(datagrams[0].cmd, pattern_datagram.cmd);
        assert_eq!(datagrams[0].payload(), pattern_datagram.payload());

        assert_eq!(datagrams[1].cmd, config_datagram.cmd);
        assert_eq!(datagrams[1].payload(), config_datagram.payload());

        assert_eq!(datagrams[2].cmd, Cmd::Nop);
        assert_eq!(datagrams[2].payload_len, 0);
        assert_eq!(datagrams[2].payload, [0u8; PAYLOAD_BYTES]);
    }

    #[test]
    fn re_encoding_each_reuses_the_buffer_without_growing() {
        let geometry = test_geometry(2);
        let per_device = || each(|device| (device.idx() == 1).then_some(Marker(1)));

        let mut frames = Frames::default();
        frames.encode_into(&geometry, per_device()).unwrap();
        let capacity = frames.datagram_capacity();
        frames.encode_into(&geometry, per_device()).unwrap();

        assert_eq!(frames.len(), 1);
        assert_eq!(frames.datagram_capacity(), capacity);
    }
}
