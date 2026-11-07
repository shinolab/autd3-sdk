use crate::commands::Command;
use crate::error::Error;
use crate::geometry::Device;

use super::expansion::Expansion;

pub struct Each<F>(F);

#[must_use]
pub fn each<C, F>(assign: F) -> Each<F>
where
    C: Command,
    F: FnMut(&Device) -> Option<C>,
{
    Each(assign)
}

impl<C, F> Command for Each<F>
where
    C: Command,
    F: FnMut(&Device) -> Option<C>,
{
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        expansion.push_each(self.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::operation::{
        ConfigModulation, Distribution, WriteFociBuffer, WriteModulationBuffer, WritePatternBuffer,
    };
    use crate::commands::{
        Command, FociStm, FociStmOption, Modulation, Operation, Pattern, SetSilencer,
    };
    use crate::datagram::{Datagram, Frames, each};
    use crate::error::Error;
    use crate::geometry::{Autd3, Point3};
    use crate::protocol::{Cmd, PAYLOAD_BYTES};
    use crate::test_utils::{FailAt, Marker, build, cmd_at, payload, test_geometry, waits};
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
        assert_eq!(waits(&frames), [false]);
    }

    #[test]
    fn each_waits_when_any_assigned_operation_waits() {
        let config = ConfigModulation {
            bank: ModulationBank::B0,
            config: SamplingConfig::FREQ_40K,
            size: 2,
            loop_behavior: LoopBehavior::Infinite,
        };
        let frames = build(
            3,
            each(|device| match device.idx() {
                0 => Some(Box::new(config) as Box<dyn Operation>),
                1 => Some(Box::new(Marker(0))),
                _ => None,
            }),
        )
        .unwrap();

        assert_eq!(waits(&frames), [true]);
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
        let frames = build(2, each(|_| None::<Marker>)).unwrap();

        assert!(frames.is_empty());
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
        let modulation = build(
            2,
            each(|device| {
                Some(WriteModulationBuffer {
                    bank: ModulationBank::B0,
                    offset: 0,
                    data: if device.idx() == 0 { &low } else { &high },
                })
            }),
        )
        .unwrap();
        let foci = build(
            2,
            each(|device| {
                (device.idx() == 1).then_some(WriteFociBuffer {
                    bank: PatternBank::B0,
                    start_index: 0,
                    points: &points,
                })
            }),
        )
        .unwrap();

        assert_eq!(modulation.len(), 1);
        assert_eq!(&payload(&modulation, 0, 0)[6..], &low);
        assert_eq!(&payload(&modulation, 0, 1)[6..], &high);
        assert_eq!(foci.len(), 1);
        assert_eq!(cmd_at(&foci, 0, 0), Cmd::Nop);
        assert_eq!(cmd_at(&foci, 0, 1), Cmd::WriteFociBuffer);
    }

    #[test]
    fn each_lines_up_the_frames_of_multi_frame_commands_and_pads_the_shorter_with_nop() {
        let data = [0x5Au8; 4];
        let modulation = || Modulation::new(SamplingConfig::FREQ_4K, &data);
        let alone = build(3, modulation()).unwrap();
        let frames = build(
            3,
            each(|device| match device.idx() {
                0 => Some(Box::new(SetSilencer::default()) as Box<dyn Operation>),
                1 => Some(Box::new(Marker(1))),
                _ => None,
            }),
        )
        .unwrap();
        assert_eq!(frames.len(), 1);

        let frames = build(
            3,
            each(|device| match device.idx() {
                0 => Some(modulation().boxed()),
                1 => Some(SetSilencer::default().boxed()),
                _ => None,
            }),
        )
        .unwrap();

        assert_eq!(frames.len(), alone.len());
        for (i, frame) in frames.iter().enumerate() {
            assert_eq!(frame.distribution(), Distribution::PerDevice);
            assert_eq!(cmd_at(&frames, i, 0), cmd_at(&alone, i, 0));
            assert_eq!(payload(&frames, i, 0), payload(&alone, i, 0));
            let expected = if i == 0 { Cmd::SetSilencer } else { Cmd::Nop };
            assert_eq!(cmd_at(&frames, i, 1), expected);
            assert_eq!(cmd_at(&frames, i, 2), Cmd::Nop);
            assert_eq!(frame.datagrams()[2].payload, [0u8; PAYLOAD_BYTES]);
        }
        let mut expected_waits = waits(&alone);
        expected_waits[0] |= SetSilencer::default().waits_for_previous();
        assert_eq!(waits(&frames), expected_waits);
    }

    #[test]
    fn a_per_device_stm_keeps_the_encoding_of_its_own_device() {
        let points: Vec<_> = (0..4)
            .map(|i| ControlPoints::from(Point3::new(i as f32, 0.0, 150.0)))
            .collect();
        let stm = || FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default());
        let geometry = test_geometry(3);
        let alone = Frames::encode(&geometry, stm()).unwrap();
        let frames =
            Frames::encode(&geometry, each(|device| (device.idx() != 1).then(stm))).unwrap();

        assert_eq!(frames.len(), alone.len());
        for (i, frame) in alone.iter().enumerate() {
            for device in [0, 2] {
                assert_eq!(cmd_at(&frames, i, device), frame.datagram_for(device).cmd);
                assert_eq!(
                    payload(&frames, i, device),
                    frame.datagram_for(device).payload()
                );
            }
            assert_eq!(cmd_at(&frames, i, 1), Cmd::Nop);
        }
    }

    #[test]
    fn commands_after_an_each_start_after_its_longest_device() {
        let data = [0u8; 4];
        let frames = build(
            2,
            (
                Marker(9),
                each(|device| {
                    (device.idx() == 1).then(|| Modulation::new(SamplingConfig::FREQ_4K, &data))
                }),
                Marker(8),
            ),
        )
        .unwrap();

        let modulation_frames = build(1, Modulation::new(SamplingConfig::FREQ_4K, &data))
            .unwrap()
            .len();
        assert_eq!(frames.len(), modulation_frames + 2);
        assert_eq!(payload(&frames, 0, 0)[0], 9);
        assert_eq!(payload(&frames, modulation_frames + 1, 0)[0], 8);
        assert_eq!(cmd_at(&frames, 1, 0), Cmd::Nop);
    }

    #[test]
    fn a_nested_each_is_asked_only_for_the_device_it_runs_under() {
        let asked = std::sync::Mutex::new(Vec::new());
        let frames = build(
            3,
            each(|outer| {
                let outer = outer.idx();
                (outer != 0).then(|| {
                    let asked = &asked;
                    each(move |inner| {
                        asked.lock().unwrap().push((outer, inner.idx()));
                        Some(Marker(u8::try_from(inner.idx()).unwrap()))
                    })
                    .boxed()
                })
            }),
        )
        .unwrap();

        assert_eq!(*asked.lock().unwrap(), [(1, 1), (2, 2)]);
        assert_eq!(frames.len(), 1);
        assert_eq!(cmd_at(&frames, 0, 0), Cmd::Nop);
        assert_eq!(payload(&frames, 0, 2)[0], 2);
    }

    #[test]
    fn a_failing_device_inside_each_leaves_no_frame_behind() {
        let mut frames = Frames::default();
        let result = frames.encode_into(&test_geometry(2), each(|_| Some(FailAt(1))));

        assert!(matches!(result, Err(Error::InvalidPayload(_))));
        assert!(frames.is_empty());
    }

    #[test]
    fn each_frame_holds_the_per_device_encodings_and_untouched_nops() {
        let geometry = test_geometry(3);
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 3];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 3];
        let pattern = WritePatternBuffer::new(
            PatternBank::B0,
            0,
            core::slice::from_ref(&phases),
            &intensities,
        );
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
