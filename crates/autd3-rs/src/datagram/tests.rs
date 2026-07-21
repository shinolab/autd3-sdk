use super::*;
use crate::commands::operation::{
    ConfigModulation, ConfigPattern, Distribution, Operation, WritePatternBuffer,
};
use crate::commands::{Command, Pattern};
use crate::error::Error;
use crate::geometry::Autd3;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Emission, LoopBehavior, ModulationBank, PatternBank, SamplingConfig};

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

fn cmd_at(frames: &Frames, frame: usize, device: usize) -> Cmd {
    frames.frame(frame).unwrap().datagrams()[device].cmd
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
            config: SamplingConfig::FREQ_40K,
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        })
    });
    let frames = b.build().unwrap();

    assert_eq!(frames.len(), 1);
    let frame = frames.frame(0).unwrap();
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
            config: SamplingConfig::FREQ_40K,
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        })
    });
    let frames = b.build().unwrap();

    assert_eq!(cmd_at(&frames, 0, 0), Cmd::ConfigModulation);
    assert_eq!(cmd_at(&frames, 0, 1), Cmd::Nop, "unassigned -> Nop");
}

#[test]
fn push_each_pads_shorter_device_with_nop() {
    let mut b = DatagramBuilder::new(2);
    b.push_each(|device| Some(if device == 0 { Multi(1) } else { Multi(3) }));
    let frames = b.build().unwrap();

    assert_eq!(frames.len(), 3, "frame count = max over devices");
    assert_eq!(cmd_at(&frames, 0, 0), Cmd::ConfigModulation);
    assert_eq!(cmd_at(&frames, 1, 0), Cmd::Nop);
    assert_eq!(cmd_at(&frames, 2, 0), Cmd::Nop);
    for frame in 0..3 {
        assert_eq!(cmd_at(&frames, frame, 1), Cmd::ConfigModulation);
        assert_eq!(
            frames.frame(frame).unwrap().datagrams()[1].payload[0] as usize,
            frame
        );
    }
}

#[test]
fn push_each_accepts_heterogeneous_boxed_commands() {
    let patterns = vec![vec![crate::value::Emission::default(); Autd3::NUM_TRANSDUCERS]; 2];
    let mut b = DatagramBuilder::new(2);
    b.push_each(|device| {
        Some(if device == 0 {
            Pattern::new(&patterns).boxed()
        } else {
            ConfigModulation {
                bank: ModulationBank::B0,
                config: SamplingConfig::FREQ_40K,
                size: 1,
                loop_behavior: LoopBehavior::Infinite,
            }
            .boxed()
        })
    });
    let frames = b.build().unwrap();

    assert_eq!(frames.len(), 1, "both commands are single-frame");
    assert_eq!(cmd_at(&frames, 0, 0), Cmd::WritePatternFused);
    assert_eq!(cmd_at(&frames, 0, 1), Cmd::ConfigModulation);
}

#[test]
fn adjacent_disjoint_push_each_fuse_into_shared_frames() {
    let mut b = DatagramBuilder::new(2);
    b.push_each(|device| {
        (device == 0).then_some(ConfigModulation {
            bank: ModulationBank::B0,
            config: SamplingConfig::FREQ_40K,
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        })
    });
    b.push_each(|device| {
        (device == 1).then_some(ConfigModulation {
            bank: ModulationBank::B1,
            config: SamplingConfig::FREQ_40K,
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        })
    });
    let frames = b.build().unwrap();

    assert_eq!(frames.len(), 1, "disjoint groups fuse into one frame");
    let frame = frames.frame(0).unwrap();
    assert_eq!(frame.datagrams()[0].payload[0], 0, "device 0 -> B0");
    assert_eq!(frame.datagrams()[1].payload[0], 1, "device 1 -> B1");
}

#[test]
fn adjacent_overlapping_push_each_stay_sequential() {
    let mut b = DatagramBuilder::new(2);
    b.push_each(|_| {
        Some(ConfigModulation {
            bank: ModulationBank::B0,
            config: SamplingConfig::FREQ_40K,
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        })
    });
    b.push_each(|_| {
        Some(ConfigModulation {
            bank: ModulationBank::B1,
            config: SamplingConfig::FREQ_40K,
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        })
    });
    let frames = b.build().unwrap();

    assert_eq!(frames.len(), 2, "overlapping coverage stays sequential");
    assert_eq!(frames.frame(0).unwrap().datagrams()[0].payload[0], 0);
    assert_eq!(frames.frame(1).unwrap().datagrams()[0].payload[0], 1);
}

#[test]
fn broadcast_push_is_a_fuse_barrier() {
    let mut b = DatagramBuilder::new(2);
    b.push_each(|device| {
        (device == 0).then_some(ConfigModulation {
            bank: ModulationBank::B0,
            config: SamplingConfig::FREQ_40K,
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        })
    });
    b.push(ConfigPattern {
        bank: PatternBank::B0,
        config: SamplingConfig::FREQ_40K,
        size: 1,
        loop_behavior: LoopBehavior::Infinite,
    });
    b.push_each(|device| {
        (device == 1).then_some(ConfigModulation {
            bank: ModulationBank::B1,
            config: SamplingConfig::FREQ_40K,
            size: 1,
            loop_behavior: LoopBehavior::Infinite,
        })
    });
    let frames = b.build().unwrap();

    assert_eq!(frames.len(), 3, "broadcast between steps prevents fusion");
    assert_eq!(
        frames.frame(1).unwrap().distribution(),
        Distribution::Broadcast
    );
}

#[test]
fn broadcast_op_yields_one_frame_of_one_datagram() {
    let op = ConfigPattern {
        bank: PatternBank::B0,
        config: SamplingConfig::FREQ_40K,
        size: 1,
        loop_behavior: LoopBehavior::Infinite,
    };
    let mut b = DatagramBuilder::new(4);
    b.push(op);
    let frames = b.build().unwrap();

    assert_eq!(frames.len(), 1);
    let frame = frames.frame(0).unwrap();
    assert_eq!(frame.distribution(), Distribution::Broadcast);
    assert_eq!(frame.datagrams().len(), 1);
    assert_eq!(frame.datagrams()[0].cmd, Cmd::ConfigPattern);
}

#[test]
fn per_device_op_yields_one_datagram_per_device() {
    let patterns = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; 3];
    let op = WritePatternBuffer {
        bank: PatternBank::B0,
        index: 0,
        emissions: &patterns,
    };
    let mut b = DatagramBuilder::new(3);
    b.push(op);
    let frames = b.build().unwrap();

    assert_eq!(frames.len(), 1);
    let frame = frames.frame(0).unwrap();
    assert_eq!(frame.distribution(), Distribution::PerDevice);
    assert_eq!(frame.datagrams().len(), 3);
}

#[test]
fn composite_emission_orders_write_then_config() {
    let patterns = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; 2];
    let we = WritePatternBuffer {
        bank: PatternBank::B0,
        index: 0,
        emissions: &patterns,
    };
    let ce = ConfigPattern {
        bank: PatternBank::B0,
        config: SamplingConfig::FREQ_40K,
        size: 1,
        loop_behavior: LoopBehavior::Infinite,
    };
    let mut b = DatagramBuilder::new(2);
    b.push(we).push(ce);
    let frames = b.build().unwrap();

    assert_eq!(frames.len(), 2);
    assert_eq!(
        frames.frame(0).unwrap().distribution(),
        Distribution::PerDevice
    );
    assert_eq!(frames.frame(0).unwrap().datagrams().len(), 2);
    assert_eq!(
        frames.frame(1).unwrap().distribution(),
        Distribution::Broadcast
    );
    assert_eq!(
        frames.frame(1).unwrap().datagrams()[0].cmd,
        Cmd::ConfigPattern
    );
}

#[test]
fn build_into_reuses_buffer_without_growing() {
    let op = ConfigPattern {
        bank: PatternBank::B0,
        config: SamplingConfig::FREQ_40K,
        size: 1,
        loop_behavior: LoopBehavior::Infinite,
    };
    let mut b = DatagramBuilder::new(1);
    b.push(op);

    let mut buf = Frames::default();
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
