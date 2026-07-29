use autd3_rs_core::geometry::Device;
use autd3_rs_core::value::{Emission, LoopBehavior, SamplingConfig};
use zerocopy::{Immutable, IntoBytes};

use super::LegacyOperation;
use super::gain::{emissions_for, write_emissions};
use crate::legacy::error::{LegacyError, PayloadError};
use crate::legacy::wire::params::{
    GAIN_STM_BUF_SIZE_MAX, GAIN_STM_FLAG_BEGIN, GAIN_STM_FLAG_END, GAIN_STM_FLAG_SEGMENT,
    GAIN_STM_FLAG_SEND_BIT0, GAIN_STM_FLAG_SEND_BIT1, GAIN_STM_FLAG_TRANSITION, STM_BUF_SIZE_MIN,
};
use crate::legacy::wire::{GainStmMode, Segment, Tag, TransitionMode};

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct GainStmHead {
    tag: u8,
    flag: u8,
    mode: u8,
    transition_mode: u8,
    freq_div: u16,
    rep: u16,
    transition_value: u64,
}

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct GainStmSubseq {
    tag: u8,
    flag: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GainStmOption {
    pub mode: GainStmMode,
    pub segment: Segment,
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
}

impl Default for GainStmOption {
    fn default() -> Self {
        Self {
            mode: GainStmMode::PhaseIntensityFull,
            segment: Segment::S0,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GainStm<'a> {
    patterns: &'a [Vec<Vec<Emission>>],
    config: SamplingConfig,
    option: GainStmOption,
    sent: usize,
}

impl<'a> GainStm<'a> {
    #[must_use]
    pub fn new(
        config: impl Into<SamplingConfig>,
        patterns: &'a [Vec<Vec<Emission>>],
        option: GainStmOption,
    ) -> Self {
        Self {
            patterns,
            config: config.into(),
            option,
            sent: 0,
        }
    }
}

fn write_phase_nibbles(tx: &mut [u8], emissions: &[Emission], slot: usize) {
    for (dst, emission) in tx.chunks_exact_mut(2).zip(emissions) {
        let word = u16::from_le_bytes([dst[0], dst[1]])
            | (u16::from(emission.phase.0 >> 4) & 0x000F) << (4 * slot);
        dst.copy_from_slice(&word.to_le_bytes());
    }
}

fn write_phase_bytes(tx: &mut [u8], emissions: &[Emission], slot: usize) {
    for (dst, emission) in tx.chunks_exact_mut(2).zip(emissions) {
        dst[slot] = emission.phase.0;
    }
}

impl LegacyOperation for GainStm<'_> {
    fn required_size(&self, device: &Device) -> usize {
        let head = if self.sent == 0 {
            size_of::<GainStmHead>()
        } else {
            size_of::<GainStmSubseq>()
        };
        head + device.num_transducers() * size_of::<Emission>()
    }

    fn pack(&mut self, device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        let size = self.patterns.len();
        if !(STM_BUF_SIZE_MIN..=GAIN_STM_BUF_SIZE_MAX).contains(&size) {
            return Err(PayloadError::GainStmSizeOutOfRange {
                size,
                min: STM_BUF_SIZE_MIN,
                max: GAIN_STM_BUF_SIZE_MAX,
            }
            .into());
        }

        let is_first = self.sent == 0;
        let offset = if is_first {
            size_of::<GainStmHead>()
        } else {
            size_of::<GainStmSubseq>()
        };
        let body = device.num_transducers() * size_of::<Emission>();
        let words = &mut tx[offset..offset + body];
        words.fill(0);

        let take = self.option.mode.frames_per_round().min(size - self.sent);
        debug_assert!(take >= 1, "a done GainStm op must not be packed again");
        for slot in 0..take {
            let emissions = emissions_for(&self.patterns[self.sent + slot], device)?;
            match self.option.mode {
                GainStmMode::PhaseIntensityFull => write_emissions(words, emissions),
                GainStmMode::PhaseFull => write_phase_bytes(words, emissions, slot),
                GainStmMode::PhaseHalf => write_phase_nibbles(words, emissions, slot),
            }
        }
        self.sent += take;

        let mut flag = 0u8;
        if self.sent == size {
            flag |= GAIN_STM_FLAG_END;
            if !self.option.transition_mode.is_later() {
                flag |= GAIN_STM_FLAG_TRANSITION;
            }
        }
        if self.option.segment == Segment::S1 {
            flag |= GAIN_STM_FLAG_SEGMENT;
        }
        let send = u8::try_from(take).expect("at most 4 patterns per frame") - 1;
        if send & 0x01 != 0 {
            flag |= GAIN_STM_FLAG_SEND_BIT0;
        }
        if send & 0x02 != 0 {
            flag |= GAIN_STM_FLAG_SEND_BIT1;
        }

        if is_first {
            let head = GainStmHead {
                tag: Tag::GainStm.as_u8(),
                flag: flag | GAIN_STM_FLAG_BEGIN,
                mode: self.option.mode.as_u8(),
                transition_mode: self.option.transition_mode.as_u8(),
                freq_div: self.config.divide()?,
                rep: self.option.loop_behavior.rep(),
                transition_value: self.option.transition_mode.value(),
            };
            tx[..size_of::<GainStmHead>()].copy_from_slice(head.as_bytes());
        } else {
            let subseq = GainStmSubseq {
                tag: Tag::GainStm.as_u8(),
                flag,
            };
            tx[..size_of::<GainStmSubseq>()].copy_from_slice(subseq.as_bytes());
        }
        Ok(offset + body)
    }

    fn is_done(&self) -> bool {
        self.sent == self.patterns.len()
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU16;

    use autd3_rs_core::geometry::{Autd3, Geometry};
    use autd3_rs_core::value::{Intensity, Phase};

    use super::*;
    use crate::legacy::op::test_frames;
    use crate::legacy::wire::PAYLOAD_BYTES;

    fn geometry(n: usize) -> Geometry {
        Geometry::new((0..n).map(|_| Autd3::default()).collect())
    }

    fn config() -> SamplingConfig {
        SamplingConfig::new(NonZeroU16::new(0x4321).unwrap())
    }

    fn pattern(geo: &Geometry, base: u8) -> Vec<Vec<Emission>> {
        geo.iter()
            .map(|d| {
                (0..d.num_transducers())
                    .map(|i| Emission {
                        #[allow(clippy::cast_possible_truncation)]
                        phase: Phase(base.wrapping_add(i as u8)),
                        #[allow(clippy::cast_possible_truncation)]
                        intensity: Intensity(base.wrapping_mul(2).wrapping_add(i as u8)),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn phase_intensity_full_sends_one_pattern_per_frame() {
        let geo = geometry(1);
        let n = geo[0].num_transducers();
        let patterns = vec![pattern(&geo, 0x10), pattern(&geo, 0x20)];
        let mut op = GainStm::new(config(), &patterns, GainStmOption::default());
        assert_eq!(op.required_size(&geo[0]), 16 + 2 * n);

        let mut tx = vec![0u8; PAYLOAD_BYTES];
        let size = op.pack(&geo[0], &mut tx).unwrap();
        assert!(!op.is_done());
        assert_eq!(size, 16 + 2 * n);

        assert_eq!(tx[0], Tag::GainStm.as_u8());
        assert_eq!(tx[1], GAIN_STM_FLAG_BEGIN);
        assert_eq!(tx[2], GainStmMode::PhaseIntensityFull.as_u8());
        assert_eq!(tx[3], TransitionMode::Immediate.as_u8());
        assert_eq!(&tx[4..6], &0x4321u16.to_le_bytes());
        assert_eq!(&tx[6..8], &0xFFFFu16.to_le_bytes());
        for (i, chunk) in tx[16..16 + 2 * n].chunks_exact(2).enumerate() {
            assert_eq!(chunk[0], patterns[0][0][i].phase.0);
            assert_eq!(chunk[1], patterns[0][0][i].intensity.0);
        }

        let mut tx = vec![0u8; PAYLOAD_BYTES];
        let size = op.pack(&geo[0], &mut tx).unwrap();
        assert!(op.is_done());
        assert_eq!(size, 2 + 2 * n);
        assert_eq!(tx[1], GAIN_STM_FLAG_END | GAIN_STM_FLAG_TRANSITION);
    }

    #[test]
    fn phase_full_packs_two_patterns_into_the_high_and_low_byte() {
        let geo = geometry(1);
        let n = geo[0].num_transducers();
        let patterns = vec![pattern(&geo, 0x10), pattern(&geo, 0x90)];
        let mut op = GainStm::new(
            config(),
            &patterns,
            GainStmOption {
                mode: GainStmMode::PhaseFull,
                ..GainStmOption::default()
            },
        );
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        op.pack(&geo[0], &mut tx).unwrap();
        assert!(op.is_done());

        assert_eq!(
            tx[1],
            GAIN_STM_FLAG_BEGIN
                | GAIN_STM_FLAG_END
                | GAIN_STM_FLAG_TRANSITION
                | GAIN_STM_FLAG_SEND_BIT0
        );
        for (i, chunk) in tx[16..16 + 2 * n].chunks_exact(2).enumerate() {
            assert_eq!(chunk[0], patterns[0][0][i].phase.0);
            assert_eq!(chunk[1], patterns[1][0][i].phase.0);
        }
    }

    #[test]
    fn phase_half_packs_four_patterns_into_nibbles() {
        let geo = geometry(1);
        let n = geo[0].num_transducers();
        let patterns = (0..4)
            .map(|k| pattern(&geo, 0x10 * (k + 1)))
            .collect::<Vec<_>>();
        let mut op = GainStm::new(
            config(),
            &patterns,
            GainStmOption {
                mode: GainStmMode::PhaseHalf,
                segment: Segment::S1,
                ..GainStmOption::default()
            },
        );
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        op.pack(&geo[0], &mut tx).unwrap();
        assert!(op.is_done());

        assert_eq!(
            tx[1],
            GAIN_STM_FLAG_BEGIN
                | GAIN_STM_FLAG_END
                | GAIN_STM_FLAG_TRANSITION
                | GAIN_STM_FLAG_SEGMENT
                | GAIN_STM_FLAG_SEND_BIT0
                | GAIN_STM_FLAG_SEND_BIT1
        );
        for (i, chunk) in tx[16..16 + 2 * n].chunks_exact(2).enumerate() {
            let word = u16::from_le_bytes([chunk[0], chunk[1]]);
            for (k, pat) in patterns.iter().enumerate() {
                assert_eq!(
                    (word >> (4 * k)) & 0x0F,
                    u16::from(pat[0][i].phase.0 >> 4),
                    "nibble {k} of transducer {i}"
                );
            }
        }
    }

    #[test]
    fn phase_half_with_a_partial_round_reports_the_actual_count() {
        let geo = geometry(1);
        let patterns = (0..3)
            .map(|k| pattern(&geo, 0x10 * (k + 1)))
            .collect::<Vec<_>>();
        let mut op = GainStm::new(
            config(),
            &patterns,
            GainStmOption {
                mode: GainStmMode::PhaseHalf,
                ..GainStmOption::default()
            },
        );
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        op.pack(&geo[0], &mut tx).unwrap();
        assert!(op.is_done());
        assert_eq!(tx[1] & GAIN_STM_FLAG_SEND_BIT0, 0);
        assert_eq!(tx[1] & GAIN_STM_FLAG_SEND_BIT1, GAIN_STM_FLAG_SEND_BIT1);
    }

    #[test]
    fn multi_frame_split_covers_every_pattern_once() {
        let geo = geometry(2);
        let patterns = (0..5).map(|k| pattern(&geo, 0x10 * k)).collect::<Vec<_>>();
        let frames = test_frames(
            &geo,
            GainStm::new(config(), &patterns, GainStmOption::default()),
        )
        .unwrap();
        assert_eq!(frames.len(), 5);

        for device in &geo {
            for (round, expected) in patterns.iter().enumerate() {
                let frame = frames.frame(round).unwrap();
                let payload = &frame.frames()[device.idx()].payload;
                let offset = if round == 0 { 16 } else { 2 };
                for (i, chunk) in payload[offset..offset + 2 * device.num_transducers()]
                    .chunks_exact(2)
                    .enumerate()
                {
                    assert_eq!(chunk[0], expected[device.idx()][i].phase.0);
                    assert_eq!(chunk[1], expected[device.idx()][i].intensity.0);
                }
            }
        }
    }

    #[test]
    fn size_out_of_range_is_rejected() {
        let geo = geometry(1);
        let patterns = vec![pattern(&geo, 0)];
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        let err = GainStm::new(config(), &patterns, GainStmOption::default())
            .pack(&geo[0], &mut tx)
            .unwrap_err();
        assert!(matches!(
            err,
            LegacyError::InvalidPayload(PayloadError::GainStmSizeOutOfRange { size: 1, .. })
        ));
    }
}
