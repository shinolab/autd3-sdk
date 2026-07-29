use autd3_rs_core::geometry::Device;
use autd3_rs_core::value::{LoopBehavior, SamplingConfig};
use zerocopy::{Immutable, IntoBytes};

use super::LegacyOperation;
use crate::legacy::error::{LegacyError, PayloadError};
use crate::legacy::wire::params::{
    MOD_BUF_SIZE_MAX, MOD_BUF_SIZE_MIN, MOD_HEAD_SIZE_MAX, MODULATION_FLAG_BEGIN,
    MODULATION_FLAG_END, MODULATION_FLAG_SEGMENT, MODULATION_FLAG_TRANSITION,
};
use crate::legacy::wire::{Segment, Tag, TransitionMode};

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct ModulationHead {
    tag: u8,
    flag: u8,
    size: u8,
    transition_mode: u8,
    freq_div: u16,
    rep: u16,
    transition_value: u64,
}

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct ModulationSubseq {
    tag: u8,
    flag: u8,
    size: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModulationOption {
    pub segment: Segment,
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
}

impl Default for ModulationOption {
    fn default() -> Self {
        Self {
            segment: Segment::S0,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modulation<'a> {
    buffer: &'a [u8],
    config: SamplingConfig,
    option: ModulationOption,
    sent: usize,
    done: bool,
}

impl<'a> Modulation<'a> {
    #[must_use]
    pub fn new(
        config: impl Into<SamplingConfig>,
        buffer: &'a [u8],
        option: ModulationOption,
    ) -> Self {
        Self {
            buffer,
            config: config.into(),
            option,
            sent: 0,
            done: false,
        }
    }
}

const fn align_up(n: usize) -> usize {
    (n + 1) & !0x1
}

impl LegacyOperation for Modulation<'_> {
    fn required_size(&self, _device: &Device) -> usize {
        if self.sent == 0 {
            size_of::<ModulationHead>() + 2
        } else {
            size_of::<ModulationSubseq>() + 2
        }
    }

    fn pack(&mut self, _device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        let is_first = self.sent == 0;
        let offset = if is_first {
            size_of::<ModulationHead>()
        } else {
            size_of::<ModulationSubseq>()
        };
        let capacity = if is_first {
            (tx.len() - offset).min(MOD_HEAD_SIZE_MAX)
        } else {
            tx.len() - offset
        };
        let send_num = (self.buffer.len() - self.sent).min(capacity);

        tx[offset..offset + send_num]
            .copy_from_slice(&self.buffer[self.sent..self.sent + send_num]);
        self.sent += send_num;

        if self.sent > MOD_BUF_SIZE_MAX {
            return Err(PayloadError::ModulationSizeOutOfRange {
                size: self.buffer.len(),
                min: MOD_BUF_SIZE_MIN,
                max: MOD_BUF_SIZE_MAX,
            }
            .into());
        }

        let mut flag = if self.option.segment == Segment::S1 {
            MODULATION_FLAG_SEGMENT
        } else {
            0
        };
        if self.buffer.len() == self.sent {
            if self.sent < MOD_BUF_SIZE_MIN {
                return Err(PayloadError::ModulationSizeOutOfRange {
                    size: self.buffer.len(),
                    min: MOD_BUF_SIZE_MIN,
                    max: MOD_BUF_SIZE_MAX,
                }
                .into());
            }
            self.done = true;
            flag |= MODULATION_FLAG_END;
            if !self.option.transition_mode.is_later() {
                flag |= MODULATION_FLAG_TRANSITION;
            }
        }

        if is_first {
            let head = ModulationHead {
                tag: Tag::Modulation.as_u8(),
                flag: flag | MODULATION_FLAG_BEGIN,
                size: u8::try_from(send_num).expect("the head chunk is capped at 254 bytes"),
                transition_mode: self.option.transition_mode.as_u8(),
                freq_div: self.config.divide()?,
                rep: self.option.loop_behavior.rep(),
                transition_value: self.option.transition_mode.value(),
            };
            tx[..size_of::<ModulationHead>()].copy_from_slice(head.as_bytes());
            Ok(size_of::<ModulationHead>() + align_up(send_num))
        } else {
            let subseq = ModulationSubseq {
                tag: Tag::Modulation.as_u8(),
                flag,
                size: u16::try_from(send_num).expect("a payload chunk always fits in u16"),
            };
            tx[..size_of::<ModulationSubseq>()].copy_from_slice(subseq.as_bytes());
            Ok(size_of::<ModulationSubseq>() + align_up(send_num))
        }
    }

    fn is_done(&self) -> bool {
        self.done
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU16;

    use autd3_rs_core::geometry::{Autd3, Geometry};
    use autd3_rs_core::value::DcSysTime;

    use super::*;
    use crate::legacy::op::test_frames;
    use crate::legacy::wire::PAYLOAD_BYTES;

    fn geometry(n: usize) -> Geometry {
        Geometry::new((0..n).map(|_| Autd3::default()).collect())
    }

    fn config() -> SamplingConfig {
        SamplingConfig::new(NonZeroU16::new(10).unwrap())
    }

    #[test]
    fn single_frame_modulation_is_begin_end_and_transition() {
        let geo = geometry(1);
        let buffer = (0..100u8).collect::<Vec<_>>();
        let mut op = Modulation::new(config(), &buffer, ModulationOption::default());
        assert_eq!(op.required_size(&geo[0]), 18);

        let mut tx = vec![0u8; PAYLOAD_BYTES];
        let size = op.pack(&geo[0], &mut tx).unwrap();
        assert!(op.is_done());
        assert_eq!(size, 16 + 100);

        assert_eq!(tx[0], Tag::Modulation.as_u8());
        assert_eq!(
            tx[1],
            MODULATION_FLAG_BEGIN | MODULATION_FLAG_END | MODULATION_FLAG_TRANSITION
        );
        assert_eq!(tx[2], 100);
        assert_eq!(tx[3], TransitionMode::Immediate.as_u8());
        assert_eq!(&tx[4..6], &10u16.to_le_bytes());
        assert_eq!(&tx[6..8], &0xFFFFu16.to_le_bytes());
        assert_eq!(&tx[8..16], &0u64.to_le_bytes());
        assert_eq!(&tx[16..116], &buffer[..]);
    }

    #[test]
    fn segment_1_and_later_transition_set_the_matching_flags() {
        let geo = geometry(1);
        let buffer = vec![0x80u8; 4];
        let mut op = Modulation::new(
            config(),
            &buffer,
            ModulationOption {
                segment: Segment::S1,
                loop_behavior: LoopBehavior::ONCE,
                transition_mode: TransitionMode::Later,
            },
        );
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        op.pack(&geo[0], &mut tx).unwrap();
        assert_eq!(
            tx[1],
            MODULATION_FLAG_BEGIN | MODULATION_FLAG_END | MODULATION_FLAG_SEGMENT
        );
        assert_eq!(tx[3], TransitionMode::Later.as_u8());
        assert_eq!(&tx[6..8], &0u16.to_le_bytes());
    }

    #[test]
    fn sys_time_transition_value_is_carried_in_the_head() {
        let geo = geometry(1);
        let buffer = vec![0u8; 2];
        let time = DcSysTime::from_nanos(0x0123_4567_89AB_CDEF);
        let mut op = Modulation::new(
            config(),
            &buffer,
            ModulationOption {
                transition_mode: TransitionMode::SysTime(time),
                ..ModulationOption::default()
            },
        );
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        op.pack(&geo[0], &mut tx).unwrap();
        assert_eq!(tx[3], TransitionMode::SysTime(time).as_u8());
        assert_eq!(&tx[8..16], &time.sys_time().to_le_bytes());
    }

    #[test]
    fn head_chunk_is_capped_at_254_bytes() {
        let geo = geometry(1);
        let buffer = vec![0x55u8; 600];
        let mut op = Modulation::new(config(), &buffer, ModulationOption::default());
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        let size = op.pack(&geo[0], &mut tx).unwrap();
        assert!(!op.is_done());
        assert_eq!(tx[2], 254);
        assert_eq!(size, 16 + 254);
    }

    #[test]
    fn multi_frame_split_covers_the_whole_buffer_exactly_once() {
        let geo = geometry(1);
        let buffer = (0..2000u32).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let frames = test_frames(
            &geo,
            Modulation::new(config(), &buffer, ModulationOption::default()),
        )
        .unwrap();

        assert!(frames.len() > 1);
        let mut restored = Vec::new();
        for round in 0..frames.len() {
            let frame = frames.frame(round).unwrap();
            let payload = &frame.frames()[0].payload;
            assert_eq!(payload[0], Tag::Modulation.as_u8());
            let (offset, size) = if round == 0 {
                assert_eq!(payload[1] & MODULATION_FLAG_BEGIN, MODULATION_FLAG_BEGIN);
                (16, usize::from(payload[2]))
            } else {
                assert_eq!(payload[1] & MODULATION_FLAG_BEGIN, 0);
                (4, usize::from(u16::from_le_bytes([payload[2], payload[3]])))
            };
            restored.extend_from_slice(&payload[offset..offset + size]);
            let is_last = round == frames.len() - 1;
            assert_eq!(
                payload[1] & MODULATION_FLAG_END != 0,
                is_last,
                "only the last frame carries END"
            );
        }
        assert_eq!(restored, buffer);
    }

    #[test]
    fn odd_chunks_round_the_reported_size_up_to_a_word() {
        let geo = geometry(1);
        let buffer = vec![0u8; 3];
        let mut op = Modulation::new(config(), &buffer, ModulationOption::default());
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        assert_eq!(op.pack(&geo[0], &mut tx).unwrap(), 16 + 4);
    }

    #[test]
    fn buffers_shorter_than_two_samples_are_rejected() {
        let geo = geometry(1);
        for buffer in [vec![], vec![0u8; 1]] {
            let mut op = Modulation::new(config(), &buffer, ModulationOption::default());
            let mut tx = vec![0u8; PAYLOAD_BYTES];
            let err = op.pack(&geo[0], &mut tx).unwrap_err();
            assert!(matches!(
                err,
                LegacyError::InvalidPayload(PayloadError::ModulationSizeOutOfRange { .. })
            ));
        }
    }
}
